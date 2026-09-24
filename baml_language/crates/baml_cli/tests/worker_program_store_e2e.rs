// End-to-end tests of the worker's program store (contract section 9.5 of
// `documents/durable-poc-contracts.md`): `--program-store`, `--program-hash`,
// `hello.program_hash`, and `resumed.stats.program_source`.
//
// Each test starts the real `baml-cli worker` binary. The fixture
// `tests/fixtures/worker_store` keeps classes, nested classes, an enum, a map,
// an optional, a union, and a cyclic object graph alive across the pause, so
// that a resume from the store is checked against a full structured result.
//
// A worker that must not compile gets no `--project` and runs in an empty
// working directory. The run can then only succeed with the stored program.

mod common;

use std::{
    io::{BufRead as _, BufReader, Read as _, Write as _},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

use bex_program_store::{Durability, Lookup, Missing, ProgramHash, ProgramStore};
use serde_json::{Value, json};

const RUN_ID: &str = "r-store";
const SOURCE_FILE: &str = "baml_src/itinerary.baml";
const EVENT_TIMEOUT: Duration = Duration::from_secs(120);

/// The directories of one test: a copy of the fixture project, the program
/// store, the snapshot directory, and an empty working directory.
struct Dirs {
    home: tempfile::TempDir,
}

impl Dirs {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let dirs = Self { home };
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/worker_store");
        std::fs::create_dir_all(dirs.project().join("baml_src")).unwrap();
        std::fs::copy(source.join(SOURCE_FILE), dirs.project().join(SOURCE_FILE)).unwrap();
        std::fs::create_dir_all(dirs.empty_cwd()).unwrap();
        dirs
    }

    fn project(&self) -> PathBuf {
        self.home.path().join("project")
    }

    fn store(&self) -> PathBuf {
        self.home.path().join("programs")
    }

    fn snapshots(&self) -> PathBuf {
        self.home.path().join("snapshots")
    }

    fn empty_cwd(&self) -> PathBuf {
        self.home.path().join("empty")
    }

    /// The store as this build of the worker sees it.
    fn open_store(&self) -> ProgramStore {
        ProgramStore::open(self.store(), bex_engine::durable::runtime_build())
            .unwrap()
            .with_durability(Durability::None)
    }
}

/// Whether the worker gets `--project`.
#[derive(Clone, Copy, PartialEq)]
enum Project {
    Given,
    Absent,
}

struct Worker {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: mpsc::Receiver<String>,
    events: Vec<Value>,
}

impl Worker {
    fn spawn(dirs: &Dirs, project: Project, segment: u64, args: &[&str]) -> Self {
        let mut command = Command::new(common::baml_cli());
        command.arg("worker");
        if project == Project::Given {
            command.arg("--project").arg(dirs.project());
        }
        let mut child = command
            .args(["--run", RUN_ID, "--segment", &segment.to_string()])
            .arg("--snapshot-dir")
            .arg(dirs.snapshots())
            .args(["--auto-snapshot-ms", "0"])
            .args(args)
            // An empty directory: a worker that fell back to compiling the
            // working directory would find no `.baml` file.
            .current_dir(dirs.empty_cwd())
            .env("HOME", dirs.home.path())
            .env("BAML_HOME", dirs.home.path().join(".baml-home"))
            .env("BAML_CLI_ALLOW_DIRECT", "1")
            .env("BAML_AGENT_SKILL_CHECK", "off")
            .env_remove("BAML_LOG")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn baml-cli worker");
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin: Some(stdin),
            lines,
            events: Vec::new(),
        }
    }

    /// `--start` with the program store and `extra` arguments.
    fn start(
        dirs: &Dirs,
        project: Project,
        function: &str,
        json_args: &str,
        extra: &[&str],
    ) -> Self {
        let store = dirs.store();
        let mut args = vec![
            "--program-store",
            store.to_str().unwrap(),
            "--start",
            function,
            "--json-args",
            json_args,
        ];
        args.extend_from_slice(extra);
        Self::spawn(dirs, project, 1, &args)
    }

    /// `--resume` with the program store.
    fn resume(dirs: &Dirs, project: Project, snapshot: &str) -> Self {
        let store = dirs.store();
        Self::spawn(
            dirs,
            project,
            2,
            &[
                "--program-store",
                store.to_str().unwrap(),
                "--resume",
                snapshot,
            ],
        )
    }

    fn send(&mut self, command: &Value) {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        writeln!(stdin, "{command}").unwrap();
        stdin.flush().unwrap();
    }

    fn next_event(&mut self) -> Option<Value> {
        let line = match self.lines.recv_timeout(EVENT_TIMEOUT) {
            Ok(line) => line,
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = self.child.kill();
                panic!(
                    "timed out waiting for a worker event; got: {:#?}",
                    self.events
                );
            }
        };
        let event: Value = serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("stdout line is not JSON ({e}): {line:?}"));
        self.events.push(event.clone());
        Some(event)
    }

    fn wait_for(&mut self, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
        loop {
            let Some(event) = self.next_event() else {
                panic!(
                    "worker output ended before {what}; events: {:#?}",
                    self.events
                );
            };
            if pred(&event) {
                return event;
            }
        }
    }

    /// Drain the output. Returns the exit code, the events, and stderr.
    fn finish(mut self) -> (i32, Vec<Value>, String) {
        while self.next_event().is_some() {}
        let status = self.child.wait().unwrap();
        let mut stderr = String::new();
        if let Some(mut pipe) = self.child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        (
            status.code().expect("worker ended by signal"),
            self.events,
            stderr,
        )
    }
}

