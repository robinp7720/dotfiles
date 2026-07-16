use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, anyhow, bail};
use gtk4::glib;

const MAX_ARTWORK_BYTES: u64 = 8 * 1024 * 1024;
const MAX_CACHE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CACHE_FILES: usize = 64;
const ARTWORK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct ArtworkRequest {
    pub uri: String,
    pub generation: u64,
}

#[derive(Debug)]
pub struct ArtworkResult {
    pub uri: String,
    pub generation: u64,
    pub bytes: Result<Vec<u8>, String>,
}

pub fn result_is_current(current_generation: u64, result_generation: u64) -> bool {
    current_generation == result_generation
}

pub fn prefer_artwork_candidate(best_pixels: u64, candidate_pixels: u64) -> bool {
    candidate_pixels > best_pixels
}

pub fn spawn_artwork_loader() -> (Sender<ArtworkRequest>, Receiver<ArtworkResult>) {
    let (request_tx, request_rx) = mpsc::channel::<ArtworkRequest>();
    let (result_tx, result_rx) = mpsc::channel();
    let cache_root = dirs::cache_dir().map(|root| root.join("cockpit-bar/artwork"));

    thread::spawn(move || {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(ARTWORK_TIMEOUT))
            .https_only(true)
            .build();
        let agent: ureq::Agent = config.into();

        while let Ok(request) = request_rx.recv() {
            let result = load_artwork(&request.uri, cache_root.as_deref(), |remote_uri| {
                let mut response = agent
                    .get(remote_uri)
                    .call()
                    .with_context(|| format!("failed to fetch artwork from {remote_uri}"))?;
                response
                    .body_mut()
                    .with_config()
                    .limit(MAX_ARTWORK_BYTES)
                    .read_to_vec()
                    .context("failed to read artwork response")
            })
            .map_err(|error| error.to_string());

            if result_tx
                .send(ArtworkResult {
                    uri: request.uri,
                    generation: request.generation,
                    bytes: result,
                })
                .is_err()
            {
                break;
            }
        }
    });

    (request_tx, result_rx)
}

fn load_artwork<F>(uri: &str, cache_root: Option<&Path>, fetch_remote: F) -> Result<Vec<u8>>
where
    F: FnOnce(&str) -> Result<Vec<u8>>,
{
    if uri.starts_with("file://") {
        let (path, hostname) = glib::filename_from_uri(uri).context("invalid artwork file URI")?;
        if hostname.is_some() {
            bail!("remote file artwork URIs are not supported");
        }
        return read_limited(&path);
    }

    if !uri.starts_with("https://") {
        bail!("unsupported artwork URI scheme");
    }

    let Some(cache_root) = cache_root else {
        let bytes = fetch_remote(uri)?;
        ensure_size(&bytes)?;
        return Ok(bytes);
    };

    let cache_path = cache_root.join(cache_key(uri));
    if cache_path.is_file() {
        return read_limited(&cache_path);
    }

    let bytes = fetch_remote(uri)?;
    ensure_size(&bytes)?;
    fs::create_dir_all(cache_root)
        .with_context(|| format!("failed to create {}", cache_root.display()))?;
    write_atomically(&cache_path, &bytes)?;
    prune_cache(cache_root)?;
    Ok(bytes)
}

fn read_limited(path: &Path) -> Result<Vec<u8>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(MAX_ARTWORK_BYTES + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read {}", path.display()))?;
    ensure_size(&bytes)?;
    Ok(bytes)
}

fn ensure_size(bytes: &[u8]) -> Result<()> {
    if bytes.len() as u64 > MAX_ARTWORK_BYTES {
        bail!("artwork exceeds the 8 MiB limit");
    }
    if bytes.is_empty() {
        bail!("artwork is empty");
    }
    Ok(())
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("artwork cache path has no file name"))?;
    let temporary = path.with_file_name(format!(".{file_name}.{}.part", std::process::id()));
    fs::write(&temporary, bytes)
        .with_context(|| format!("failed to write {}", temporary.display()))?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error).with_context(|| format!("failed to install {}", path.display()));
    }
    Ok(())
}

