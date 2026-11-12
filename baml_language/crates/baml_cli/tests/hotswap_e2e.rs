//! `baml run --hot` through the real CLI (BEP-087, stage 1): the build cache
//! and swapping each save. The swap semantics are tested
//! against the engine API in `bex_project/tests/hotswap.rs`.

use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

const WATCH_FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/hotswap_watch");

/// `$BAML_HOME` with update checks off, so runs are isolated.
fn home() -> tempfile::TempDir {
    let home = tempfile::tempdir().expect("tempdir for BAML_HOME");
    std::fs::write(
        home.path().join("config.toml"),
        "[update]\nauto_check = false\n",
    )
    .unwrap();
    home
}

fn baml(home: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_baml-cli"));
    command
        .args(args)
        .current_dir(WATCH_FIXTURE)
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .env("BAML_HOME", home)
        .env("BOUNDARY_API_KEY", "local")
        .env_remove("BAML_BUILD_CACHE");
    command
}

fn run(home: &Path, args: &[&str]) -> Output {
    baml(home, args).output().expect("baml-cli runs")
}

fn cache_entries(home: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, found);
            } else {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(&home.join("build/cache"), &mut found);
    found.sort();
    found
}

/// `--hot` makes every function swappable, which changes escape facts, so
/// neither mode may be served the other's bytecode: a run in the other mode is
/// a cache miss that writes its own entries, and repeating a mode is a hit
/// that writes none.
#[test]
fn the_bytecode_cache_keeps_hot_runs_apart() {
    let home = home();
    assert!(run(home.path(), &["run", "Step"]).status.success());
    let plain = cache_entries(home.path());
    assert!(!plain.is_empty(), "the first run should populate the cache");
    assert!(run(home.path(), &["run", "--hot", "Step"]).status.success());
    let hot = cache_entries(home.path());
    assert!(hot.len() > plain.len(), "--hot reused ordinary artifacts");
    assert!(run(home.path(), &["run", "--hot", "Step"]).status.success());
    assert_eq!(
        hot,
        cache_entries(home.path()),
        "--hot should hit its own entries"
    );
    assert!(run(home.path(), &["run", "Step"]).status.success());
    assert_eq!(
        hot,
        cache_entries(home.path()),
        "an ordinary run should hit its own entries"
    );
}

/// Expression mode runs no watcher, so it refuses `--hot` instead of
/// ignoring it.
#[test]
fn hot_is_refused_with_an_expression() {
    let home = home();
    let output = run(home.path(), &["run", "--hot", "-e", "1"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cannot be used with"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Kills and reaps the child if the test fails before it exits.
struct KillOnDrop(std::process::Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The child's combined output. Stdout and stderr arrive through separate
/// threads, so lines a check passes over are kept for a later one.
struct ChildOutput {
    lines: std::sync::mpsc::Receiver<String>,
    unmatched: Vec<String>,
}

impl ChildOutput {
    /// Wait for a line containing `needle`, whether it already arrived or not.
    fn expect_line(&mut self, needle: &str) {
        if let Some(at) = self.unmatched.iter().position(|line| line.contains(needle)) {
            self.unmatched.remove(at);
            return;
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match self.lines.recv_timeout(left) {
                Ok(line) if line.contains(needle) => return,
                Ok(line) => self.unmatched.push(line),
                Err(_) => break,
            }
        }
        panic!("never saw {needle:?} in: {:#?}", self.unmatched);
    }
}

/// Run `baml run` with `args` in `dir` while editing `source`: a swap, a
/// rejected save that leaves the program running, and a second swap.
fn hot_edits_take_effect(dir: &Path, args: &[&str], source: &Path, original: &str) {
    use std::{
        io::{BufRead as _, BufReader},
        process::Stdio,
    };
    let home = home();
    let mut child = KillOnDrop(
        baml(home.path(), args)
            .current_dir(dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("baml-cli runs"),
    );
    let (sender, lines) = std::sync::mpsc::channel();
    let stdout = child.0.stdout.take().unwrap();
    let stderr = child.0.stderr.take().unwrap();
    let forward = |stream: Box<dyn std::io::Read + Send>,
                   sender: std::sync::mpsc::Sender<String>| {
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let _ = sender.send(line);
            }
        });
    };
    forward(Box::new(stdout), sender.clone());
    forward(Box::new(stderr), sender);
    let mut output = ChildOutput {
        lines,
        unmatched: Vec::new(),
    };

    output.expect_line("Watching");
    output.expect_line("v1");
    std::fs::write(source, original.replace("\"v1\"", "\"v2\"")).unwrap();
    output.expect_line("Swapped Step (");
    output.expect_line("v2");
    // A signature change is rejected and the program keeps running v2.
    std::fs::write(
        source,
        original.replace(
            "function Step() -> string { \"v1\" }",
            "function Step(x: int = 1) -> string { \"v9\" }",
        ),
    )
    .unwrap();
    output.expect_line("Rejected save needs a restart");
    std::fs::write(source, original.replace("\"v1\"", "\"v3\"")).unwrap();
    output.expect_line("v3");
    std::fs::write(dir.join("stop"), "").unwrap();
    let status = child.0.wait().expect("baml-cli exits");
    assert!(status.success());
}

fn watch_source() -> String {
    std::fs::read_to_string(format!("{WATCH_FIXTURE}/baml_src/main.baml")).unwrap()
}

#[test]
fn hot_swaps_each_save_and_survives_a_rejected_one() {
    let project = tempfile::tempdir().expect("tempdir for project");
    let source = project.path().join("baml_src/main.baml");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::copy(
        format!("{WATCH_FIXTURE}/baml.toml"),
        project.path().join("baml.toml"),
    )
    .unwrap();
    let original = watch_source();
    std::fs::write(&source, &original).unwrap();
    // A file project discovery ignores never ran, so `--hot` must not compile
    // it either: its duplicate `Step` would make every patch fail.
    std::fs::write(
        project.path().join("baml_src/ignored.baml"),
        "function Step() -> string { \"ignored\" }\n",
    )
    .unwrap();
    std::fs::write(project.path().join("baml_src/.ignore"), "ignored.baml\n").unwrap();
    hot_edits_take_effect(
        project.path(),
        &["run", "--hot", "main"],
        &source,
        &original,
    );
}

/// A standalone file has no project, so its own text is the baseline.
#[test]
fn hot_swaps_a_standalone_file() {
    let dir = tempfile::tempdir().expect("tempdir for file");
    let source = dir.path().join("demo.baml");
    let original = watch_source();
    std::fs::write(&source, &original).unwrap();
    hot_edits_take_effect(
        dir.path(),
        &["run", "--hot", "--file", "demo.baml", "main"],
        &source,
        &original,
    );
}
