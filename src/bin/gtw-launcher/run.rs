//! Running the game: as itself (Play) or as the graphics test (`--bench`).
//! The game is started with `FABLE_PROGRESS=1`, so it prints each stage of
//! its start on stdout (src/progress.rs); the graphics test also prints its
//! result there (`@fable-bench {json}`). Its log (stderr) is kept in
//! `<cache>/gtw/last-run.log` and the last lines shown if it fails.
//!
//! The stages go into the start's `Timeline` on the reading thread itself,
//! not when the window next draws: a window the game's covers (Wayland) may
//! not draw at all, and the start must still be followed.
use crossbeam_channel::{Receiver, Sender};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// What the game's threads report.
pub enum Event {
    /// a stage of the start came (it is already in `Running::tl`)
    Stage,
    /// the graphics test's figures
    Bench(serde_json::Value),
    /// a line of the game's log
    Log(String),
    /// the game has gone, with its exit code
    Exit(Option<i32>),
}

pub struct Running {
    child: Arc<parking_lot::Mutex<Child>>,
    pub rx: Receiver<Event>,
    pub started: Instant,
    /// the start's stages so far, kept up by the thread reading the game
    pub tl: Arc<parking_lot::Mutex<Timeline>>,
}

impl Running {
    /// Ends it (a graphics test called off).
    pub fn kill(&self) {
        let _ = self.child.lock().kill();
    }
}

/// The game's log of the last run.
pub fn log_path() -> PathBuf {
    crate::paths::cache().unwrap_or_else(std::env::temp_dir).join("last-run.log")
}

/// Starts the game with these arguments; `history` is the last start's
/// timings (for the bar), `ctx` the window to wake at each stage.
pub fn spawn(exe: &Path, args: &[String], history: &[(String, u64)], ctx: Option<egui::Context>) -> std::io::Result<Running> {
    let mut cmd = Command::new(exe);
    cmd.args(args).env("FABLE_PROGRESS", "1").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if std::env::var_os("RUST_LOG").is_none() {
        cmd.env("RUST_LOG", "warn,gtw=info");
    }
    if let Some(dir) = exe.parent().filter(|d| !d.as_os_str().is_empty()) {
        cmd.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // no console window of its own for the game (CREATE_NO_WINDOW)
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn()?;
    let started = Instant::now();
    let (tx, rx) = crossbeam_channel::unbounded();
    let out = child.stdout.take();
    let err = child.stderr.take();
    let child = Arc::new(parking_lot::Mutex::new(child));
    let tl = Arc::new(parking_lot::Mutex::new(Timeline::new(history)));
    if let Some(out) = out {
        let tx = tx.clone();
        let tl = tl.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if let Some(stage) = line.strip_prefix("@fable-progress ") {
                    let (stage, at) = (stage.trim().to_string(), started.elapsed());
                    tl.lock().stage(&stage, at);
                    let _ = tx.send(Event::Stage);
                    if let Some(ctx) = &ctx {
                        ctx.request_repaint();
                    }
                } else if let Some(j) = line.strip_prefix("@fable-bench ") {
                    if let Ok(v) = serde_json::from_str(j) {
                        let _ = tx.send(Event::Bench(v));
                    }
                } else {
                    let _ = tx.send(Event::Log(line));
                }
            }
        });
    }
    if let Some(err) = err {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let path = log_path();
            if let Some(d) = path.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let mut file = std::fs::File::create(&path).ok();
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                if let Some(f) = &mut file {
                    let _ = writeln!(f, "{line}");
                }
                let _ = tx.send(Event::Log(line));
            }
        });
    }
    {
        let child = child.clone();
        std::thread::spawn(move || waiter(child, tx));
    }
    Ok(Running { child, rx, started, tl })
}

