//! GTW smoke tests: the on-foot start really reaches the game loop headless,
//! without a window or the network, and the CLI carries the ground flags.
//!
//! The binary is still `fablegame` until the phase 7 rebrand, so the Cargo
//! `CARGO_BIN_EXE_*` name is the old one on purpose.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn bin() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_fablegame"));
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    c
}

/// Run a command, killing it if it outlives `limit`; returns (ok, output).
fn run_with_limit(mut c: Command, limit: Duration) -> (bool, String) {
    let mut child = match c.spawn() {
        Ok(child) => child,
        Err(e) => return (false, format!("could not start: {e}")),
    };
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let start = Instant::now();
    let mut timed_out = false;
    let code = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s.code(),
            Ok(None) if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                timed_out = true;
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return (false, format!("waiting for the process: {e}")),
        }
    };
    use std::io::Read;
    let mut text = String::new();
    if let Some(o) = &mut stdout {
        let _ = o.read_to_string(&mut text);
    }
    if let Some(e) = &mut stderr {
        let _ = e.read_to_string(&mut text);
    }
    (code == Some(0) && !timed_out, format!("exit {code:?}\n{text}"))
}

#[test]
fn help_lists_the_ground_flags() {
    let mut c = bin();
    c.arg("--help");
    let (ok, text) = run_with_limit(c, Duration::from_secs(30));
    assert!(ok, "--help failed: {text}");
    assert!(text.contains("--start foot|car|plane"), "usage names --start: {text}");
    assert!(text.contains("--flat"), "usage names --flat: {text}");
}

#[test]
fn starts_on_foot_headless() {
    // Flat ground and no network: the character starts at the default place,
    // settles on the ground and the loop runs for the hold's seconds.
    let mut c = bin();
    c.args(["--headless", "--flat", "--start", "foot", "--hold", "2", "--mute", "--weather", "clear"]);
    let (ok, text) = run_with_limit(c, Duration::from_secs(120));
    assert!(ok, "headless foot run failed: {text}");
    assert!(!text.to_lowercase().contains("panicked"), "no panic: {text}");
}

#[test]
fn starts_in_a_car_headless() {
    // The same flat world, but the start puts a sedan under the player: the
    // car physics runs, the HUD draws and the loop stays panic-free.
    let mut c = bin();
    c.args(["--headless", "--flat", "--start", "car", "--hold", "2", "--mute", "--weather", "clear"]);
    let (ok, text) = run_with_limit(c, Duration::from_secs(120));
    assert!(ok, "headless car run failed: {text}");
    assert!(!text.to_lowercase().contains("panicked"), "no panic: {text}");
}

#[test]
fn bad_start_mode_is_rejected() {
    let mut c = bin();
    c.args(["--start", "swim"]);
    let (ok, text) = run_with_limit(c, Duration::from_secs(30));
    assert!(!ok, "an unknown start mode exits non-zero: {text}");
    assert!(text.contains("foot, car or plane"), "the error says what is accepted: {text}");
}
