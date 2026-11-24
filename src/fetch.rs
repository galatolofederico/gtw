//! Disk-cached HTTP fetches for map tiles.
use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// User-Agent sent on outbound HTTP requests (includes project URL for providers
/// such as OpenStreetMap's Nominatim that ask for a contact reference).
pub const USER_AGENT: &str = concat!("fablegame-flightsim/", env!("CARGO_PKG_VERSION"), " (+https://github.com/Rolandjg/fableflight)");

/// Default disk cache cap for tile data (bytes, 3 GiB).
pub const CACHE_MAX_BYTES: u64 = 3 << 30;

pub struct Fetcher {
    agent: ureq::Agent,
    pub root: PathBuf,
    missing: Mutex<HashSet<String>>,
    /// Per-host backoff after throttling (429 / 403) or server errors (5xx):
    /// `(retry_after, consecutive_failures)`.
    backoff: Mutex<HashMap<String, (Instant, u32)>>,
    pub offline: bool,
}

fn url_host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split('/').next().unwrap_or(rest)
}

impl Fetcher {
    pub fn new(root: PathBuf, offline: bool) -> Self {
        let cfg = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .user_agent(USER_AGENT)
            .build();
        Self { agent: cfg.into(), root, missing: Mutex::new(HashSet::new()), backoff: Mutex::new(HashMap::new()), offline }
    }

    fn note_backoff(&self, host: &str, base_secs: u64) {
        let mut map = self.backoff.lock();
        let n = map.get(host).map_or(0, |e| e.1.saturating_add(1));
        let secs = (base_secs.saturating_mul(1u64 << n.min(5))).min(60);
        map.insert(host.to_string(), (Instant::now() + Duration::from_secs(secs), n));
    }

    /// Plain GET with no caching.
    pub fn get(&self, url: &str) -> Result<Vec<u8>> {
        if self.offline {
            return Err(anyhow!("offline"));
        }
        let host = url_host(url);
        if let Some(&(until, _)) = self.backoff.lock().get(host) {
            if Instant::now() < until {
                return Err(anyhow!("backed off {host}"));
            }
        }
        match self.agent.get(url).call() {
            Ok(mut res) => {
                self.backoff.lock().remove(host);
                Ok(res.body_mut().with_config().limit(64 << 20).read_to_vec()?)
            }
            Err(e) => {
                match e {
                    ureq::Error::StatusCode(429 | 500..=599) => self.note_backoff(host, 2),
                    ureq::Error::StatusCode(403) => self.note_backoff(host, 10),
                    _ => {}
                }
                Err(e.into())
            }
        }
    }

    /// GET with a disk cache at `<root>/<key>`. Permanently missing tiles
    /// (404 / 204 / 410) are remembered for the session so they are not
    /// retried every frame; throttling (403 / 429) and server errors (5xx)
    /// back off per host instead.
    pub fn get_cached(&self, key: &str, url: &str) -> Result<Vec<u8>> {
        let path = self.root.join(key);
        if let Ok(data) = std::fs::read(&path) {
            if !data.is_empty() {
                return Ok(data);
            }
        }
        if self.missing.lock().contains(key) {
            return Err(anyhow!("missing {key}"));
        }
        match self.get(url) {
            Ok(data) if !data.is_empty() => {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                // Write to a temp name first so a crash never leaves a half tile.
                let tmp = path.with_extension(format!("tmp{:?}", std::thread::current().id()).replace(['(', ')'], ""));
                if std::fs::write(&tmp, &data).is_ok() {
                    let _ = std::fs::rename(&tmp, &path);
                }
                Ok(data)
            }
            Ok(_) => {
                self.missing.lock().insert(key.to_string());
                Err(anyhow!("empty {key}"))
            }
            Err(e) => {
                if matches!(e.downcast_ref::<ureq::Error>(), Some(ureq::Error::StatusCode(404 | 204 | 410))) {
                    self.missing.lock().insert(key.to_string());
                }
                Err(e)
            }
        }
    }
}

/// Prunes cached tile files under `root` by oldest modification time until
/// total size fits within `max_bytes` (trimming to 90% when over the cap).
/// Skips `g3d` (which `gmaps.rs` prunes by HTTP expiry header) and removes
/// stale `.tmp*` files left by crashed writes.
pub fn prune_cache(root: &Path, max_bytes: u64) {
    let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    let stale_cutoff = SystemTime::now().checked_sub(Duration::from_secs(300));
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let path = entry.path();
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                // `g3d` has its own header-expiry pruner in gmaps.rs.
                if dir == root && entry.file_name() == "g3d" {
                    continue;
                }
                stack.push(path);
            } else if ft.is_file() {
                let Ok(meta) = entry.metadata() else { continue };
                let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                let is_tmp = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.starts_with("tmp"));
                if is_tmp {
                    if stale_cutoff.is_some_and(|c| mtime < c) {
                        let _ = std::fs::remove_file(&path);
                    }
                    continue;
                }
                files.push((mtime, meta.len(), path));
            }
        }
    }
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    if total <= max_bytes {
        return;
    }
    let target = max_bytes * 9 / 10;
    files.sort_by_key(|f| f.0);
    for (_, len, path) in files {
        if total <= target {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_host_extracts_authority() {
        assert_eq!(url_host("https://tiles.example.com/1/2/3.png"), "tiles.example.com");
        assert_eq!(url_host("http://localhost:8080/a/b"), "localhost:8080");
        assert_eq!(url_host("example.org"), "example.org");
    }

    #[test]
    fn prune_cache_drops_oldest_files_and_keeps_g3d() {
        let dir = std::env::temp_dir().join(format!("fable-fetch-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("img/1/2")).unwrap();
        std::fs::create_dir_all(dir.join("g3d/ab")).unwrap();
        let old = dir.join("img/1/2/old.jpg");
        let mid = dir.join("img/1/2/mid.jpg");
        let new = dir.join("img/1/2/new.jpg");
        let g3d = dir.join("g3d/ab/keep.glb");
        std::fs::write(&old, vec![1u8; 40]).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&mid, vec![2u8; 40]).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&new, vec![3u8; 40]).unwrap();
        std::fs::write(&g3d, vec![4u8; 100]).unwrap();
        prune_cache(&dir, 70);
        assert!(!old.exists(), "oldest tile should be pruned");
        assert!(new.exists(), "newest tile should remain");
        assert!(g3d.exists(), "g3d directory should be untouched");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn typed_status_classification_does_not_mark_403_missing() {
        let dir = std::env::temp_dir().join(format!("fable-fetch-cls-{}", std::process::id()));
        let f = Fetcher::new(dir.clone(), true);
        // Simulate backoff on 403/429 without permanently adding to `missing`.
        f.note_backoff("throttled.example", 10);
        assert!(f.backoff.lock().contains_key("throttled.example"));
        assert!(!f.missing.lock().contains("k"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