fn waiter(child: Arc<parking_lot::Mutex<Child>>, tx: Sender<Event>) {
    loop {
        match child.lock().try_wait() {
            Ok(Some(status)) => {
                let _ = tx.send(Event::Exit(status.code()));
                return;
            }
            Ok(None) => {}
            Err(_) => {
                let _ = tx.send(Event::Exit(None));
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The stages of a start in order (src/progress.rs), the four batches of
/// shaders numbered.
pub const STAGES: [&str; 12] = ["boot", "window", "gpu", "shaders", "shaders-part1", "shaders-part2", "shaders-part3", "shaders-part4", "shaders-done", "aircraft", "world", "ready"];

/// A cold start on a mid-range PC (ms), until the launcher has timed one.
const TYPICAL_MS: [u64; 12] = [0, 300, 450, 700, 1500, 3900, 4500, 9600, 9600, 11000, 11200, 11500];

/// What a stage means to the player.
pub fn stage_label(stage: &str) -> &'static str {
    match stage {
        "" => "Launching the game",
        "boot" => "Starting up",
        "window" => "Opening the window",
        "gpu" => "Setting up the graphics card",
        "shaders" | "shaders-part1" | "shaders-part2" | "shaders-part3" => "Compiling shaders",
        "shaders-part4" | "shaders-done" => "Building the aircraft",
        "aircraft" => "Loading the world",
        "world" => "Drawing the first frame",
        "ready" => "Ready",
        _ => "Starting",
    }
}

/// The start's progress: the stages seen, timed against the last start.
pub struct Timeline {
    /// when each stage is expected (ms), from the last start or a typical one
    expect: [u64; 12],
    /// those seen this time: index and when (ms)
    seen: Vec<(usize, u64)>,
    parts: usize,
}

impl Timeline {
    pub fn new(history: &[(String, u64)]) -> Self {
        let mut expect = TYPICAL_MS;
        if history.iter().any(|(s, _)| s == "ready") {
            for (i, s) in STAGES.iter().enumerate() {
                if let Some((_, ms)) = history.iter().find(|(h, _)| h == s) {
                    expect[i] = *ms;
                }
            }
            // keep it in order should a stage be missing
            for i in 1..expect.len() {
                expect[i] = expect[i].max(expect[i - 1]);
            }
        }
        Timeline { expect, seen: Vec::new(), parts: 0 }
    }

    /// A stage reported by the game.
    pub fn stage(&mut self, stage: &str, at: Duration) {
        let key = if stage == "shaders-part" {
            self.parts += 1;
            format!("shaders-part{}", self.parts.min(4))
        } else {
            stage.to_string()
        };
        if let Some(i) = STAGES.iter().position(|s| *s == key) {
            if self.seen.last().is_none_or(|(j, _)| i > *j) {
                self.seen.push((i, at.as_millis() as u64));
            }
        }
    }

    pub fn ready(&self) -> bool {
        self.ready_at().is_some()
    }

    /// When the first frame was drawn (seconds since the launch).
    pub fn ready_at(&self) -> Option<f64> {
        self.seen.last().filter(|(i, _)| *i == STAGES.len() - 1).map(|(_, ms)| *ms as f64 / 1000.0)
    }

    /// The stage now, for its label.
    pub fn current(&self) -> &'static str {
        self.seen.last().map_or("", |(i, _)| STAGES[*i])
    }

    /// Which batch of shaders is compiling (1..4), if any.
    pub fn shader_batch(&self) -> Option<usize> {
        let i = self.seen.last()?.0;
        (3..=6).contains(&i).then_some(i - 2)
    }

    /// How far the start is, 0..1, `elapsed` since the launch: each stage
    /// reached moves the bar to its share (by the last start's timings), and
    /// between stages it creeps on but never past the next one's share.
    pub fn progress(&self, elapsed: Duration) -> f32 {
        let total = self.expect[STAGES.len() - 1].max(1) as f64;
        let now = elapsed.as_millis() as f64;
        let (i, at) = self.seen.last().map_or((0usize, 0.0), |(i, at)| (*i, *at as f64));
        if i + 1 >= STAGES.len() {
            return 1.0;
        }
        let (a, b) = (self.expect[i] as f64, self.expect[i + 1] as f64);
        let span = (b - a).max(50.0);
        let t = (now - at) / span;
        // straight on until 85% of the way to the next stage, then ever slower
        // (a slow stage creeps rather than freezes)
        let f = if t < 0.85 { t } else { 0.85 + 0.15 * (1.0 - (-(t - 0.85) * 2.5).exp()) };
        // stop short of the next stage's share: only the game moves it on
        let p = (a + span * f.clamp(0.0, 0.97)) / total;
        p.clamp(0.0, 0.995) as f32
    }

    /// The timings to remember once the start is over.
    pub fn record(&self) -> Vec<(String, u64)> {
        self.seen.iter().map(|(i, ms)| (STAGES[*i].to_string(), *ms)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_runs_through_the_stages() {
        let mut t = Timeline::new(&[]);
        let ms = Duration::from_millis;
        assert!(t.progress(ms(0)) < 0.05);
        t.stage("boot", ms(10));
        t.stage("window", ms(300));
        t.stage("gpu", ms(500));
        t.stage("shaders", ms(700));
        let p1 = t.progress(ms(800));
        for _ in 0..4 {
            t.stage("shaders-part", ms(5000));
        }
        assert_eq!(t.current(), "shaders-part4");
        let p2 = t.progress(ms(5100));
        assert!(p2 > p1);
        // a stage far over its time never reaches the next one's share
        let p3 = t.progress(ms(600_000));
        assert!(p3 < 1.0 && p3 >= p2);
        assert!(p3 < t.expect[9] as f32 / t.expect[11] as f32);
        t.stage("shaders-done", ms(9000));
        t.stage("aircraft", ms(10000));
        t.stage("world", ms(10100));
        t.stage("ready", ms(10200));
        assert!(t.ready());
        assert_eq!(t.ready_at(), Some(10.2));
        assert_eq!(t.progress(ms(10300)), 1.0);
        let rec = t.record();
        let again = Timeline::new(&rec);
        assert_eq!(again.expect[11], 10200);
    }

    #[cfg(unix)]
    #[test]
    fn reads_the_games_lines() {
        // a stand-in for the game: two stages, a test result, a log line, and out
        let dir = std::env::temp_dir().join(format!("ff-launcher-run-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("fake-game");
        std::fs::write(&exe, "#!/bin/sh\necho '@fable-progress boot'\necho '@fable-progress shaders-part'\necho 'some log' >&2\necho '@fable-bench {\"fps\": 61.5, \"low1\": 48}'\nexit 3\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let r = spawn(&exe, &[], &[], None).unwrap();
        let (mut stages, mut bench, mut log, mut code) = (Vec::new(), None, Vec::new(), None);
        while let Ok(ev) = r.rx.recv_timeout(Duration::from_secs(5)) {
            match ev {
                Event::Stage => stages.push(r.tl.lock().current()),
                Event::Bench(j) => bench = Some(j),
                Event::Log(l) => log.push(l),
                Event::Exit(c) => {
                    code = Some(c);
                    break;
                }
            }
        }
        // in the timeline as each event comes, read or not
        assert_eq!(r.tl.lock().current(), "shaders-part1");
        assert!(stages.last().is_some_and(|s| *s == "shaders-part1"));
        assert_eq!(bench.unwrap()["fps"].as_f64(), Some(61.5));
        assert_eq!(code, Some(Some(3)));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = log;
    }
}