fn hello_of(events: &[Value]) -> &Value {
    assert_eq!(
        events[0]["type"], "hello",
        "hello is the first event: {events:#?}"
    );
    &events[0]
}

fn hash_of(hello: &Value) -> ProgramHash {
    hello["program_hash"]
        .as_str()
        .unwrap_or_else(|| panic!("hello has no program_hash: {hello}"))
        .parse()
        .unwrap_or_else(|e| panic!("hello.program_hash is not a hash ({e}): {hello}"))
}

/// Run `quick` from the project with the store and return the program hash.
fn store_program(dirs: &Dirs) -> ProgramHash {
    let worker = Worker::start(dirs, Project::Given, "quick", r#"{"city":"Oslo"}"#, &[]);
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    hash_of(hello_of(&events))
}

/// Start `durable_itinerary` from the project, pause it after its second
/// stop, and return the program hash with the snapshot path.
fn paused_itinerary(dirs: &Dirs, season: &str) -> (ProgramHash, String) {
    let args = json!({ "city": "Oslo", "season": season }).to_string();
    let mut worker = Worker::start(dirs, Project::Given, "durable_itinerary", &args, &[]);
    worker.wait_for("the second stop", |e| {
        e["type"] == "log" && e["text"] == "stop 2"
    });
    worker.send(&json!({ "type": "pause" }));
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 75, "stderr: {stderr}\nevents: {events:#?}");
    let paused = events.last().unwrap();
    assert_eq!(paused["type"], "paused", "{events:#?}");
    (
        hash_of(hello_of(&events)),
        paused["snapshot_path"].as_str().unwrap().to_string(),
    )
}

fn expected_itinerary(season: &str) -> Value {
    let note = if season == "Summer" {
        json!("bring water")
    } else {
        Value::Null
    };
    let stop = |day: u64| json!({ "name": format!("Oslo {day}"), "nights": day, "note": note });
    json!({
        "city": "Oslo",
        "season": season,
        "stops": [stop(1), stop(2), stop(3)],
        "legs": [
            { "origin": stop(1), "target": stop(2), "hours": 1.5 },
            { "origin": stop(2), "target": stop(3), "hours": 1.5 },
        ],
        "budget": { "base": 100, "day1": 10, "day2": 20, "day3": 30 },
        "choice": if season == "Summer" { json!("beach") } else { json!(7) },
        "ring": "first>second>first",
    })
}

fn assert_failed_with(events: &[Value], code: i32, needle: &str) {
    assert_eq!(code, 1, "{events:#?}");
    let last = events.last().unwrap();
    assert_eq!(last["type"], "failed", "{events:#?}");
    assert!(
        last["error"].as_str().unwrap().contains(needle),
        "expected an error that contains {needle:?}: {last}"
    );
}

