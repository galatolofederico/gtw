//! GTW's folders, and the one-time move from FableFlight's.
//!
//! Everything GTW keeps between runs lives under `<config>/gtw/` and
//! `<cache>/gtw/`. FableFlight used `<config>/fablegame/` and
//! `<cache>/fablegame/`: the first time GTW starts, the known config files are
//! copied across, and the cache folder is renamed when it can be. The
//! migration runs once; later starts leave both folders alone.
//!
//! Two names are deliberately kept from upstream:
//! - `<data>/fablegame/` (installed aircraft and AMD's FSR runtime), because
//!   moving it would orphan files the player has already downloaded;
//! - the `FABLE_*` environment variables, which players and scripts already
//!   set (see the README).
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The folder name under `<config>` and `<cache>`.
pub const NAME: &str = "gtw";
/// The folder name FableFlight used, read once for the migration.
pub const LEGACY_NAME: &str = "fablegame";

/// The files copied from the old config folder, when GTW's own is missing.
pub const CONFIG_FILES: [&str; 5] = ["settings.json", "bindings.json", "graphics.json", "google_maps.json", "launcher.json"];

/// Says the one-time import has happened, so it never runs again.
const MARKER: &str = ".migrated-from-fableflight";

/// What the start-up move did, for the notice and for tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Migration {
    /// the files copied from `<config>/fablegame/`
    pub copied: Vec<String>,
    pub cache: Cache,
}

/// Where this run's cache came from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cache {
    /// `<cache>/gtw` was already there, or is new.
    #[default]
    Existing,
    /// `<cache>/fablegame` was renamed to `<cache>/gtw`.
    Renamed,
    /// The rename failed: this run reads the old folder.
    Legacy,
}

struct Roots {
    config: Option<PathBuf>,
    cache: Option<PathBuf>,
    migration: Migration,
}

static ROOTS: OnceLock<Roots> = OnceLock::new();

/// Does the one-time move (if it is still to do) and returns its report.
/// Safe to call more than once; the work happens once per process.
pub fn prepare() -> &'static Migration {
    &roots().migration
}

/// What the start-up move did.
pub fn migration() -> &'static Migration {
    prepare()
}

/// `<config>/gtw`, or None when the platform has no config folder.
pub fn config() -> Option<PathBuf> {
    roots().config.clone()
}

/// `<cache>/gtw` (or the old cache for this run when it could not be renamed).
pub fn cache() -> Option<PathBuf> {
    roots().cache.clone()
}

/// `<data>/fablegame`, the historical data folder (installed aircraft, FSR).
pub fn data() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(LEGACY_NAME))
}

/// FableFlight's cache, when it is still there beside GTW's own. The fetcher
/// reads tiles from it that the new cache lacks or got wrong, so a cache that
/// was already present before the move does not lose the player's tiles.
pub fn legacy_cache() -> Option<PathBuf> {
    let current = cache()?;
    let legacy = dirs::cache_dir()?.join(LEGACY_NAME);
    (legacy.is_dir() && legacy != current).then_some(legacy)
}

/// `<base>/gtw`, for callers that already hold the platform's base folder.
pub fn config_root(base: &Path) -> PathBuf {
    base.join(NAME)
}

/// `<base>/gtw`, for callers that already hold the platform's base folder.
pub fn cache_root(base: &Path) -> PathBuf {
    base.join(NAME)
}

fn roots() -> &'static Roots {
    ROOTS.get_or_init(|| {
        let config_base = dirs::config_dir();
        let cache_base = dirs::cache_dir();
        let (config, copied) = match config_base.as_deref() {
            Some(base) => (Some(config_root(base)), migrate_config(base)),
            None => (None, Vec::new()),
        };
        let (cache, state) = match cache_base.as_deref() {
            Some(base) => {
                let (root, state) = cache_root_in(base);
                (Some(root), state)
            }
            None => (None, Cache::Existing),
        };
        if state == Cache::Legacy {
            log::warn!("the FableFlight tile cache could not be renamed; reading {} for this run", cache.as_ref().map_or_else(|| ".".into(), |p| p.display().to_string()));
        }
        if !copied.is_empty() {
            log::info!("imported {} file(s) from the FableFlight settings folder", copied.len());
        }
        Roots { config, cache, migration: Migration { copied, cache: state } }
    })
}

/// Copies the known settings files from `<base>/fablegame/` to `<base>/gtw/`.
///
/// A file already in the GTW folder is left alone; a file in neither place is
/// skipped. Once anything has been copied, a marker stops a later start from
/// importing a file the player has since deleted from the GTW folder.
pub fn migrate_config(base: &Path) -> Vec<String> {
    let root = config_root(base);
    let legacy = base.join(LEGACY_NAME);
    if root.join(MARKER).exists() || !legacy.is_dir() {
        return Vec::new();
    }
    let mut copied = Vec::new();
    for name in CONFIG_FILES {
        let from = legacy.join(name);
        let to = root.join(name);
        if to.exists() || !from.is_file() {
            continue;
        }
        if let Err(e) = std::fs::create_dir_all(&root) {
            log::warn!("creating the GTW config folder {}: {e}", root.display());
            return copied;
        }
        match std::fs::copy(&from, &to) {
            Ok(_) => copied.push(name.to_string()),
            Err(e) => log::warn!("importing {} from FableFlight: {e}", from.display()),
        }
    }
    if !copied.is_empty() && std::fs::create_dir_all(&root).is_ok() {
        let _ = std::fs::write(root.join(MARKER), b"GTW imported the FableFlight settings once.\n");
    }
    copied
}

