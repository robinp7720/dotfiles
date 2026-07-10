use std::fs;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::{ResourceState, SourceHealth, SourceId, StateUpdate, SystemUpdate};

use super::SourceSupervisor;

const PROC_STAT_PATH: &str = "/proc/stat";
const PROC_MEMINFO_PATH: &str = "/proc/meminfo";
const RESOURCE_POLL_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuTotals {
    pub total: u64,
    pub idle: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcSample {
    pub state: ResourceState,
    pub cpu_totals: CpuTotals,
}

pub fn spawn_resource_source(
    sender: Sender<StateUpdate>,
    cancelled: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    let mut previous = None;

    SourceSupervisor::spawn(cancelled.clone(), RESOURCE_POLL_INTERVAL, move || {
        match publish_resource_snapshot(&sender, &cancelled, &mut previous) {
            Ok(()) => Ok(true),
            Err(error) => {
                let _ = sender.send(StateUpdate::Health {
                    source: SourceId::Resources,
                    health: SourceHealth::Disconnected {
                        message: error.to_string(),
                    },
                });
                Err(error)
            }
        }
    })
}

pub fn read_proc_sample(
    proc_stat: &str,
    proc_meminfo: &str,
    previous: Option<CpuTotals>,
) -> Result<ProcSample> {
    let cpu_totals = parse_cpu_totals(proc_stat)?;
    let memory_percent = parse_memory_percent(proc_meminfo)?;
    let cpu_percent = previous.and_then(|previous| cpu_percent(cpu_totals, previous));

    Ok(ProcSample {
        state: ResourceState {
            cpu_percent,
            memory_percent: Some(memory_percent),
        },
        cpu_totals,
    })
}

fn publish_resource_snapshot(
    sender: &Sender<StateUpdate>,
    cancelled: &Arc<AtomicBool>,
    previous: &mut Option<CpuTotals>,
) -> Result<()> {
    let proc_stat = fs::read_to_string(PROC_STAT_PATH)
        .with_context(|| format!("failed to read {PROC_STAT_PATH}"))?;
    let proc_meminfo = fs::read_to_string(PROC_MEMINFO_PATH)
        .with_context(|| format!("failed to read {PROC_MEMINFO_PATH}"))?;
    let sample = read_proc_sample(&proc_stat, &proc_meminfo, *previous)?;
    *previous = Some(sample.cpu_totals);

    if sender
        .send(StateUpdate::System(SystemUpdate::Resources(sample.state)))
        .is_err()
    {
        cancelled.store(true, Ordering::Relaxed);
    }

    Ok(())
}

fn parse_cpu_totals(proc_stat: &str) -> Result<CpuTotals> {
    let Some(line) = proc_stat.lines().find(|line| line.starts_with("cpu ")) else {
        bail!("missing aggregate cpu line in /proc/stat");
    };

    let mut fields = line.split_whitespace();
    let _cpu_label = fields.next();
    let values = fields
        .map(|field| {
            field
                .parse::<u64>()
                .with_context(|| format!("failed to parse cpu field '{field}'"))
        })
        .collect::<Result<Vec<_>>>()?;

    if values.len() < 4 {
        bail!("aggregate cpu line must contain at least four columns");
    }

    let total = values.iter().copied().sum();
    let idle = values[3].saturating_add(*values.get(4).unwrap_or(&0));

    Ok(CpuTotals { total, idle })
}

fn parse_memory_percent(proc_meminfo: &str) -> Result<u8> {
    let mut total_kib = None;
    let mut available_kib = None;

    for line in proc_meminfo.lines() {
        if let Some(value) = parse_meminfo_kib(line, "MemTotal:")? {
            total_kib = Some(value);
        } else if let Some(value) = parse_meminfo_kib(line, "MemAvailable:")? {
            available_kib = Some(value);
        }
    }

    let total_kib = total_kib.context("missing MemTotal in /proc/meminfo")?;
    let available_kib = available_kib.context("missing MemAvailable in /proc/meminfo")?;
    if total_kib == 0 {
        bail!("MemTotal must be greater than zero");
    }

    let used_kib = total_kib.saturating_sub(available_kib);
    let percent = ((used_kib * 100) + (total_kib / 2)) / total_kib;
    Ok(u8::try_from(percent.min(100)).unwrap_or(100))
}

fn parse_meminfo_kib(line: &str, key: &str) -> Result<Option<u64>> {
    if !line.starts_with(key) {
        return Ok(None);
    }

    let value = line[key.len()..]
        .split_whitespace()
        .next()
        .context("missing meminfo value")?;
    let parsed = value
        .parse::<u64>()
        .with_context(|| format!("failed to parse meminfo value '{value}'"))?;
    Ok(Some(parsed))
}

fn cpu_percent(current: CpuTotals, previous: CpuTotals) -> Option<u8> {
    let total_delta = current.total.checked_sub(previous.total)?;
    let idle_delta = current.idle.checked_sub(previous.idle)?;
    if total_delta == 0 {
        return None;
    }

    let busy_delta = total_delta.saturating_sub(idle_delta);
    let percent = ((busy_delta * 100) + (total_delta / 2)) / total_delta;
    Some(u8::try_from(percent.min(100)).unwrap_or(100))
}

#[cfg(test)]
mod tests {
    use crate::ResourceState;

    use super::read_proc_sample;

    const PROC_STAT_FIRST: &str = "\
cpu  100 0 100 800 0 0 0 0 0 0
cpu0 50 0 50 400 0 0 0 0 0 0
";

    const PROC_STAT_SECOND: &str = "\
cpu  135 0 135 850 0 0 0 0 0 0
cpu0 67 0 67 425 0 0 0 0 0 0
";

    const MEMINFO_SAMPLE: &str = "\
MemTotal:       8000000 kB
MemFree:        1000000 kB
MemAvailable:   3000000 kB
Buffers:         500000 kB
Cached:         1000000 kB
";

    #[test]
    fn first_proc_sample_has_no_cpu_delta_but_reports_memory_percent() {
        let sample = read_proc_sample(PROC_STAT_FIRST, MEMINFO_SAMPLE, None).unwrap();

        assert_eq!(
            sample.state,
            ResourceState {
                cpu_percent: None,
                memory_percent: Some(63),
            }
        );
        assert_eq!(sample.cpu_totals.total, 1_000);
        assert_eq!(sample.cpu_totals.idle, 800);
    }

    #[test]
    fn second_proc_sample_uses_cpu_delta_percent() {
        let first = read_proc_sample(PROC_STAT_FIRST, MEMINFO_SAMPLE, None).unwrap();
        let second =
            read_proc_sample(PROC_STAT_SECOND, MEMINFO_SAMPLE, Some(first.cpu_totals)).unwrap();

        assert_eq!(
            second.state,
            ResourceState {
                cpu_percent: Some(58),
                memory_percent: Some(63),
            }
        );
    }
}