#[test]
fn a_start_from_the_project_stores_the_program() {
    let dirs = Dirs::new();
    let worker = Worker::start(&dirs, Project::Given, "quick", r#"{"city":"Oslo"}"#, &[]);
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");

    let hello = hello_of(&events);
    assert_eq!(hello["mode"], "start");
    assert_eq!(hello["function"], "quick");
    assert_eq!(hello["durable"], false);
    assert_eq!(hello["program_source"], "compile");
    let hash = hash_of(hello);
    assert_eq!(
        events.last().unwrap()["value"],
        json!({ "name": "Oslo", "nights": 1, "note": null })
    );

    // The contract layout, and bytes that hash to the reported value.
    let hex = hash.to_hex();
    let entry = dirs.store().join(&hex[..2]).join(format!("{hex}.bamlprog"));
    assert!(entry.is_file(), "{entry:?}");
    let stored = dirs.open_store().get(hash).found().expect("a usable entry");
    assert_eq!(
        bex_engine::durable::program_hash_of_bytes(stored.bytes()),
        *hash.as_bytes(),
        "the store key is the snapshot crate's program hash"
    );
    assert_eq!(stored.written_by(), bex_engine::durable::runtime_build());

    // A second start finds the entry and leaves it alone.
    let before = std::fs::metadata(&entry).unwrap().modified().unwrap();
    assert_eq!(store_program(&dirs), hash);
    assert_eq!(
        std::fs::metadata(&entry).unwrap().modified().unwrap(),
        before
    );
}

#[test]
fn a_start_with_a_program_hash_runs_without_a_project() {
    let dirs = Dirs::new();
    let hash = store_program(&dirs);
    // The machine that runs the program never had the project.
    std::fs::remove_dir_all(dirs.project()).unwrap();

    let worker = Worker::start(
        &dirs,
        Project::Absent,
        "quick",
        r#"{"city":"Bergen"}"#,
        &["--program-hash", &hash.to_hex()],
    );
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    let hello = hello_of(&events);
    assert_eq!(hello["program_hash"], hash.to_hex());
    assert_eq!(hello["program_source"], "store");
    assert!(hello["program_compile_ms"].is_null(), "{hello}");
    assert_eq!(
        events.last().unwrap()["value"],
        json!({ "name": "Bergen", "nights": 1, "note": null })
    );
}

#[test]
fn a_resume_without_a_project_loads_the_program_from_the_store() {
    for season in ["Summer", "Winter"] {
        let dirs = Dirs::new();
        let (hash, snapshot) = paused_itinerary(&dirs, season);

        // The snapshot header carries the value the store is keyed by.
        let header = bex_engine::durable::read_header(&std::fs::read(&snapshot).unwrap()).unwrap();
        assert_eq!(header.program_hash, *hash.as_bytes());

        std::fs::remove_dir_all(dirs.project()).unwrap();
        let mut worker = Worker::resume(&dirs, Project::Absent, &snapshot);
        let hello = worker.next_event().expect("hello");
        assert_eq!(hello["type"], "hello");
        assert_eq!(hello["mode"], "resume");
        assert_eq!(hello["function"], "durable_itinerary");
        assert_eq!(hello["program_hash"], hash.to_hex());
        let resumed = worker.wait_for("resumed", |e| e["type"] == "resumed");
        assert_eq!(resumed["stats"]["program_source"], "store", "{resumed}");
        assert!(resumed["stats"]["program_load_ms"].is_number(), "{resumed}");
        assert!(
            resumed["stats"]["program_compile_ms"].is_null(),
            "{resumed}"
        );
        // Positions name the source file relative to the project, although
        // this worker never saw the project.
        let position = worker.wait_for("position", |e| e["type"] == "position");
        assert_eq!(position["file"], SOURCE_FILE, "{position}");
        assert_eq!(position["function"], "durable_itinerary", "{position}");

        let (code, events, stderr) = worker.finish();
        assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
        let logs: Vec<_> = events
            .iter()
            .filter(|e| e["type"] == "log")
            .map(|e| e["text"].as_str().unwrap())
            .collect();
        assert_eq!(logs, ["stop 3"], "only the remaining iteration prints");
        let last = events.last().unwrap();
        assert_eq!(last["type"], "completed");
        assert_eq!(last["value"], expected_itinerary(season));
    }
}

/// The offer that the test delivers for a `remote_offer` call: the announced
/// arguments come back inside a class with a map and a union.
fn offer_for(call: &Value, choice: &Value) -> Value {
    let stop = &call["args"]["stop"];
    json!({
        "stop": stop,
        "season": call["args"]["season"],
        "prices": { "night": stop["nights"].as_i64().unwrap() * 10, "cleaning": 5 },
        "choice": choice,
    })
}

fn threads_started(events: &[Value]) -> Vec<(u64, Option<u64>)> {
    let mut threads: Vec<_> = events
        .iter()
        .filter(|e| e["type"] == "thread_started")
        .map(|e| (e["thread"].as_u64().unwrap(), e["parent_thread"].as_u64()))
        .collect();
    threads.sort_unstable();
    threads
}

/// The program store and the multi-thread resume path together: a run with
/// four threads suspends itself for a sleep, and a worker that has no project
/// restores every thread from the store entry. The results of the three
/// outstanding calls arrive shuffled and with a duplicate, a race in the second
/// segment cancels its loser, and the result holds classes, an enum, maps,
/// optionals, unions, and a walk over a cyclic graph.
#[test]
fn a_self_suspended_run_with_several_threads_resumes_from_the_store_without_a_project() {
    for season in ["Summer", "Winter"] {
        let dirs = Dirs::new();
        let args = json!({ "city": "Oslo", "season": season, "nap_ms": 2500 }).to_string();
        // Nothing is sent to this worker. Its stdin stays open until it exits,
        // so the suspend is the run's own decision.
        let worker = Worker::start(
            &dirs,
            Project::Given,
            "durable_offer_board",
            &args,
            &["--sleep-suspend-ms", "1000"],
        );
        let (code, events, stderr) = worker.finish();
        assert_eq!(code, 75, "stderr: {stderr}\nevents: {events:#?}");
        let hash = hash_of(hello_of(&events));
        let paused = events.last().unwrap();
        assert_eq!(paused["type"], "paused", "{events:#?}");
        assert_eq!(paused["wake"]["reason"], "sleep", "{paused}");
        assert_eq!(paused["stats"]["threads"], 4, "{paused}");
        let calls: Vec<Value> = events
            .iter()
            .filter(|e| e["type"] == "remote_call")
            .cloned()
            .collect();
        assert_eq!(calls.len(), 3, "{events:#?}");
        let first_threads = threads_started(&events);
        assert_eq!(first_threads.len(), 4, "{events:#?}");
        let snapshot = paused["snapshot_path"].as_str().unwrap().to_string();

        // The sleep ends while the run has no process, and the project is gone.
        std::fs::remove_dir_all(dirs.project()).unwrap();
        let wake_at = paused["wake"]["at_ts"].as_u64().unwrap();
        let now = u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(wake_at.saturating_sub(now) + 50));

        // Results in another order than the calls, one of them twice.
        let choices = [json!("beach"), json!(7), json!("museum")];
        let results: Vec<String> = [2usize, 0, 2, 1]
            .iter()
            .map(|&i| {
                json!({ "call_id": calls[i]["call_id"], "value": offer_for(&calls[i], &choices[i]) })
                    .to_string()
            })
            .collect();
        let store = dirs.store();
        let mut resume_args = vec![
            "--program-store",
            store.to_str().unwrap(),
            "--sleep-suspend-ms",
            "1000",
            "--resume",
            &snapshot,
        ];
        for result in &results {
            resume_args.extend_from_slice(&["--remote-result", result]);
        }
        let mut worker = Worker::spawn(&dirs, Project::Absent, 2, &resume_args);
        let hello = worker.next_event().expect("hello");
        assert_eq!(hello["mode"], "resume", "{hello}");
        assert_eq!(hello["function"], "durable_offer_board", "{hello}");
        assert_eq!(hello["program_hash"], hash.to_hex(), "{hello}");
        let resumed = worker.wait_for("resumed", |e| e["type"] == "resumed");
        assert_eq!(resumed["stats"]["program_source"], "store", "{resumed}");
        assert!(
            resumed["stats"]["program_compile_ms"].is_null(),
            "{resumed}"
        );

        // The race of the second segment: answer the slow call, so the quick
        // one is the loser that the worker reports as abandoned.
        let mut race_calls = Vec::new();
        while race_calls.len() < 2 {
            race_calls
                .push(worker.wait_for("a remote_call of the race", |e| e["type"] == "remote_call"));
        }
        let slow = race_calls
            .iter()
            .find(|c| c["args"]["stop"]["name"] == "Oslo slow")
            .expect("the slow call");
        let quick = race_calls
            .iter()
            .find(|c| c["args"]["stop"]["name"] == "Oslo quick")
            .expect("the quick call");
        worker.send(&json!({
            "type": "remote_result",
            "call_id": slow["call_id"],
            "value": offer_for(slow, &json!(42)),
        }));
        let (code, events, stderr) = worker.finish();
        assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");

        // Every restored thread keeps its id and its parent.
        let second_threads = threads_started(&events);
        for thread in &first_threads {
            assert!(
                second_threads.contains(thread),
                "thread {thread:?} of segment 1 is missing in segment 2: {second_threads:?}"
            );
        }
        // No call of segment 1 is announced again, and each result is taken once.
        let announced: Vec<_> = events
            .iter()
            .filter(|e| e["type"] == "remote_call")
            .map(|e| e["call_id"].clone())
            .collect();
        assert_eq!(announced.len(), 2, "{events:#?}");
        for call in &calls {
            assert!(!announced.contains(&call["call_id"]), "{events:#?}");
            let received = events
                .iter()
                .filter(|e| {
                    e["type"] == "remote_result_received" && e["call_id"] == call["call_id"]
                })
                .count();
            assert_eq!(received, 1, "{events:#?}");
        }
        let cancels: Vec<_> = events
            .iter()
            .filter(|e| e["type"] == "remote_cancel")
            .collect();
        assert_eq!(cancels.len(), 1, "{events:#?}");
        assert_eq!(cancels[0]["call_id"], quick["call_id"], "{events:#?}");
        assert_eq!(cancels[0]["thread"], quick["thread"], "{events:#?}");
        let logs: Vec<_> = events
            .iter()
            .filter(|e| e["type"] == "log")
            .map(|e| e["text"].as_str().unwrap())
            .collect();
        assert_eq!(logs, ["collecting the offers"], "{events:#?}");

        // `baml.future.all` keeps the order of its inputs, which is the order
        // of the stops and not the order of the call ids or of the results.
        let mut by_stop: Vec<&Value> = calls.iter().collect();
        by_stop.sort_by_key(|c| c["args"]["stop"]["nights"].as_i64().unwrap());
        let offers: Vec<Value> = by_stop
            .iter()
            .map(|call| {
                let i = calls.iter().position(|c| c == *call).unwrap();
                offer_for(call, &choices[i])
            })
            .collect();
        let note = if season == "Summer" {
            json!("bring water")
        } else {
            Value::Null
        };
        for offer in &offers {
            assert_eq!(
                offer["stop"]["note"], note,
                "the optional travels in the arguments"
            );
            assert_eq!(offer["season"], season);
        }
        let last = events.last().unwrap();
        assert_eq!(last["type"], "completed", "{events:#?}");
        assert_eq!(
            last["value"],
            json!({
                "city": "Oslo",
                "offers": offers,
                "fastest": offer_for(slow, &json!(42)),
                "ring": "first>second>first",
            })
        );
    }
}