/// The cache folder for `base`: `<base>/gtw` when it is there or can be made
/// by renaming `<base>/fablegame`, else the old folder for this run.
pub fn cache_root_in(base: &Path) -> (PathBuf, Cache) {
    cache_root_using(base, |from, to| std::fs::rename(from, to))
}

fn cache_root_using(base: &Path, rename: impl Fn(&Path, &Path) -> std::io::Result<()>) -> (PathBuf, Cache) {
    let root = cache_root(base);
    if root.exists() {
        return (root, Cache::Existing);
    }
    let legacy = base.join(LEGACY_NAME);
    if !legacy.is_dir() {
        return (root, Cache::Existing);
    }
    match rename(&legacy, &root) {
        Ok(()) => (root, Cache::Renamed),
        Err(_) => (legacy, Cache::Legacy),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A fresh folder under the system temp folder, removed with the test.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let p = std::env::temp_dir().join(format!("gtw-paths-{}-{}-{name}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    #[test]
    fn roots_are_gtw_under_the_platform_folders() {
        let base = Path::new("/tmp/example");
        assert_eq!(config_root(base), base.join("gtw"));
        assert_eq!(cache_root(base), base.join("gtw"));
        assert_eq!(NAME, "gtw");
    }

    #[test]
    fn copies_each_known_file_once() {
        let d = TempDir::new("copy");
        for (i, name) in CONFIG_FILES.iter().enumerate() {
            write(&d.0.join(LEGACY_NAME).join(name), &format!("old {i}"));
        }
        let copied = migrate_config(&d.0);
        assert_eq!(copied, CONFIG_FILES.map(str::to_string));
        for (i, name) in CONFIG_FILES.iter().enumerate() {
            assert_eq!(std::fs::read_to_string(d.0.join(NAME).join(name)).unwrap(), format!("old {i}"));
        }
        // The marker keeps it to one time, even if GTW's copy is deleted.
        std::fs::remove_file(d.0.join(NAME).join("settings.json")).unwrap();
        assert!(migrate_config(&d.0).is_empty());
        assert!(!d.0.join(NAME).join("settings.json").exists());
    }

    #[test]
    fn a_gtw_file_wins_over_the_old_one_and_the_other_files_still_copy() {
        let d = TempDir::new("both");
        write(&d.0.join(LEGACY_NAME).join("settings.json"), "old settings");
        write(&d.0.join(LEGACY_NAME).join("bindings.json"), "old bindings");
        write(&d.0.join(NAME).join("settings.json"), "new settings");
        let copied = migrate_config(&d.0);
        assert_eq!(copied, vec!["bindings.json".to_string()]);
        assert_eq!(std::fs::read_to_string(d.0.join(NAME).join("settings.json")).unwrap(), "new settings");
        assert_eq!(std::fs::read_to_string(d.0.join(NAME).join("bindings.json")).unwrap(), "old bindings");
    }

    #[test]
    fn neither_folder_copies_nothing_and_makes_nothing() {
        let d = TempDir::new("neither");
        assert!(migrate_config(&d.0).is_empty());
        assert!(!d.0.join(NAME).exists());
        assert!(!d.0.join(MARKER).exists());
    }

    #[test]
    fn an_empty_old_folder_copies_nothing() {
        let d = TempDir::new("empty-old");
        std::fs::create_dir_all(d.0.join(LEGACY_NAME)).unwrap();
        assert!(migrate_config(&d.0).is_empty());
        // Nothing to copy: GTW's folder is not created either.
        assert!(!d.0.join(NAME).exists());
        for name in CONFIG_FILES {
            assert!(!d.0.join(NAME).join(name).exists());
        }
    }

    #[test]
    fn cache_is_renamed_when_it_can_be() {
        let d = TempDir::new("cache-rename");
        write(&d.0.join(LEGACY_NAME).join("lc/14/1/1.png"), "tile");
        let (root, state) = cache_root_in(&d.0);
        assert_eq!((root.clone(), state), (d.0.join(NAME), Cache::Renamed));
        assert_eq!(std::fs::read_to_string(root.join("lc/14/1/1.png")).unwrap(), "tile");
        assert!(!d.0.join(LEGACY_NAME).exists());
        // The new folder is used from then on.
        assert_eq!(cache_root_in(&d.0), (d.0.join(NAME), Cache::Existing));
    }

    #[test]
    fn cache_falls_back_to_the_old_folder_when_the_rename_fails() {
        let d = TempDir::new("cache-fallback");
        write(&d.0.join(LEGACY_NAME).join("lc/14/1/1.png"), "tile");
        let (root, state) = cache_root_using(&d.0, |_, _| Err(std::io::Error::other("busy")));
        assert_eq!((root.clone(), state), (d.0.join(LEGACY_NAME), Cache::Legacy));
        assert!(root.join("lc/14/1/1.png").exists());
    }

    #[test]
    fn no_cache_at_all_leaves_the_new_folder_for_later() {
        let d = TempDir::new("cache-none");
        assert_eq!(cache_root_in(&d.0), (d.0.join(NAME), Cache::Existing));
        assert!(!d.0.join(NAME).exists());
    }
}