fn prune_cache(cache_root: &Path) -> Result<()> {
    let mut entries = fs::read_dir(cache_root)
        .with_context(|| format!("failed to read {}", cache_root.display()))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            metadata.is_file().then(|| CacheEntry {
                path: entry.path(),
                bytes: metadata.len(),
                modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.modified);

    let mut total_bytes = entries.iter().map(|entry| entry.bytes).sum::<u64>();
    let mut total_files = entries.len();
    for entry in entries {
        if total_files <= MAX_CACHE_FILES && total_bytes <= MAX_CACHE_BYTES {
            break;
        }
        if fs::remove_file(&entry.path).is_ok() {
            total_files = total_files.saturating_sub(1);
            total_bytes = total_bytes.saturating_sub(entry.bytes);
        }
    }
    Ok(())
}

fn cache_key(uri: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in uri.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

struct CacheEntry {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        MAX_ARTWORK_BYTES, MAX_CACHE_FILES, cache_key, load_artwork, prefer_artwork_candidate,
        prune_cache, result_is_current,
    };

    #[test]
    fn cache_key_is_stable_and_uri_specific() {
        assert_eq!(
            cache_key("https://example.test/art.jpg"),
            "7adf2053ac8c2119"
        );
        assert_ne!(
            cache_key("https://example.test/art.jpg"),
            cache_key("https://example.test/other.jpg")
        );
    }

    #[test]
    fn stale_results_do_not_match_the_current_artwork_request() {
        assert!(result_is_current(4, 4));
        assert!(!result_is_current(4, 3));
    }

    #[test]
    fn artwork_only_upgrades_to_a_larger_pixel_area() {
        assert!(prefer_artwork_candidate(0, 160_000));
        assert!(prefer_artwork_candidate(160_000, 1_000_000));
        assert!(!prefer_artwork_candidate(1_000_000, 160_000));
        assert!(!prefer_artwork_candidate(1_000_000, 1_000_000));
    }

    #[test]
    fn remote_artwork_is_cached_after_the_first_fetch() {
        let root = temp_dir("remote-cache");
        let calls = Cell::new(0);
        let fetch = |_: &str| {
            calls.set(calls.get() + 1);
            Ok(vec![1, 2, 3, 4])
        };

        assert_eq!(
            load_artwork("https://example.test/art.jpg", Some(&root), fetch).unwrap(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(
            load_artwork("https://example.test/art.jpg", Some(&root), |_| {
                panic!("cache hit must not fetch")
            })
            .unwrap(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(calls.get(), 1);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn unsupported_and_oversized_artwork_is_rejected() {
        let root = temp_dir("rejected");
        assert!(load_artwork("http://example.test/art.jpg", Some(&root), |_| Ok(vec![1])).is_err());
        assert!(
            load_artwork("https://example.test/art.jpg", Some(&root), |_| {
                Ok(vec![0; MAX_ARTWORK_BYTES as usize + 1])
            })
            .is_err()
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn file_artwork_decodes_escaped_paths_without_networking() {
        let root = temp_dir("local-file");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("album art.jpg");
        fs::write(&path, [4, 3, 2, 1]).unwrap();
        let uri = format!("file://{}", path.display().to_string().replace(' ', "%20"));

        assert_eq!(
            load_artwork(&uri, None, |_| panic!("local artwork must not fetch")).unwrap(),
            vec![4, 3, 2, 1]
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn cache_prunes_old_entries_to_the_file_limit() {
        let root = temp_dir("prune");
        fs::create_dir_all(&root).unwrap();
        for index in 0..MAX_CACHE_FILES + 3 {
            fs::write(root.join(format!("entry-{index:03}")), [index as u8]).unwrap();
        }

        prune_cache(&root).unwrap();

        assert!(fs::read_dir(&root).unwrap().count() <= MAX_CACHE_FILES);
        fs::remove_dir_all(root).ok();
    }

    fn temp_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("cockpit-bar-artwork-{label}-{unique}"))
    }

    use std::path::PathBuf;
}
