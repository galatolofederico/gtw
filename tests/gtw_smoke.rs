//! GTW smoke tests: the on-foot start really reaches the game loop headless,
//! without a window or the network, and the CLI carries the ground flags.

use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The game opens a GPU device on start, and several at once can exhaust a
/// small card's memory: the runs that draw wait for each other.
static GPU: Mutex<()> = Mutex::new(());

fn gpu_lock() -> std::sync::MutexGuard<'static, ()> {
    GPU.lock().unwrap_or_else(|e| e.into_inner())
}

fn bin() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_gtw"));
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
    assert!(text.contains("--bailout SECONDS"), "usage names --bailout: {text}");
    assert!(text.contains("--car-bailout SECONDS"), "usage names --car-bailout: {text}");
    assert!(text.contains("--flat"), "usage names --flat: {text}");
    assert!(text.contains("--no-save"), "usage names --no-save: {text}");
    assert!(text.contains("gtw [--version]"), "usage names the GTW binary: {text}");
    assert!(!text.contains("FableFlight") && !text.contains("Fable Flight") && !text.contains("fablegame"), "usage carries no old name: {text}");
}

#[test]
fn version_names_gtw() {
    let mut c = bin();
    c.arg("--version");
    let (ok, text) = run_with_limit(c, Duration::from_secs(30));
    assert!(ok, "--version failed: {text}");
    assert!(text.lines().any(|l| l.trim().starts_with("gtw ")), "--version prints the GTW name: {text}");
    assert!(!text.contains("FableFlight") && !text.contains("fablegame"), "--version carries no old name: {text}");
}

#[test]
fn starts_on_foot_headless() {
    let _gpu = gpu_lock();
    // Flat ground and no network: the character starts at the default place,
    // settles on the ground and the loop runs for the hold's seconds.
    let mut c = bin();
    c.args(["--headless", "--flat", "--start", "foot", "--hold", "2", "--no-save", "--mute", "--weather", "clear", "--graphics", "very-low", "--render-scale", "0.35"]);
    let (ok, text) = run_with_limit(c, Duration::from_secs(120));
    assert!(ok, "headless foot run failed: {text}");
    assert!(!text.to_lowercase().contains("panicked"), "no panic: {text}");
}

#[test]
fn starts_in_a_car_headless() {
    let _gpu = gpu_lock();
    // The same flat world, but the start puts a sedan under the player: the
    // car physics runs, the HUD draws and the loop stays panic-free.
    let mut c = bin();
    c.args(["--headless", "--flat", "--start", "car", "--hold", "2", "--no-save", "--mute", "--weather", "clear", "--graphics", "very-low", "--render-scale", "0.35"]);
    let (ok, text) = run_with_limit(c, Duration::from_secs(120));
    assert!(ok, "headless car run failed: {text}");
    assert!(!text.to_lowercase().contains("panicked"), "no panic: {text}");
}

#[test]
fn takes_a_traffic_car_and_exits_headless() {
    let _gpu = gpu_lock();
    // --carjack puts one deterministic traffic car just ahead on a flat world.
    // The scripted run flags it down, takes it at a standstill, drives a moment
    // and gets out again.
    let mut c = bin();
    c.args(["--headless", "--flat", "--start", "foot", "--carjack", "--hold", "3", "--no-save", "--mute", "--weather", "clear", "--graphics", "very-low", "--render-scale", "0.35"]);
    let (ok, text) = run_with_limit(c, Duration::from_secs(120));
    assert!(ok, "headless traffic carjack run failed: {text}");
    assert!(text.contains("carjack: took a traffic car"), "it takes the traffic car: {text}");
    assert!(text.contains("carjack: left the traffic car"), "it gets out again: {text}");
    assert!(!text.to_lowercase().contains("panicked"), "no panic: {text}");
}

#[test]
fn starts_a_plane_at_an_airport_headless() {
    let _gpu = gpu_lock();
    // --start plane --airport takes the normal airport path. Offline and
    // with no save, so the run needs no network and never touches the
    // developer's resume file; if the airport layout cannot be built it
    // falls back to an overhead start, which still runs the loop.
    let mut c = bin();
    c.args(["--headless", "--flat", "--start", "plane", "--airport", "EZE", "--offline", "--no-save", "--hold", "2", "--mute", "--weather", "clear", "--graphics", "very-low", "--render-scale", "0.35"]);
    let (ok, text) = run_with_limit(c, Duration::from_secs(120));
    assert!(ok, "headless plane-start run failed: {text}");
    assert!(!text.to_lowercase().contains("panicked"), "no panic: {text}");
}

#[test]
fn bails_out_and_lands_headless() {
    let _gpu = gpu_lock();
    // Flat dry ground over Buenos Aires: the Cessna takes off, the script
    // leaves it at 180 m above the sea, freefalls, pulls the ripcord and the
    // body lands on foot. The run quits itself once the fall is over.
    let mut c = bin();
    c.args([
        "--headless",
        "--flat",
        "--start",
        "plane",
        "--aircraft",
        "c172",
        "--lat",
        "-34.60",
        "--lon",
        "-58.45",
        "--alt",
        "180",
        "--wind",
        "0,0",
        "--bailout",
        "1",
        "--no-save",
        "--mute",
        "--offline",
        "--weather",
        "clear",
        "--graphics",
        "very-low",
        "--render-scale",
        "0.35",
    ]);
    let (ok, text) = run_with_limit(c, Duration::from_secs(120));
    assert!(ok, "headless bailout run failed: {text}");
    assert!(text.contains("bailout: jumped"), "it leaves the aircraft: {text}");
    assert!(text.contains("bailout: parachute open"), "the scripted pull opens it: {text}");
    assert!(text.contains("bailout: landed"), "it comes down: {text}");
    assert!(text.contains("bailout: on foot"), "the landing returns to walk mode: {text}");
    assert!(!text.to_lowercase().contains("panicked"), "no panic: {text}");
}

#[test]
fn bails_out_of_a_moving_car_headless() {
    let _gpu = gpu_lock();
    // A car start on flat dry ground: the script puts the car up to speed,
    // leaves it driverless and rolls the body out onto its feet. The run
    // quits itself once the fall is over.
    let mut c = bin();
    c.args([
        "--headless",
        "--flat",
        "--start",
        "car",
        "--car-bailout",
        "1",
        "--lat",
        "-34.60",
        "--lon",
        "-58.45",
        "--no-save",
        "--mute",
        "--offline",
        "--weather",
        "clear",
        "--graphics",
        "very-low",
        "--render-scale",
        "0.35",
    ]);
    let (ok, text) = run_with_limit(c, Duration::from_secs(120));
    assert!(ok, "headless car bailout run failed: {text}");
    assert!(text.contains("car bailout: left the car"), "it leaves the moving car: {text}");
    assert!(text.contains("car bailout: on foot, empty car still rolling"), "the roll puts it back on foot and the car carries on: {text}");
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