#[test]
fn a_resume_with_a_project_and_a_store_hit_does_not_compile() {
    let dirs = Dirs::new();
    let (_, snapshot) = paused_itinerary(&dirs, "Summer");
    let mut worker = Worker::resume(&dirs, Project::Given, &snapshot);
    let resumed = worker.wait_for("resumed", |e| e["type"] == "resumed");
    assert_eq!(resumed["stats"]["program_source"], "store", "{resumed}");
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(
        events.last().unwrap()["value"],
        expected_itinerary("Summer")
    );
}

#[test]
fn a_missing_entry_fails_cleanly() {
    // A start that names a hash nobody stored.
    let dirs = Dirs::new();
    let absent = ProgramHash::of(b"nobody stored this");
    let worker = Worker::start(
        &dirs,
        Project::Absent,
        "quick",
        r#"{"city":"Oslo"}"#,
        &["--program-hash", &absent.to_hex()],
    );
    let (code, events, _) = worker.finish();
    assert_eq!(hello_of(&events)["program_hash"], absent.to_hex());
    assert_failed_with(
        &events,
        code,
        &format!("program {absent} is not in the store"),
    );
    assert_eq!(events.len(), 2, "hello and failed: {events:#?}");

    // A resume on a site whose store is empty.
    let (hash, snapshot) = paused_itinerary(&dirs, "Summer");
    assert!(dirs.open_store().remove(hash).unwrap());
    let worker = Worker::resume(&dirs, Project::Absent, &snapshot);
    let (code, events, _) = worker.finish();
    assert_eq!(hello_of(&events)["program_hash"], hash.to_hex());
    assert_failed_with(
        &events,
        code,
        &format!("program {hash} is not in the store"),
    );

    // Neither a project nor a hash.
    let worker = Worker::start(&dirs, Project::Absent, "quick", r#"{"city":"Oslo"}"#, &[]);
    let (code, events, _) = worker.finish();
    assert!(hello_of(&events)["program_hash"].is_null());
    assert_failed_with(&events, code, "--project or --program-hash");
}

#[test]
fn a_hash_argument_that_names_a_path_is_refused() {
    let dirs = Dirs::new();
    let outside = dirs.home.path().join("outside.bamlprog");
    std::fs::write(&outside, b"not a program").unwrap();
    for hostile in [
        "../outside",
        "../../../../etc/passwd",
        "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855",
        "",
    ] {
        let worker = Worker::start(
            &dirs,
            Project::Absent,
            "quick",
            r#"{"city":"Oslo"}"#,
            &["--program-hash", hostile],
        );
        let (code, events, _) = worker.finish();
        assert!(hello_of(&events)["program_hash"].is_null());
        assert_failed_with(&events, code, "is not a program hash");
    }
    assert!(!dirs.store().exists(), "a refused hash creates nothing");
}

#[test]
fn a_corrupt_entry_falls_back_to_the_project_and_fails_without_one() {
    let dirs = Dirs::new();
    let (hash, snapshot) = paused_itinerary(&dirs, "Winter");
    let store = dirs.open_store();
    let entry = store.entry_path(hash);
    let good = std::fs::read(&entry).unwrap();

    let damage: [(&str, Vec<u8>); 3] = [
        ("one flipped bit", {
            let mut bytes = good.clone();
            let middle = bytes.len() / 2;
            bytes[middle] ^= 0x10;
            bytes
        }),
        ("a torn write", good[..good.len() / 3].to_vec()),
        ("an empty file", Vec::new()),
    ];
    for (what, bytes) in damage {
        // Without a project the run fails, and says which program is missing.
        std::fs::write(&entry, &bytes).unwrap();
        let worker = Worker::resume(&dirs, Project::Absent, &snapshot);
        let (code, events, stderr) = worker.finish();
        assert_failed_with(
            &events,
            code,
            &format!("program {hash} is not in the store"),
        );
        assert!(
            stderr.contains("corrupt"),
            "{what}: stderr names the reason: {stderr}"
        );

        // With a project the worker compiles, checks the hash, and repairs
        // the entry.
        let mut worker = Worker::resume(&dirs, Project::Given, &snapshot);
        let resumed = worker.wait_for("resumed", |e| e["type"] == "resumed");
        assert_eq!(
            resumed["stats"]["program_source"], "compile",
            "{what}: {resumed}"
        );
        let (code, events, stderr) = worker.finish();
        assert_eq!(code, 0, "{what}: stderr: {stderr}\nevents: {events:#?}");
        assert_eq!(
            events.last().unwrap()["value"],
            expected_itinerary("Winter")
        );
        assert_eq!(
            std::fs::read(&entry).unwrap(),
            good,
            "{what}: the entry is repaired"
        );
    }
}

#[test]
fn a_resume_refuses_a_store_entry_of_another_program() {
    let dirs = Dirs::new();
    let (hash, snapshot) = paused_itinerary(&dirs, "Summer");

    // A second, valid program: the same project with one more function.
    let source = dirs.project().join(SOURCE_FILE);
    let original = std::fs::read_to_string(&source).unwrap();
    std::fs::write(
        &source,
        format!("{original}\nfunction added_later() -> int {{ 1 }}\n"),
    )
    .unwrap();
    let other = store_program(&dirs);
    assert_ne!(other, hash, "another source is another program");

    // Its entry is put in the place of the first program's entry.
    let store = dirs.open_store();
    std::fs::copy(store.entry_path(other), store.entry_path(hash)).unwrap();
    assert!(matches!(
        store.get(hash),
        Lookup::Missing(Missing::Corrupt(_))
    ));

    let worker = Worker::resume(&dirs, Project::Absent, &snapshot);
    let (code, events, stderr) = worker.finish();
    assert_failed_with(
        &events,
        code,
        &format!("program {hash} is not in the store"),
    );
    assert!(
        stderr.contains(&other.to_hex()),
        "stderr names the found hash: {stderr}"
    );

    // A project that compiles to another program is no substitute either.
    let worker = Worker::resume(&dirs, Project::Given, &snapshot);
    let (code, events, _) = worker.finish();
    assert_failed_with(&events, code, "which is not the requested program");

    // The original project is: the worker compiles it and repairs the entry.
    std::fs::write(&source, original).unwrap();
    let worker = Worker::resume(&dirs, Project::Given, &snapshot);
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(
        events.last().unwrap()["value"],
        expected_itinerary("Summer")
    );
    assert!(store.get(hash).found().is_some());
}

#[test]
fn the_optimization_level_is_part_of_the_identity() {
    let dirs = Dirs::new();
    let level_0 = store_program(&dirs);
    let worker = Worker::start(
        &dirs,
        Project::Given,
        "quick",
        r#"{"city":"Oslo"}"#,
        &["--opt-level", "2"],
    );
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    let level_2 = hash_of(hello_of(&events));
    assert_ne!(level_0, level_2, "other bytes, other hash");

    // Asking for the level 0 program while compiling at level 2 is an error,
    // not a silent substitution. The store is emptied so that the worker has
    // to compile.
    assert!(dirs.open_store().remove(level_0).unwrap());
    let worker = Worker::start(
        &dirs,
        Project::Given,
        "quick",
        r#"{"city":"Oslo"}"#,
        &["--program-hash", &level_0.to_hex(), "--opt-level", "2"],
    );
    let (code, events, _) = worker.finish();
    assert_eq!(hello_of(&events)["program_hash"], level_0.to_hex());
    assert_failed_with(&events, code, "which is not the requested program");
    assert!(
        matches!(
            dirs.open_store().get(level_0),
            Lookup::Missing(Missing::NotFound)
        ),
        "a program that failed the check is not stored under the requested hash"
    );

    // At the matching level the worker compiles, verifies, and stores.
    let worker = Worker::start(
        &dirs,
        Project::Given,
        "quick",
        r#"{"city":"Oslo"}"#,
        &["--program-hash", &level_0.to_hex()],
    );
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(hello_of(&events)["program_source"], "compile");
    assert!(dirs.open_store().get(level_0).found().is_some());
}

#[test]
fn an_entry_of_another_runtime_build_is_treated_as_missing() {
    let dirs = Dirs::new();
    let hash = store_program(&dirs);
    let store = dirs.open_store();
    let program = store.get(hash).found().unwrap().into_bytes();

    // The same bytes, stored by another build only.
    assert!(store.remove(hash).unwrap());
    let other_build = ProgramStore::open(dirs.store(), "0.0.0-some-other-build")
        .unwrap()
        .with_durability(Durability::None);
    other_build.put(&program).unwrap();
    assert!(matches!(
        store.get(hash),
        Lookup::Missing(Missing::OtherBuild { .. })
    ));

    let worker = Worker::start(
        &dirs,
        Project::Absent,
        "quick",
        r#"{"city":"Oslo"}"#,
        &["--program-hash", &hash.to_hex()],
    );
    let (code, events, stderr) = worker.finish();
    assert_failed_with(
        &events,
        code,
        &format!("program {hash} is not in the store"),
    );
    assert!(stderr.contains("0.0.0-some-other-build"), "{stderr}");

    // With the project this build compiles the same bytes and records that.
    // Both builds are served from then on.
    let worker = Worker::start(
        &dirs,
        Project::Given,
        "quick",
        r#"{"city":"Oslo"}"#,
        &["--program-hash", &hash.to_hex()],
    );
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(hello_of(&events)["program_source"], "compile");
    assert!(store.get(hash).found().is_some());
    assert!(other_build.get(hash).found().is_some());
}

#[test]
fn a_start_that_cannot_store_its_program_fails() {
    let dirs = Dirs::new();
    // A file where the store directory should be.
    std::fs::write(dirs.store(), b"not a directory").unwrap();
    let worker = Worker::start(&dirs, Project::Given, "quick", r#"{"city":"Oslo"}"#, &[]);
    let (code, events, _) = worker.finish();
    assert_failed_with(&events, code, "cannot store program");
}

/// A store that a site server filled holds entries without build markers. A
/// start from the project on such a store, mounted read-only, finds the
/// program already published and must not fail because it cannot add a marker.
#[cfg(unix)]
#[test]
fn a_start_on_a_read_only_store_that_holds_the_program_succeeds() {
    use std::os::unix::fs::PermissionsExt as _;

    let dirs = Dirs::new();
    let hash = store_program(&dirs);
    let store = dirs.open_store();
    let marker = store.marker_path(hash, &bex_engine::durable::runtime_build());
    std::fs::remove_file(&marker).unwrap();
    let shard = marker.parent().unwrap().to_path_buf();
    let set_mode = |mode: u32| {
        std::fs::set_permissions(&shard, std::fs::Permissions::from_mode(mode)).unwrap();
        std::fs::set_permissions(dirs.store(), std::fs::Permissions::from_mode(mode)).unwrap();
    };
    set_mode(0o555);
    let worker = Worker::start(&dirs, Project::Given, "quick", r#"{"city":"Oslo"}"#, &[]);
    let (code, events, stderr) = worker.finish();
    set_mode(0o755);
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(hash_of(hello_of(&events)), hash);
    assert!(!marker.exists());
}

#[test]
fn the_program_hash_does_not_depend_on_where_the_project_lies() {
    // Two machines check the same sources out into different directories.
    // They must agree on the hash, or a program could not be shared.
    let here = Dirs::new();
    let there = Dirs::new();
    assert_ne!(here.project(), there.project());
    let hash = store_program(&here);
    assert_eq!(store_program(&there), hash);

    // And a changed source is another program.
    let source = there.project().join(SOURCE_FILE);
    let text = std::fs::read_to_string(&source).unwrap();
    std::fs::write(&source, text.replace("bring water", "bring a hat")).unwrap();
    assert_ne!(store_program(&there), hash);
}

#[test]
fn concurrent_workers_store_the_same_program_once() {
    let dirs = Dirs::new();
    let workers: Vec<_> = (0..4)
        .map(|_| Worker::start(&dirs, Project::Given, "quick", r#"{"city":"Oslo"}"#, &[]))
        .collect();
    let mut hashes = Vec::new();
    for worker in workers {
        let (code, events, stderr) = worker.finish();
        assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
        hashes.push(hash_of(hello_of(&events)));
    }
    hashes.dedup();
    assert_eq!(hashes.len(), 1, "a compile is deterministic: {hashes:?}");

    let store = dirs.open_store();
    assert!(store.get(hashes[0]).found().is_some());
    assert_eq!(store.entries().count(), 1);
    assert_eq!(store.remove_stale_temp_files(Duration::ZERO), 0);
}

#[test]
fn a_worker_without_a_store_behaves_as_before() {
    let dirs = Dirs::new();
    let mut command_args = vec!["--start", "quick", "--json-args", r#"{"city":"Oslo"}"#];
    let worker = Worker::spawn(&dirs, Project::Given, 1, &command_args);
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    let hello = hello_of(&events);
    assert_eq!(hello["program_source"], "compile");
    hash_of(hello);
    assert!(!dirs.store().exists());

    // `--program-hash` needs a store to look in: clap refuses the
    // combination before the worker starts.
    let hash = ProgramHash::of(b"x").to_hex();
    command_args.extend_from_slice(&["--program-hash", &hash]);
    let worker = Worker::spawn(&dirs, Project::Given, 1, &command_args);
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 2, "{events:#?}");
    assert!(stderr.contains("--program-store"), "{stderr}");
}

// ============================================================================
// `baml-cli program-store`: what a site server uses to serve and fetch programs
// ============================================================================

/// Run `baml-cli program-store --store <store> <args>` with `stdin` and return
/// the exit code and the JSON object on stdout.
fn program_store(dirs: &Dirs, store: &std::path::Path, args: &[&str], stdin: &str) -> (i32, Value) {
    let mut child = Command::new(common::baml_cli())
        .arg("program-store")
        .arg("--store")
        .arg(store)
        .args(args)
        .current_dir(dirs.empty_cwd())
        .env("HOME", dirs.home.path())
        .env("BAML_HOME", dirs.home.path().join(".baml-home"))
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("BAML_AGENT_SKILL_CHECK", "off")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn baml-cli program-store");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "stdout is one JSON object: {stdout:?}");
    (
        output.status.code().unwrap(),
        serde_json::from_str(lines[0]).unwrap(),
    )
}

#[test]
fn a_program_travels_to_a_site_that_never_compiled_it() {
    let dirs = Dirs::new();
    let hash = store_program(&dirs);
    let hex = hash.to_hex();
    let site_a = dirs.store();
    let site_b = dirs.home.path().join("programs-site-b");

    // Site B does not hold the program.
    let (code, has) = program_store(&dirs, &site_b, &["has", &hex], "");
    assert_eq!(code, 0);
    assert_eq!(
        has,
        json!({ "hash": hex, "usable": false, "reason": "no entry" })
    );

    // Site A serves it: the body of `GET /api/programs/:hash`.
    let (code, exported) = program_store(&dirs, &site_a, &["export", &hex], "");
    assert_eq!(code, 0, "{exported}");
    assert_eq!(exported["hash"], hex);
    assert_eq!(
        exported["runtime_build"],
        bex_engine::durable::runtime_build()
    );
    assert!(exported["program_base64"].as_str().unwrap().len() > 1000);

    // A body that was altered on the way is refused, and nothing is stored.
    let mut altered = exported.clone();
    let text = altered["program_base64"].as_str().unwrap();
    let swapped = if text.starts_with('A') { "B" } else { "A" };
    altered["program_base64"] = json!(format!("{swapped}{}", &text[1..]));
    let (code, refused) = program_store(&dirs, &site_b, &["import"], &altered.to_string());
    assert_eq!(code, 1, "{refused}");
    assert!(
        refused["error"].as_str().unwrap().contains("hash to"),
        "{refused}"
    );
    assert!(!site_b.exists());

    // The intact body is stored, once.
    let (code, imported) = program_store(&dirs, &site_b, &["import"], &exported.to_string());
    assert_eq!(code, 0, "{imported}");
    assert_eq!(
        imported,
        json!({ "hash": hex, "written": true, "usable": true })
    );
    let (_, again) = program_store(&dirs, &site_b, &["import"], &exported.to_string());
    assert_eq!(
        again,
        json!({ "hash": hex, "written": false, "usable": true })
    );

    // Site B runs a program that it never compiled and has no project for.
    std::fs::remove_dir_all(dirs.project()).unwrap();
    let worker = Worker::spawn(
        &dirs,
        Project::Absent,
        1,
        &[
            "--program-store",
            site_b.to_str().unwrap(),
            "--program-hash",
            &hex,
            "--start",
            "quick",
            "--json-args",
            r#"{"city":"Tromso"}"#,
        ],
    );
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(hello_of(&events)["program_source"], "store");
    assert_eq!(
        events.last().unwrap()["value"],
        json!({ "name": "Tromso", "nights": 1, "note": null })
    );

    // A program that another runtime build produced is kept and forwarded
    // under that build's name, and this build does not run it.
    let site_c = dirs.home.path().join("programs-site-c");
    let mut foreign = exported;
    foreign["runtime_build"] = json!("0.0.0-some-other-build");
    let (code, imported) = program_store(&dirs, &site_c, &["import"], &foreign.to_string());
    assert_eq!(code, 0, "{imported}");
    assert_eq!(
        imported,
        json!({ "hash": hex, "written": true, "usable": false })
    );
    let (_, forwarded) = program_store(&dirs, &site_c, &["export", &hex], "");
    assert_eq!(forwarded, foreign);

    // Hostile and unknown hashes.
    let (code, error) = program_store(&dirs, &site_a, &["export", "../../etc/passwd"], "");
    assert_eq!(code, 1);
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .contains("not a program hash"),
        "{error}"
    );
    let absent = ProgramHash::of(b"absent").to_hex();
    let (code, error) = program_store(&dirs, &site_a, &["export", &absent], "");
    assert_eq!(code, 1);
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .contains("is not in the store"),
        "{error}"
    );
}

// ============================================================================
// Measurement
// ============================================================================

/// Top-level names of the demo program. The generator renames them per copy.
const DEMO_NAMES: [&str; 5] = [
    "TripPlan",
    "remote_fetch_weather",
    "durable_plan_trip_parallel",
    "durable_plan_trip",
    "plan_trip",
];

/// Write a project that holds the demo program `copies` times: the original
/// file, and `copies - 1` files in which every top-level name has a suffix.
fn generate_scaled_demo(project: &std::path::Path, copies: usize) {
    let demo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../demo_durable/program/baml_src/trip.baml");
    let source = std::fs::read_to_string(&demo).unwrap();
    let src = project.join("baml_src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("trip.baml"), &source).unwrap();
    for copy in 1..copies {
        let mut text = source.clone();
        // Placeholders first, so that `plan_trip` does not rewrite the inside
        // of `durable_plan_trip`.
        for (index, name) in DEMO_NAMES.iter().enumerate() {
            text = text.replace(name, &format!("\u{1}{index}\u{2}"));
        }
        for (index, name) in DEMO_NAMES.iter().enumerate() {
            text = text.replace(&format!("\u{1}{index}\u{2}"), &format!("{name}_{copy}"));
        }
        std::fs::write(src.join(format!("trip_{copy}.baml")), text).unwrap();
    }
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

/// Prints `program_load_ms` with and without the program store, for the demo
/// program and for a project 20 times its size. A measurement, not a check:
///
/// ```text
/// cargo nextest run -p baml_cli --test worker_program_store_e2e \
///     --run-ignored only --no-capture measure_program_load
/// ```
///
/// `BAML_WORKER_BIN` selects another worker binary, for example an optimized
/// build. The default is the debug binary of the test run.
#[test]
#[ignore = "measurement: prints a table and asserts only that the store is faster"]
#[allow(clippy::print_stderr)]
fn measure_program_load() {
    const RUNS: usize = 7;
    let binary = std::env::var_os("BAML_WORKER_BIN").map_or_else(common::baml_cli, PathBuf::from);
    eprintln!("worker binary: {}", binary.display());
    eprintln!(
        "{:<8} {:<22} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "project", "path", "load_ms", "compile", "put", "read", "decode", "engine"
    );

    for (label, copies) in [("demo", 1usize), ("demo x20", 20)] {
        let home = tempfile::tempdir().unwrap();
        let project = home.path().join("project");
        generate_scaled_demo(&project, copies);
        let store = home.path().join("programs");
        let cwd = home.path().join("empty");
        std::fs::create_dir_all(&cwd).unwrap();

        // One `hello` of a worker whose stdin is closed at once: the run is
        // cancelled right after the program load, which is all that is
        // measured here.
        let hello = |args: &[&str]| -> Value {
            let output = Command::new(&binary)
                .arg("worker")
                .args(args)
                .args(["--run", "r-measure", "--auto-snapshot-ms", "0"])
                .arg("--snapshot-dir")
                .arg(home.path().join("snapshots"))
                .args([
                    "--start",
                    "remote_fetch_weather",
                    "--json-args",
                    r#"{"city":"Oslo"}"#,
                ])
                .current_dir(&cwd)
                .env("HOME", home.path())
                .env("BAML_HOME", home.path().join(".baml-home"))
                .env("BAML_CLI_ALLOW_DIRECT", "1")
                .env("BAML_AGENT_SKILL_CHECK", "off")
                .env_remove("BAML_LOG")
                .stdin(Stdio::null())
                .output()
                .unwrap();
            let stdout = String::from_utf8(output.stdout).unwrap();
            let first = stdout.lines().next().expect("a hello line");
            let hello: Value = serde_json::from_str(first).unwrap();
            assert!(hello["program_hash"].is_string(), "{stdout}");
            hello
        };
        let report = |path: &str, hellos: &[Value]| -> f64 {
            let column = |field: &str| -> String {
                let mut values: Vec<f64> =
                    hellos.iter().filter_map(|h| h[field].as_f64()).collect();
                if values.is_empty() {
                    "-".to_string()
                } else {
                    format!("{:.2}", median(&mut values))
                }
            };
            eprintln!(
                "{label:<8} {path:<22} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
                column("program_load_ms"),
                column("program_compile_ms"),
                column("program_store_put_ms"),
                column("program_store_read_ms"),
                column("program_decode_ms"),
                column("program_engine_ms"),
            );
            let mut loads: Vec<f64> = hellos
                .iter()
                .map(|h| h["program_load_ms"].as_f64().unwrap())
                .collect();
            median(&mut loads)
        };

        let project_arg = project.to_str().unwrap();
        let store_arg = store.to_str().unwrap();
        let compiled: Vec<Value> = (0..RUNS)
            .map(|_| hello(&["--project", project_arg]))
            .collect();
        let compile_ms = report("compile (no store)", &compiled);
        let first = hello(&["--project", project_arg, "--program-store", store_arg]);
        report("compile + first put", std::slice::from_ref(&first));
        let hash = first["program_hash"].as_str().unwrap().to_string();
        let stored: Vec<Value> = (0..RUNS)
            .map(|_| hello(&["--program-store", store_arg, "--program-hash", &hash]))
            .collect();
        assert!(stored.iter().all(|h| h["program_source"] == "store"));
        let store_ms = report("store (no project)", &stored);

        let entry = store.join(&hash[..2]).join(format!("{hash}.bamlprog"));
        eprintln!(
            "{label:<8} entry: {} bytes, store is {:.1}x faster",
            std::fs::metadata(entry).unwrap().len(),
            compile_ms / store_ms
        );
        assert!(
            store_ms < compile_ms,
            "the store path must not be slower than a compile"
        );
    }
}
