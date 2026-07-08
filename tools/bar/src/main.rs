use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, anyhow, bail};
use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "cockpit-bar")]
#[command(about = "Wayland command cockpit bar")]
struct Cli {
    #[arg(long, value_name = "PATH", default_value_os_t = default_config_path())]
    config: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    Timer {
        #[command(subcommand)]
        command: TimerCommand,
    },
    #[command(name = "__test_control_server", hide = true)]
    TestControlServer {
        #[arg(long, default_value_t = 1)]
        requests: usize,
    },
}

#[derive(Debug, clap::Subcommand)]
enum TimerCommand {
    Start {
        duration: String,
        #[arg(long)]
        label: String,
    },
    Pause {
        id: String,
    },
    Resume {
        id: String,
    },
    Cancel {
        id: String,
    },
    List,
}

fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("cockpit-bar")
        .join("config.toml")
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .without_time()
        .init();

    let cli = Cli::parse();
    match run_cli(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run_cli(cli: Cli) -> Result<()> {
    match cli.command {
        None => cockpit_bar::run(&cli.config),
        Some(Command::Timer { command }) => {
            let request = timer_request(command)?;
            let response = cockpit_bar::ControlClient::new()?.send(&request)?;
            match response {
                cockpit_bar::ControlResponse::Error { message } => Err(anyhow!(message)),
                other => {
                    println!("{}", serde_json::to_string(&other)?);
                    Ok(())
                }
            }
        }
        Some(Command::TestControlServer { requests }) => {
            cockpit_bar::run_test_control_server(requests)
        }
    }
}

fn timer_request(command: TimerCommand) -> Result<cockpit_bar::ControlRequest> {
    Ok(match command {
        TimerCommand::Start { duration, label } => cockpit_bar::ControlRequest::TimerStart {
            label,
            duration_seconds: parse_duration_seconds(&duration)?,
        },
        TimerCommand::Pause { id } => cockpit_bar::ControlRequest::TimerPause { id },
        TimerCommand::Resume { id } => cockpit_bar::ControlRequest::TimerResume { id },
        TimerCommand::Cancel { id } => cockpit_bar::ControlRequest::TimerCancel { id },
        TimerCommand::List => cockpit_bar::ControlRequest::TimerList,
    })
}

fn parse_duration_seconds(text: &str) -> Result<u64> {
    if text.is_empty() {
        bail!("duration must not be empty");
    }

    let (number, multiplier) = match text.chars().last().expect("non-empty duration") {
        's' => (&text[..text.len() - 1], 1_u64),
        'm' => (&text[..text.len() - 1], 60_u64),
        'h' => (&text[..text.len() - 1], 60_u64 * 60),
        'd' => (&text[..text.len() - 1], 60_u64 * 60 * 24),
        _ => (text, 1_u64),
    };

    if number.is_empty() {
        bail!("duration is missing its numeric value");
    }

    let value = number
        .parse::<u64>()
        .map_err(|_| anyhow!("invalid duration: {text}"))?;
    if value == 0 {
        bail!("duration must be greater than zero");
    }

    value
        .checked_mul(multiplier)
        .ok_or_else(|| anyhow!("duration is too large: {text}"))
}
