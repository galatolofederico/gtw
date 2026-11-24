//! Start-up progress for the launcher (src/bin/gtw-launcher). With
//! `FABLE_PROGRESS` set, each stage of the start is printed on stdout as a
//! line `@fable-progress <stage>`; the launcher times them against the last
//! start to draw its progress bar. Without it nothing is printed.
//!
//! The stages, in order: `boot` (main), `window`, `gpu` (adapter and device),
//! `shaders` (the pipelines start compiling, most of a cold start), four
//! `shaders-part` (each batch done), `shaders-done`, `aircraft` (the first
//! aircraft built), `world` (the game set up) and `ready` (the first frame).
use std::io::Write;
use std::sync::OnceLock;

fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("FABLE_PROGRESS").is_some())
}

/// Reports a stage of the start (a no-op unless the launcher asked).
pub fn step(stage: &str) {
    if on() {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "@fable-progress {stage}");
        let _ = out.flush();
    }
}
