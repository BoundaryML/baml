// End-to-end tests for the hidden `baml-cli worker` subcommand (durable
// functions proof of concept).
//
// Each test starts the real binary on `tests/fixtures/worker_trip`, reads the
// JSON event lines from its stdout, answers events by writing JSON command
// lines to its stdin, and checks the protocol of
// `documents/durable-poc-contracts.md` section 2: one JSON object per stdout
// line and nothing else, the common event fields, the exit codes, positions
// with correct file and line values, log capture, remote calls, thread
// lifecycle events, and pause / resume across processes (`pause` writes a
// snapshot and exits 75; `--resume` continues the run in a new process).

mod common;

use std::{
    collections::BTreeSet,
    io::{BufRead as _, BufReader, Read as _, Write as _},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

use serde_json::{Value, json};

const RUN_ID: &str = "r-e2e";
const SOURCE_FILE: &str = "baml_src/trip.baml";
/// Generous: the worker compiles the project (and the stdlib) at startup.
const EVENT_TIMEOUT: Duration = Duration::from_secs(120);

/// Copy the fixture project into `dir`. The worker runs with the profiler's
/// defaults, which write a store under `<project>/.baml/`; a copy keeps the
/// source tree clean.
fn copy_fixture_project(dir: &Path) -> PathBuf {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/worker_trip");
    let project = dir.join("worker_trip");
    std::fs::create_dir_all(project.join("baml_src")).unwrap();
    std::fs::copy(source.join(SOURCE_FILE), project.join(SOURCE_FILE)).unwrap();
    project
}

/// The directories one run lives in across its worker processes.
struct RunDirs {
    home: tempfile::TempDir,
}

impl RunDirs {
    fn new() -> std::rc::Rc<Self> {
        let home = tempfile::tempdir().unwrap();
        copy_fixture_project(home.path());
        std::rc::Rc::new(Self { home })
    }

    fn project(&self) -> PathBuf {
        self.home.path().join("worker_trip")
    }

    fn snapshots(&self) -> PathBuf {
        self.home.path().join("snapshots")
    }
}

struct Worker {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: mpsc::Receiver<String>,
    events: Vec<Value>,
    segment: u64,
    /// Keeps the run's temporary directories alive.
    _dirs: std::rc::Rc<RunDirs>,
}

impl Worker {
    fn spawn(mode_args: &[&str]) -> Self {
        Self::spawn_segment(&RunDirs::new(), 1, mode_args)
    }

    /// Start segment `segment` of the run that lives in `dirs`. Automatic
    /// snapshots are off unless `mode_args` turns them on, so that the
    /// snapshot numbers of a test are its own.
    fn spawn_segment(dirs: &std::rc::Rc<RunDirs>, segment: u64, mode_args: &[&str]) -> Self {
        let home = &dirs.home;
        let auto: &[&str] = if mode_args.contains(&"--auto-snapshot-ms") {
            &[]
        } else {
            &["--auto-snapshot-ms", "0"]
        };
        let mut child = Command::new(common::baml_cli())
            .args(["worker", "--project"])
            .arg(dirs.project())
            .args(["--run", RUN_ID, "--segment", &segment.to_string()])
            .arg("--snapshot-dir")
            .arg(dirs.snapshots())
            .args(auto)
            .args(mode_args)
            .env("HOME", home.path())
            .env("BAML_HOME", home.path().join(".baml-home"))
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
            segment,
            _dirs: std::rc::Rc::clone(dirs),
        }
    }

    fn start(function: &str) -> Self {
        Self::spawn(&["--start", function, "--json-args", r#"{"city":"Lisbon"}"#])
    }

    /// Continue the run of `previous` from `snapshot` in the next segment.
    fn resume(dirs: &std::rc::Rc<RunDirs>, segment: u64, snapshot: &str, extra: &[&str]) -> Self {
        let mut args = vec!["--resume", snapshot];
        args.extend_from_slice(extra);
        Self::spawn_segment(dirs, segment, &args)
    }

    fn send(&mut self, command: &Value) {
        assert!(
            self.try_send(command),
            "the worker closed its stdin before the command was written"
        );
    }

    /// [`Self::send`] for a command that races the worker's own exit. False
    /// when the pipe was already closed, which is a broken pipe rather than a
    /// failure of the test.
    fn try_send(&mut self, command: &Value) -> bool {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        let written = writeln!(stdin, "{command}").and_then(|()| stdin.flush());
        match written {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => false,
            Err(error) => panic!("writing {command} to the worker failed: {error}"),
        }
    }

    fn close_stdin(&mut self) {
        self.stdin = None;
    }

    /// Read the next stdout line. Every line must be one JSON object that
    /// carries the common event fields. `None` at end of output.
    fn next_event(&mut self) -> Option<Value> {
        let line = match self.lines.recv_timeout(EVENT_TIMEOUT) {
            Ok(line) => line,
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = self.child.kill();
                panic!(
                    "timed out waiting for a worker event; got so far: {:#?}",
                    self.events
                );
            }
        };
        let event: Value = serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("stdout line is not JSON ({e}): {line:?}"));
        assert!(
            event.is_object(),
            "stdout line is not a JSON object: {line}"
        );
        assert_eq!(event["v"], 1, "{line}");
        assert_eq!(event["run"], RUN_ID, "{line}");
        assert_eq!(event["segment"], self.segment, "{line}");
        assert_eq!(event["pid"], self.child.id(), "{line}");
        assert!(event["ts"].is_u64(), "{line}");
        assert!(event["type"].is_string(), "{line}");
        self.events.push(event.clone());
        Some(event)
    }

    /// Read events until one satisfies `pred`; returns it.
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

    /// Drain the remaining output and return `(exit code, stderr)`.
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

fn is_type(kind: &'static str) -> impl Fn(&Value) -> bool {
    move |event| event["type"] == kind
}

fn of_type<'a>(events: &'a [Value], kind: &str) -> Vec<&'a Value> {
    events
        .iter()
        .filter(|event| event["type"] == kind)
        .collect()
}

fn index_of(events: &[Value], what: &str, pred: impl Fn(&Value) -> bool) -> usize {
    events
        .iter()
        .position(pred)
        .unwrap_or_else(|| panic!("no event: {what}; events: {events:#?}"))
}

fn expected_plan() -> Value {
    json!({
        "city": "Lisbon",
        "ideas": ["day 1 in Lisbon", "day 2 in Lisbon", "day 3 in Lisbon"],
        "weather": "sunny in Lisbon",
    })
}

#[test]
fn durable_run_answers_remote_call_and_completes() {
    let mut worker = Worker::start("durable_plan_trip");

    let hello = worker.next_event().expect("hello");
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["mode"], "start");
    assert_eq!(hello["function"], "durable_plan_trip");
    assert_eq!(hello["durable"], true);

    let call = worker.wait_for("remote_call", is_type("remote_call"));
    assert_eq!(call["call_id"], format!("{RUN_ID}-c1"));
    assert_eq!(call["function"], "remote_fetch_weather");
    assert_eq!(call["args"], json!({ "city": "Lisbon" }));
    worker.send(&json!({
        "type": "remote_result",
        "call_id": call["call_id"],
        "value": "sunny in Lisbon",
    }));

    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");

    let root = &of_type(&events, "thread_started")[0];
    assert_eq!(root["parent_thread"], Value::Null);
    let thread = root["thread"].clone();
    assert!(thread.is_u64());
    assert_eq!(call["thread"], thread);

    // Program output is reported as log events, attributed to the thread.
    let logs: Vec<_> = of_type(&events, "log")
        .iter()
        .map(|e| (e["stream"].clone(), e["text"].clone(), e["thread"].clone()))
        .collect();
    let expected_logs: Vec<_> = (1..=3)
        .map(|day| {
            (
                json!("stdout"),
                json!(format!("planning day {day}")),
                thread.clone(),
            )
        })
        .collect();
    assert_eq!(logs, expected_logs);

    // Positions: println on line 17 and sleep on line 18, three times, then
    // the remote call on line 22.
    let positions: Vec<_> = of_type(&events, "position")
        .iter()
        .map(|e| {
            assert_eq!(e["file"], SOURCE_FILE, "{e}");
            assert_eq!(e["function"], "durable_plan_trip", "{e}");
            assert_eq!(e["thread"], thread, "{e}");
            (
                e["line"].as_u64().unwrap(),
                e["reason"].as_str().unwrap().to_string(),
                e["op"].clone(),
            )
        })
        .collect();
    let mut expected_positions = Vec::new();
    for _ in 0..3 {
        expected_positions.push((17, "sysop".to_string(), json!("baml.io.println")));
        expected_positions.push((18, "sysop".to_string(), json!("baml.sys.sleep")));
    }
    expected_positions.push((22, "remote_call".to_string(), Value::Null));
    assert_eq!(positions, expected_positions);

    let received = index_of(
        &events,
        "remote_result_received",
        is_type("remote_result_received"),
    );
    assert_eq!(events[received]["call_id"], call["call_id"]);
    assert_eq!(events[received]["thread"], thread);
    assert!(index_of(&events, "remote_call", is_type("remote_call")) < received);

    assert_eq!(of_type(&events, "thread_ended")[0]["thread"], thread);
    let last = events.last().unwrap();
    assert_eq!(last["type"], "completed");
    assert_eq!(last["value"], expected_plan());
}

/// Contract section 10.3: the app names "the calling function and its call
/// site". The call is made in `fetch_via`, and the run's entry function is
/// `durable_nested_weather`, so the two differ and only the worker's own field
/// can tell them apart. The spelling is the one a `position` event uses.
#[test]
fn a_remote_call_names_the_function_that_made_it() {
    let mut worker = Worker::start("durable_nested_weather");

    let call = worker.wait_for("remote_call", is_type("remote_call"));
    assert_eq!(call["function"], "remote_fetch_weather");
    assert_eq!(call["caller"], "fetch_via", "{call}");
    assert_eq!(call["file"], SOURCE_FILE, "{call}");
    worker.send(&json!({
        "type": "remote_result",
        "call_id": call["call_id"],
        "value": "sunny in Lisbon via helper",
    }));

    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    // The caller and the call site are the frame of the `position` event that
    // accompanies the call (section 10.1).
    let position = of_type(&events, "position")
        .into_iter()
        .find(|e| e["reason"] == "remote_call")
        .expect("the call reports its position");
    assert_eq!(
        (
            call["caller"].clone(),
            call["file"].clone(),
            call["line"].clone()
        ),
        (
            position["function"].clone(),
            position["file"].clone(),
            position["line"].clone()
        ),
        "{call}\n{position}"
    );
    assert_eq!(events.last().unwrap()["type"], "completed");
}

#[test]
fn remote_function_started_as_root_runs_locally() {
    let worker = Worker::start("remote_fetch_weather");
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");

    assert_eq!(events[0]["type"], "hello");
    assert_eq!(events[0]["durable"], false);
    assert!(of_type(&events, "remote_call").is_empty(), "{events:#?}");
    let logs = of_type(&events, "log");
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0]["text"], "[cloud] looking up weather for Lisbon");
    let lines: Vec<_> = of_type(&events, "position")
        .iter()
        .map(|e| (e["function"].clone(), e["file"].clone(), e["line"].clone()))
        .collect();
    assert_eq!(
        lines,
        vec![
            (json!("remote_fetch_weather"), json!(SOURCE_FILE), json!(8)),
            (json!("remote_fetch_weather"), json!(SOURCE_FILE), json!(9)),
        ]
    );
    let last = events.last().unwrap();
    assert_eq!(last["type"], "completed");
    assert_eq!(last["value"], "sunny in Lisbon");
}

#[test]
fn spawned_remote_call_lets_the_parent_keep_running() {
    let mut worker = Worker::start("durable_plan_trip_parallel");
    let call = worker.wait_for("remote_call", is_type("remote_call"));
    // Hold the result back until the parent has finished its loop and parked
    // on `await pending`: the parent must make progress while the spawned
    // thread waits. The line is not asserted: the compiler's line table
    // currently attributes the `Await` instruction to the line that defines
    // `pending` (28) instead of the `await` line (36).
    worker.wait_for("the parent's await position", |e| {
        e["type"] == "position" && e["reason"] == "await"
    });
    worker.send(&json!({
        "type": "remote_result",
        "call_id": call["call_id"],
        "value": "sunny in Lisbon",
    }));
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");

    let started = of_type(&events, "thread_started");
    assert_eq!(started.len(), 2, "{events:#?}");
    let root = started[0]["thread"].clone();
    let child = started[1]["thread"].clone();
    assert_eq!(started[0]["parent_thread"], Value::Null);
    assert_eq!(started[1]["parent_thread"], root);
    assert_ne!(root, child);

    // The remote call belongs to the spawned thread and is positioned on the
    // `spawn { remote_fetch_weather(city) }` line.
    assert_eq!(call["thread"], child);
    let spawn_position = index_of(&events, "the spawned thread's position", |e| {
        e["type"] == "position" && e["thread"] == child
    });
    assert_eq!(events[spawn_position]["line"], 28);
    assert_eq!(events[spawn_position]["reason"], "remote_call");
    assert_eq!(
        events[spawn_position]["function"],
        "durable_plan_trip_parallel"
    );

    // Every log line comes from the root thread.
    for log in of_type(&events, "log") {
        assert_eq!(log["thread"], root, "{log}");
    }
    // The whole loop ran between the remote call and its result.
    let call_index = index_of(&events, "remote_call", is_type("remote_call"));
    let last_log = index_of(&events, "the parent's last log line", |e| {
        e["type"] == "log" && e["text"] == "planning day 3"
    });
    let await_index = index_of(&events, "the parent's await position", |e| {
        e["type"] == "position" && e["thread"] == root && e["reason"] == "await"
    });
    let received = index_of(
        &events,
        "remote_result_received",
        is_type("remote_result_received"),
    );
    assert!(call_index < last_log && last_log < await_index && await_index < received);

    // Both threads report their end before the terminal event, which is what
    // the worker guarantees by draining its threads. The order between them is
    // not guaranteed: the child settles the future the root awaits, and the
    // two tasks then finish independently.
    let ended: BTreeSet<u64> = of_type(&events, "thread_ended")
        .iter()
        .filter_map(|e| e["thread"].as_u64())
        .collect();
    let expected: BTreeSet<u64> = [&child, &root].iter().filter_map(|v| v.as_u64()).collect();
    assert_eq!(ended, expected);

    let last = events.last().unwrap();
    assert_eq!(last["type"], "completed");
    assert_eq!(last["value"], expected_plan());
}

#[test]
fn remote_error_is_thrown_into_the_caller() {
    // Uncaught: the run fails with a stack that points at the call site.
    let mut worker = Worker::start("durable_plan_trip");
    let call = worker.wait_for("remote_call", is_type("remote_call"));
    worker.send(&json!({
        "type": "remote_result",
        "call_id": call["call_id"],
        "error": "cloud is down",
    }));
    let (code, events, _) = worker.finish();
    assert_eq!(code, 1, "{events:#?}");
    let last = events.last().unwrap();
    assert_eq!(last["type"], "failed");
    assert!(
        last["error"].as_str().unwrap().contains("cloud is down"),
        "{last}"
    );
    assert_eq!(
        last["stack"][0],
        json!({ "function": "durable_plan_trip", "file": SOURCE_FILE, "line": 22 })
    );

    // Caught: BAML code handles it like any other thrown error.
    let mut worker = Worker::start("durable_safe_weather");
    let call = worker.wait_for("remote_call", is_type("remote_call"));
    worker.send(&json!({
        "type": "remote_result",
        "call_id": call["call_id"],
        "error": "cloud is down",
    }));
    let (code, events, _) = worker.finish();
    assert_eq!(code, 0, "{events:#?}");
    assert_eq!(events.last().unwrap()["value"], "unknown");
}

/// A thread that outlives the root function ends with the process. The
/// worker must still balance its `thread_started`, and the terminal event must
/// be the last line.
#[test]
fn unawaited_spawn_is_reported_ended_before_the_terminal_event() {
    let worker = Worker::start("durable_detached");
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");

    let last = events.last().unwrap();
    assert_eq!(last["type"], "completed", "{events:#?}");
    assert_eq!(last["value"], "done early");
    assert_eq!(of_type(&events, "completed").len(), 1);

    let mut started: Vec<u64> = of_type(&events, "thread_started")
        .iter()
        .map(|e| e["thread"].as_u64().unwrap())
        .collect();
    let mut ended: Vec<u64> = of_type(&events, "thread_ended")
        .iter()
        .map(|e| e["thread"].as_u64().unwrap())
        .collect();
    started.sort_unstable();
    ended.sort_unstable();
    assert_eq!(started, ended, "{events:#?}");
    // The spawned body never got to print.
    assert!(of_type(&events, "log").is_empty(), "{events:#?}");
}

#[test]
fn cancel_exits_130() {
    let mut worker = Worker::start("durable_plan_trip");
    worker.wait_for("remote_call", is_type("remote_call"));
    worker.send(&json!({ "type": "cancel" }));
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 130, "stderr: {stderr}\nevents: {events:#?}");
    assert!(of_type(&events, "completed").is_empty());
    assert!(of_type(&events, "failed").is_empty());
}

/// A supervisor that goes away closes the worker's stdin. The worker must not
/// stay behind as an orphan, unless a script asked for that.
#[test]
fn a_closed_stdin_cancels_the_run_unless_told_otherwise() {
    let mut worker = Worker::start("durable_plan_trip");
    worker.wait_for("remote_call", is_type("remote_call"));
    worker.close_stdin();
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 130, "stderr: {stderr}\nevents: {events:#?}");

    let mut worker = Worker::spawn(&[
        "--start",
        "remote_fetch_weather",
        "--json-args",
        r#"{"city":"Lisbon"}"#,
        "--ignore-stdin-eof",
    ]);
    worker.close_stdin();
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(events.last().unwrap()["value"], "sunny in Lisbon");
}

fn texts(events: &[Value]) -> Vec<String> {
    of_type(events, "log")
        .iter()
        .map(|e| e["text"].as_str().unwrap().to_string())
        .collect()
}

/// Send `pause`, expect `paused` as the terminal event and exit code 75, and
/// return the `paused` event with everything the segment printed.
fn pause_and_finish(mut worker: Worker) -> (Value, Vec<Value>) {
    worker.send(&json!({ "type": "pause" }));
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 75, "stderr: {stderr}\nevents: {events:#?}");
    let paused = events.last().unwrap().clone();
    assert_eq!(paused["type"], "paused", "{events:#?}");
    for field in [
        "pause_latency_ms",
        "walk_ms",
        "encode_ms",
        "compress_ms",
        "write_ms",
        "objects",
        "raw_bytes",
        "compressed_bytes",
        "program_bytes",
        "blocked_attempts",
    ] {
        assert!(paused["stats"][field].is_number(), "{field}: {paused}");
    }
    assert!(paused["stats"]["program_bytes"].as_u64().unwrap() > 0);
    // Every started thread is reported ended before the terminal event.
    assert_eq!(
        of_type(&events, "thread_started").len(),
        of_type(&events, "thread_ended").len(),
        "{events:#?}"
    );
    (paused, events)
}

fn read_state(paused: &Value) -> Value {
    let text = std::fs::read_to_string(paused["state_path"].as_str().unwrap()).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn local<'a>(frame: &'a Value, name: &str) -> &'a Value {
    frame["locals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|local| local["name"] == name)
        .unwrap_or_else(|| panic!("no local `{name}` in {frame:#}"))
}

#[test]
fn a_run_paused_in_its_loop_resumes_in_a_new_process() {
    let dirs = RunDirs::new();
    let mut worker = Worker::spawn_segment(
        &dirs,
        1,
        &[
            "--start",
            "durable_slow_trip",
            "--json-args",
            r#"{"city":"Lisbon"}"#,
        ],
    );
    let first_pid = worker.child.id();
    worker.wait_for("the second log line", |e| {
        e["type"] == "log" && e["text"] == "planning day 2"
    });
    let (paused, events) = pause_and_finish(worker);
    assert_eq!(texts(&events), ["planning day 1", "planning day 2"]);

    // Files: snap-1.bamlsnap and the state dump snap-1.json, in --snapshot-dir.
    let snapshot_path = paused["snapshot_path"].as_str().unwrap().to_string();
    assert_eq!(
        Path::new(&snapshot_path),
        dirs.snapshots().join("snap-1.bamlsnap")
    );
    assert_eq!(
        Path::new(paused["state_path"].as_str().unwrap()),
        dirs.snapshots().join("snap-1.json")
    );
    let file_len = std::fs::metadata(&snapshot_path).unwrap().len();
    assert!(file_len > 0 && file_len < 64 * 1024, "{file_len}");

    // The state dump (contract section 2.5): the paused line and the named
    // locals with their values.
    let state = read_state(&paused);
    assert_eq!(state["run"], RUN_ID);
    assert_eq!(state["segment"], 1);
    let thread = &state["threads"][0];
    assert_eq!(thread["parked"]["kind"], "sleep");
    let frame = &thread["frames"][0];
    assert_eq!(frame["function"], "durable_slow_trip");
    assert_eq!(frame["file"], SOURCE_FILE);
    assert_eq!(frame["line"], 62);
    assert_eq!(local(frame, "city")["value"]["preview"], "\"Lisbon\"");
    assert_eq!(local(frame, "day")["value"]["kind"], "int");
    assert_eq!(local(frame, "day")["value"]["preview"], "2");
    let ideas = &local(frame, "ideas")["value"];
    assert_eq!(ideas["kind"], "array");
    assert_eq!(ideas["children"].as_array().unwrap().len(), 1);
    assert_eq!(
        ideas["children"][0]["value"]["preview"],
        "\"day 1 in Lisbon\""
    );
    assert!(state["heap"]["objects"].as_u64().unwrap() > 0);

    // Segment 2: a new process continues the loop.
    let mut worker = Worker::resume(&dirs, 2, &snapshot_path, &[]);
    assert_ne!(worker.child.id(), first_pid);
    let hello = worker.next_event().expect("hello");
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["mode"], "resume");
    assert_eq!(hello["function"], "durable_slow_trip");
    assert_eq!(hello["durable"], true);
    let resumed = worker.next_event().expect("resumed");
    assert_eq!(resumed["type"], "resumed", "{resumed}");
    for field in ["program_load_ms", "decode_ms", "first_exec_ms"] {
        assert!(resumed["stats"][field].is_number(), "{field}: {resumed}");
    }
    assert!(resumed["stats"]["process_start_ms"].is_null());
    let started = worker.next_event().expect("thread_started");
    assert_eq!(started["type"], "thread_started");
    assert_eq!(started["parent_thread"], Value::Null);
    // The restored thread reports where it stands: inside the sleep.
    let position = worker.next_event().expect("position");
    assert_eq!(position["type"], "position", "{position}");
    assert_eq!(position["line"], 62);
    assert_eq!(position["op"], "baml.sys.sleep");

    let call = worker.wait_for("remote_call", is_type("remote_call"));
    assert_eq!(call["call_id"], format!("{RUN_ID}-c1"));
    worker.send(&json!({
        "type": "remote_result",
        "call_id": call["call_id"],
        "value": "sunny in Lisbon",
    }));
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    // Only the remaining iteration prints.
    assert_eq!(texts(&events), ["planning day 3"]);
    let last = events.last().unwrap();
    assert_eq!(last["type"], "completed");
    assert_eq!(last["value"], expected_plan());
}

#[test]
fn a_compute_loop_pauses_and_resumes() {
    let dirs = RunDirs::new();
    let mut worker = Worker::spawn_segment(
        &dirs,
        1,
        &[
            "--start",
            "durable_spin",
            "--json-args",
            r#"{"city":"Lisbon"}"#,
        ],
    );
    worker.wait_for("thread_started", is_type("thread_started"));
    let (paused, events) = pause_and_finish(worker);
    assert!(texts(&events).is_empty(), "paused before the loop ended");
    let state = read_state(&paused);
    assert_eq!(state["threads"][0]["parked"]["kind"], "runnable");
    let frame = &state["threads"][0]["frames"][0];
    assert_eq!(frame["function"], "durable_spin");
    let i: u64 = local(frame, "i")["value"]["preview"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(i < 4_000_000, "paused at i = {i}");

    let worker = Worker::resume(&dirs, 2, paused["snapshot_path"].as_str().unwrap(), &[]);
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(texts(&events), ["spun for Lisbon"]);
    let expected: u64 = (0..4_000_000u64).map(|i| i * 2).sum();
    assert_eq!(events.last().unwrap()["value"], expected);
}

/// Pause while the run waits for a remote result; the result arrives while
/// the run has no process and is handed to the next segment at startup.
#[test]
fn a_run_paused_in_a_remote_wait_resumes_with_the_stored_result() {
    let dirs = RunDirs::new();
    let mut worker = Worker::spawn_segment(
        &dirs,
        1,
        &[
            "--start",
            "durable_plan_trip",
            "--json-args",
            r#"{"city":"Lisbon"}"#,
        ],
    );
    let call = worker.wait_for("remote_call", is_type("remote_call"));
    let (paused, _) = pause_and_finish(worker);
    let state = read_state(&paused);
    let thread = &state["threads"][0];
    assert_eq!(thread["parked"]["kind"], "remote_call");
    assert!(
        thread["parked"]["detail"]
            .as_str()
            .unwrap()
            .contains(call["call_id"].as_str().unwrap()),
        "{thread}"
    );
    // The detail is the engine's JSON payload, unshortened.
    let detail: Value = serde_json::from_str(thread["parked"]["detail"].as_str().unwrap())
        .unwrap_or_else(|e| panic!("parked.detail is not JSON ({e}): {thread}"));
    assert_eq!(detail["call_id"], call["call_id"]);
    assert_eq!(thread["frames"][0]["line"], 22);
    assert_eq!(local(&thread["frames"][0], "day")["value"]["preview"], "4");

    let result = json!({ "call_id": call["call_id"], "value": "sunny in Lisbon" }).to_string();
    // A result for a call the run does not wait on is ignored.
    let stray = json!({ "call_id": "r-e2e-c99", "value": "stray" }).to_string();
    let worker = Worker::resume(
        &dirs,
        2,
        paused["snapshot_path"].as_str().unwrap(),
        &["--remote-result", &stray, "--remote-result", &result],
    );
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert!(stderr.contains("r-e2e-c99"), "{stderr}");
    // The call is not announced again; the result is reported as received by
    // the resumed segment's root thread.
    assert!(of_type(&events, "remote_call").is_empty(), "{events:#?}");
    let root = of_type(&events, "thread_started")[0]["thread"].clone();
    let received = of_type(&events, "remote_result_received");
    assert_eq!(received.len(), 1, "{events:#?}");
    assert_eq!(received[0]["call_id"], call["call_id"]);
    assert_eq!(received[0]["thread"], root);
    assert!(texts(&events).is_empty());
    // The resumed thread reports where it stands: still at the remote call.
    let positions = of_type(&events, "position");
    assert_eq!(positions[0]["line"], 22, "{events:#?}");
    assert_eq!(positions[0]["reason"], "remote_call");
    assert_eq!(positions[0]["thread"], root);
    let last = events.last().unwrap();
    assert_eq!(last["type"], "completed");
    assert_eq!(last["value"], expected_plan());
}

/// As above, but the result arrives after the resume, on stdin.
#[test]
fn a_resumed_remote_wait_takes_a_later_result_from_stdin() {
    let dirs = RunDirs::new();
    let mut worker = Worker::spawn_segment(
        &dirs,
        1,
        &[
            "--start",
            "durable_plan_trip",
            "--json-args",
            r#"{"city":"Lisbon"}"#,
        ],
    );
    let call = worker.wait_for("remote_call", is_type("remote_call"));
    let (paused, _) = pause_and_finish(worker);

    let mut worker = Worker::resume(&dirs, 2, paused["snapshot_path"].as_str().unwrap(), &[]);
    worker.wait_for("thread_started", is_type("thread_started"));
    std::thread::sleep(Duration::from_millis(300));
    worker.send(&json!({ "type": "remote_result", "call_id": "r-e2e-c99", "value": "stray" }));
    worker.send(&json!({
        "type": "remote_result",
        "call_id": call["call_id"],
        "value": "sunny in Lisbon",
    }));
    // A second result for the same call is ignored.
    worker.send(&json!({
        "type": "remote_result",
        "call_id": call["call_id"],
        "value": "rain in Lisbon",
    }));
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(of_type(&events, "remote_result_received").len(), 1);
    assert_eq!(events.last().unwrap()["value"], expected_plan());
}

/// Two pauses: in the loop (segment 1) and in the remote wait (segment 2).
/// Segment 3 finishes the run. The call id counter survives both hops.
#[test]
fn a_run_survives_two_pauses() {
    let dirs = RunDirs::new();
    let mut worker = Worker::spawn_segment(
        &dirs,
        1,
        &[
            "--start",
            "durable_slow_trip",
            "--json-args",
            r#"{"city":"Lisbon"}"#,
        ],
    );
    worker.wait_for("the first log line", is_type("log"));
    let (first, _) = pause_and_finish(worker);
    assert!(
        first["snapshot_path"]
            .as_str()
            .unwrap()
            .ends_with("snap-1.bamlsnap")
    );

    let mut worker = Worker::resume(&dirs, 2, first["snapshot_path"].as_str().unwrap(), &[]);
    let call = worker.wait_for("remote_call", is_type("remote_call"));
    assert_eq!(call["call_id"], format!("{RUN_ID}-c1"));
    let (second, _) = pause_and_finish(worker);
    assert!(
        second["snapshot_path"]
            .as_str()
            .unwrap()
            .ends_with("snap-2.bamlsnap")
    );
    let state = read_state(&second);
    assert_eq!(state["segment"], 2);
    assert_eq!(state["threads"][0]["parked"]["kind"], "remote_call");

    let result = json!({ "call_id": call["call_id"], "value": "sunny in Lisbon" }).to_string();
    let worker = Worker::resume(
        &dirs,
        3,
        second["snapshot_path"].as_str().unwrap(),
        &["--remote-result", &result],
    );
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(events[0]["mode"], "resume");
    assert_eq!(events[0]["function"], "durable_slow_trip");
    assert_eq!(events.last().unwrap()["value"], expected_plan());
}

/// Phase 3: a pause of the `spawn` variant suspends both threads. The spawned
/// thread waits on the remote call, the parent is in its loop, and the pending
/// future between them is part of the snapshot. The next segment restores both
/// threads with the ids they had and takes the remote result.
#[test]
fn pausing_the_spawn_variant_suspends_both_threads_and_the_run_resumes() {
    let dirs = RunDirs::new();
    let mut worker = Worker::spawn_segment(
        &dirs,
        1,
        &[
            "--start",
            "durable_plan_trip_parallel",
            "--json-args",
            r#"{"city":"Lisbon"}"#,
        ],
    );
    let call = worker.wait_for("remote_call", is_type("remote_call"));
    let (paused, events) = pause_and_finish(worker);
    assert!(of_type(&events, "blocked").is_empty(), "{events:#?}");
    assert_eq!(paused["stats"]["threads"], 2);
    let state = read_state(&paused);
    let threads = state["threads"].as_array().unwrap();
    assert_eq!(threads.len(), 2);
    let child = threads
        .iter()
        .find(|thread| !thread["parent_thread"].is_null())
        .expect("the spawned thread");
    assert_eq!(child["parked"]["kind"], "remote_call");
    assert!(child["settles_future"].is_number());

    let result = json!({ "call_id": call["call_id"], "value": "sunny in Lisbon" }).to_string();
    let worker = Worker::resume(
        &dirs,
        2,
        paused["snapshot_path"].as_str().unwrap(),
        &["--remote-result", &result],
    );
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    let started = of_type(&events, "thread_started");
    assert_eq!(started.len(), 2, "{events:#?}");
    assert!(
        started[0]["parent_thread"].is_null(),
        "the root reports first"
    );
    assert_eq!(started[1]["thread"], child["thread"]);
    assert_eq!(started[1]["parent_thread"], child["parent_thread"]);
    assert!(
        of_type(&events, "remote_call").is_empty(),
        "the call is not announced again: {events:#?}"
    );
    let last = events.last().unwrap();
    assert_eq!(last["type"], "completed");
    assert_eq!(last["value"], expected_plan());
}

/// A durable run snapshots itself while it runs. After the process is lost
/// (SIGKILL) the latest automatic snapshot continues the run.
#[test]
fn an_automatic_snapshot_recovers_a_run_whose_process_was_lost() {
    let dirs = RunDirs::new();
    let mut worker = Worker::spawn_segment(
        &dirs,
        1,
        &[
            "--start",
            "durable_slow_trip",
            "--json-args",
            r#"{"city":"Lisbon"}"#,
            "--auto-snapshot-ms",
            "150",
        ],
    );
    worker.wait_for("the second log line", |e| {
        e["type"] == "log" && e["text"] == "planning day 2"
    });
    // The first snapshot taken after day 2 started.
    let snapshot = worker.wait_for("an automatic snapshot", is_type("snapshot"));
    assert_eq!(snapshot["automatic"], true);
    assert!(snapshot["stats"]["objects"].is_number());
    // Contract section 7.1: nobody requested this snapshot.
    assert_eq!(snapshot["stats"]["pause_latency_ms"], Value::Null);
    assert!(snapshot["stats"]["park_ms"].is_number(), "{snapshot}");
    let logs_before = texts(&worker.events).len();
    worker.child.kill().unwrap();
    let status = worker.child.wait().unwrap();
    // The process must not have ended by one of the worker's own exits, so
    // that the run is recovered from a snapshot the worker wrote on its own.
    // The platforms report a kill differently: on Unix the process carries a
    // signal and no exit code, while on Windows `Child::kill` is
    // `TerminateProcess(1)`, which is indistinguishable from an exit code of
    // 1. Naming the worker's own codes covers both without a cfg.
    let code = status.code();
    assert!(
        !matches!(code, Some(0 | 75 | 130)),
        "the process was killed, so it did not complete, pause or cancel: {code:?}"
    );

    let mut worker = Worker::resume(&dirs, 2, snapshot["snapshot_path"].as_str().unwrap(), &[]);
    let call = worker.wait_for("remote_call", is_type("remote_call"));
    worker.send(&json!({
        "type": "remote_result",
        "call_id": call["call_id"],
        "value": "sunny in Lisbon",
    }));
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(events.last().unwrap()["value"], expected_plan());
    // The `snapshot` event can be written after a log line that the snapshot
    // does not contain yet (the run continues while the event is reported),
    // so the resumed segment may repeat the last line seen, and no more.
    assert!(texts(&events).len() <= 3 - (logs_before - 1), "{events:#?}");

    // A run whose function name lacks the marker never snapshots itself.
    let worker = Worker::spawn(&[
        "--start",
        "remote_fetch_weather",
        "--json-args",
        r#"{"city":"Lisbon"}"#,
        "--auto-snapshot-ms",
        "50",
    ]);
    let (code, events, _) = worker.finish();
    assert_eq!(code, 0);
    assert!(of_type(&events, "snapshot").is_empty(), "{events:#?}");
}

#[test]
fn resume_refuses_a_missing_snapshot_and_a_changed_program() {
    let worker = Worker::spawn(&["--resume", "no-such-file.bamlsnap"]);
    let (code, events, _) = worker.finish();
    assert_eq!(code, 1);
    assert_eq!(events[0]["type"], "hello");
    assert_eq!(events[0]["mode"], "resume");
    let last = events.last().unwrap();
    assert_eq!(last["type"], "failed");
    assert!(
        last["error"]
            .as_str()
            .unwrap()
            .contains("no-such-file.bamlsnap"),
        "{last}"
    );

    // The project changes between the pause and the resume.
    let dirs = RunDirs::new();
    let mut worker = Worker::spawn_segment(
        &dirs,
        1,
        &[
            "--start",
            "durable_slow_trip",
            "--json-args",
            r#"{"city":"Lisbon"}"#,
        ],
    );
    worker.wait_for("the first log line", is_type("log"));
    let (paused, _) = pause_and_finish(worker);
    let source = dirs.project().join(SOURCE_FILE);
    let mut text = std::fs::read_to_string(&source).unwrap();
    text.push_str("\nfunction added_later() -> int { 1 }\n");
    std::fs::write(&source, text).unwrap();
    let worker = Worker::resume(&dirs, 2, paused["snapshot_path"].as_str().unwrap(), &[]);
    let (code, events, _) = worker.finish();
    assert_eq!(code, 1, "{events:#?}");
    assert_eq!(events[0]["function"], "durable_slow_trip");
    let last = events.last().unwrap();
    assert_eq!(last["type"], "failed");
    assert!(
        last["error"]
            .as_str()
            .unwrap()
            .contains("program hash differs"),
        "{last}"
    );
}

#[test]
fn unknown_function_fails_with_exit_code_1() {
    let worker = Worker::start("no_such_function");
    let (code, events, _) = worker.finish();
    assert_eq!(code, 1);
    let last = events.last().unwrap();
    assert_eq!(last["type"], "failed");
    assert!(
        last["error"].as_str().unwrap().contains("no_such_function"),
        "{last}"
    );
}

/// Names of the files in the snapshot directory, sorted.
fn snapshot_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The VM checks for an early yield right after a call and right after a
/// return. A pause that landed there wrote a snapshot that no process could
/// resume. The run is moved through several processes while it is inside a
/// loop that makes calls, so that some pauses land on such a boundary.
#[test]
fn a_compute_loop_with_calls_survives_a_chain_of_pauses() {
    let dirs = RunDirs::new();
    let mut worker = Worker::spawn_segment(
        &dirs,
        1,
        &[
            "--start",
            "durable_spin_calls",
            "--json-args",
            r#"{"city":"Lisbon"}"#,
        ],
    );
    let mut tops = Vec::new();
    for segment in 2..=6u64 {
        worker.wait_for("thread_started", is_type("thread_started"));
        // Vary where in the loop the pause lands.
        std::thread::sleep(Duration::from_millis(7 * segment));
        let (paused, events) = pause_and_finish(worker);
        assert!(texts(&events).is_empty(), "paused before the loop ended");
        let state = read_state(&paused);
        assert_eq!(state["threads"][0]["parked"]["kind"], "runnable");
        tops.push(state["threads"][0]["frames"][0]["function"].clone());
        worker = Worker::resume(
            &dirs,
            segment,
            paused["snapshot_path"].as_str().unwrap(),
            &[],
        );
    }
    let (code, events, stderr) = worker.finish();
    assert_eq!(
        code, 0,
        "paused in {tops:?}\nstderr: {stderr}\nevents: {events:#?}"
    );
    assert_eq!(texts(&events), ["spun with calls for Lisbon"]);
    assert_eq!(events.last().unwrap()["value"], 500_000);
}

/// A compute loop has no sys-op or await at which the engine would notice a
/// cancellation. `cancel` and a closed stdin still end the worker promptly,
/// through the engine (the VM is asked to yield), not through the worker's
/// exit-anyway timer.
#[test]
fn cancel_and_a_closed_stdin_end_a_run_in_a_compute_loop() {
    for close_stdin in [false, true] {
        let mut worker = Worker::start("durable_endless");
        worker.wait_for("thread_started", is_type("thread_started"));
        std::thread::sleep(Duration::from_millis(200));
        let requested = std::time::Instant::now();
        if close_stdin {
            worker.close_stdin();
        } else {
            worker.send(&json!({ "type": "cancel" }));
        }
        let (code, events, stderr) = worker.finish();
        assert_eq!(code, 130, "stderr: {stderr}\nevents: {events:#?}");
        assert_eq!(events.last().unwrap()["type"], "cancelled", "{events:#?}");
        assert!(
            requested.elapsed() < Duration::from_secs(10),
            "the worker took {:?} to exit",
            requested.elapsed()
        );
        assert!(
            !stderr.contains("did not end within"),
            "the engine did not end the run by itself: {stderr}"
        );
        assert_eq!(
            of_type(&events, "thread_started").len(),
            of_type(&events, "thread_ended").len(),
            "{events:#?}"
        );
    }
}

/// A spawned thread whose future was dropped is invisible to the snapshot
/// writer through any value. It is a thread of the run all the same: the
/// pause does not wait for it, the snapshot holds both threads, and the
/// resumed run ends the same way.
#[test]
fn a_forgotten_spawned_thread_is_part_of_the_snapshot() {
    let dirs = RunDirs::new();
    let mut worker = Worker::spawn_segment(
        &dirs,
        1,
        &[
            "--start",
            "durable_forgetful",
            "--json-args",
            r#"{"city":"Lisbon"}"#,
        ],
    );
    worker.wait_for("the spawned thread", |e| {
        e["type"] == "thread_started" && !e["parent_thread"].is_null()
    });
    let (paused, events) = pause_and_finish(worker);
    assert!(of_type(&events, "blocked").is_empty(), "{events:#?}");
    assert_eq!(paused["stats"]["blocked_attempts"], 0);
    assert_eq!(paused["stats"]["threads"], 2);
    let state = read_state(&paused);
    assert_eq!(state["threads"].as_array().unwrap().len(), 2);

    let worker = Worker::resume(&dirs, 2, paused["snapshot_path"].as_str().unwrap(), &[]);
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(events.last().unwrap()["value"], "Lisbon after 8");
}

/// When the root function has returned the run is over, even while a thread
/// it never awaited still runs. A `pause` then writes nothing.
#[test]
fn a_pause_after_the_root_returned_writes_no_snapshot() {
    let dirs = RunDirs::new();
    let mut worker = Worker::spawn_segment(
        &dirs,
        1,
        &[
            "--start",
            "durable_detached",
            "--json-args",
            r#"{"city":"Lisbon"}"#,
        ],
    );
    // The root returns right after the spawn. Under load it can return before
    // the task of the spawned thread was scheduled. That thread is then never
    // reported, and the worker completes at once. Otherwise the worker waits
    // for the thread, and the pause arrives during that wait.
    let seen = worker.wait_for("the spawned thread or the end of the run", |e| {
        (e["type"] == "thread_started" && !e["parent_thread"].is_null()) || e["type"] == "completed"
    });
    if seen["type"] == "thread_started" {
        std::thread::sleep(Duration::from_millis(300));
        // The spawned thread can finish inside that pause, and the worker then
        // completes and closes its stdin before the command is written. Either
        // way no snapshot is written, which is what this test is about.
        worker.try_send(&json!({ "type": "pause" }));
    }
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(events.last().unwrap()["type"], "completed");
    assert_eq!(events.last().unwrap()["value"], "done early");
    assert!(of_type(&events, "paused").is_empty());
    assert_eq!(
        snapshot_files(&dirs.snapshots()),
        Vec::<String>::new(),
        "{events:#?}"
    );
}

/// Output without a newline is part of the run state: the segment that pauses
/// does not report it, and the next segment completes the line.
#[test]
fn an_unfinished_output_line_is_completed_by_the_next_segment() {
    let dirs = RunDirs::new();
    let mut worker = Worker::spawn_segment(
        &dirs,
        1,
        &[
            "--start",
            "durable_split_line",
            "--json-args",
            r#"{"city":"Lisbon"}"#,
        ],
    );
    worker.wait_for("the sleep", |e| {
        e["type"] == "position" && e["op"] == "baml.sys.sleep"
    });
    let (paused, events) = pause_and_finish(worker);
    assert!(texts(&events).is_empty(), "{events:#?}");

    let worker = Worker::resume(&dirs, 2, paused["snapshot_path"].as_str().unwrap(), &[]);
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(texts(&events), ["hello Lisbon"]);
}

/// A `pause` that arrives while an automatic snapshot is in progress gets its
/// own snapshot number. No two events name the same files.
#[test]
fn a_pause_that_overlaps_automatic_snapshots_gets_its_own_number() {
    let dirs = RunDirs::new();
    let mut worker = Worker::spawn_segment(
        &dirs,
        1,
        &[
            "--start",
            "durable_slow_trip",
            "--json-args",
            r#"{"city":"Lisbon"}"#,
            "--auto-snapshot-ms",
            "1",
        ],
    );
    worker.wait_for("the first log line", is_type("log"));
    std::thread::sleep(Duration::from_millis(80));
    let (paused, events) = pause_and_finish(worker);
    let mut paths: Vec<String> = of_type(&events, "snapshot")
        .iter()
        .map(|e| e["snapshot_path"].as_str().unwrap().to_string())
        .collect();
    assert!(!paths.is_empty(), "{events:#?}");
    paths.push(paused["snapshot_path"].as_str().unwrap().to_string());
    let distinct: std::collections::BTreeSet<&String> = paths.iter().collect();
    assert_eq!(distinct.len(), paths.len(), "{paths:?}");
    // The paused snapshot is the newest file and it resumes.
    let header_n = paths.len();
    assert!(
        paused["snapshot_path"]
            .as_str()
            .unwrap()
            .ends_with(&format!("snap-{header_n}.bamlsnap")),
        "{paths:?}"
    );
}

/// A snapshot that cannot be stored does not end the pause request: the
/// worker reports `blocked` and tries again, and the pause succeeds once the
/// directory is usable.
#[test]
fn a_snapshot_that_cannot_be_stored_is_retried() {
    let dirs = RunDirs::new();
    // A regular file where the snapshot directory should be.
    std::fs::write(dirs.snapshots(), b"in the way").unwrap();
    let mut worker = Worker::spawn_segment(
        &dirs,
        1,
        &[
            "--start",
            "durable_slow_trip",
            "--json-args",
            r#"{"city":"Lisbon"}"#,
        ],
    );
    worker.wait_for("the first log line", is_type("log"));
    worker.send(&json!({ "type": "pause" }));
    let blocked = worker.wait_for("blocked", is_type("blocked"));
    assert!(
        blocked["reason"]
            .as_str()
            .unwrap()
            .contains("could not be stored"),
        "{blocked}"
    );
    std::fs::remove_file(dirs.snapshots()).unwrap();

    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 75, "stderr: {stderr}\nevents: {events:#?}");
    let paused = events.last().unwrap();
    assert_eq!(paused["type"], "paused", "{events:#?}");
    assert_eq!(of_type(&events, "blocked").len(), 1, "one per reason");
    assert!(paused["stats"]["blocked_attempts"].as_u64().unwrap() >= 1);
    assert!(Path::new(paused["snapshot_path"].as_str().unwrap()).exists());
}

/// Phase 3, contract 9.2: `race` over three remote calls. The pause lands
/// while all three racers wait, the next segment gets one result, and the two
/// losers are cancelled in their remote waits: the worker reports
/// `remote_cancel` for exactly their calls and ignores results that arrive
/// for them afterwards. Arguments and the result are class values.
#[test]
fn race_losers_report_remote_cancel_after_a_resume() {
    let dirs = RunDirs::new();
    let mut worker = Worker::spawn_segment(
        &dirs,
        1,
        &[
            "--start",
            "durable_forecast_race",
            "--json-args",
            r#"{"city":"Lisbon"}"#,
        ],
    );
    let mut calls = Vec::new();
    while calls.len() < 3 {
        calls.push(worker.wait_for("remote_call", is_type("remote_call")));
    }
    let (paused, events) = pause_and_finish(worker);
    assert!(of_type(&events, "blocked").is_empty(), "{events:#?}");
    assert_eq!(
        paused["stats"]["threads"], 5,
        "root, three racers, the race"
    );

    let winner = calls
        .iter()
        .find(|call| call["args"]["source"] == "medium")
        .expect("the medium call");
    let forecast = json!({ "city": "Lisbon", "source": "medium", "degrees": 19 });
    let result = json!({ "call_id": winner["call_id"], "value": forecast }).to_string();
    let mut worker = Worker::resume(
        &dirs,
        2,
        paused["snapshot_path"].as_str().unwrap(),
        &["--remote-result", &result],
    );
    let mut cancelled = Vec::new();
    while cancelled.len() < 2 {
        cancelled.push(worker.wait_for("remote_cancel", is_type("remote_cancel")));
    }
    // A late result for an abandoned call changes nothing.
    worker.send(&json!({
        "type": "remote_result",
        "call_id": cancelled[0]["call_id"],
        "value": { "city": "Lisbon", "source": "late", "degrees": 1 },
    }));
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    let mut cancelled_ids: Vec<&str> = cancelled
        .iter()
        .map(|event| event["call_id"].as_str().unwrap())
        .collect();
    cancelled_ids.sort_unstable();
    let mut loser_ids: Vec<&str> = calls
        .iter()
        .filter(|call| call["call_id"] != winner["call_id"])
        .map(|call| call["call_id"].as_str().unwrap())
        .collect();
    loser_ids.sort_unstable();
    assert_eq!(cancelled_ids, loser_ids);
    for event in &cancelled {
        assert!(event["thread"].is_number(), "{event}");
    }
    assert_eq!(of_type(&events, "remote_result_received").len(), 1);
    let last = events.last().unwrap();
    assert_eq!(last["type"], "completed");
    assert_eq!(last["value"], forecast);
}

// ============================================================================
// Durable sleep and remote cancellation (contract section 9.2)
// ============================================================================

fn now_ms() -> u64 {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

fn sleep_until_ms(at_ts: u64) {
    std::thread::sleep(Duration::from_millis(at_ts.saturating_sub(now_ms()) + 50));
}

/// Start `function` with `json_args` and the self-suspend threshold `ms`.
fn start_with_threshold(
    dirs: &std::rc::Rc<RunDirs>,
    function: &str,
    json_args: &str,
    ms: &str,
) -> Worker {
    Worker::spawn_segment(
        dirs,
        1,
        &[
            "--start",
            function,
            "--json-args",
            json_args,
            "--sleep-suspend-ms",
            ms,
        ],
    )
}

/// Drain a worker that suspended itself: exit code 75, `paused` as the last
/// event. Returns the `paused` event and all events.
fn finish_self_suspended(worker: Worker) -> (Value, Vec<Value>) {
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 75, "stderr: {stderr}\nevents: {events:#?}");
    let paused = events.last().unwrap().clone();
    assert_eq!(paused["type"], "paused", "{events:#?}");
    assert_eq!(of_type(&events, "paused").len(), 1);
    assert_eq!(paused["wake"]["reason"], "sleep", "{paused}");
    (paused, events)
}

fn quote(vendor: &str, cabin: &str, nights: u64) -> Value {
    json!({
        "vendor": vendor,
        "cabin": cabin,
        "total": { "amount": nights * 111, "currency": "EUR" },
        "perks": ["wifi", format!("{nights} breakfasts")],
        "extras": { "bags": nights, "seat": 12 },
        "note": if nights > 2 { json!(format!("long stay at {vendor}")) } else { Value::Null },
    })
}

/// A durable run in a long sleep suspends itself: `paused` carries `wake`,
/// the process exits 75, and nobody sent a command. A resume after the
/// deadline completes the sleep at once.
#[test]
fn a_long_sleep_suspends_the_run_and_a_resume_after_the_deadline_completes_at_once() {
    let dirs = RunDirs::new();
    let worker = start_with_threshold(&dirs, "durable_nap", r#"{"ms":1500}"#, "400");
    let (paused, events) = finish_self_suspended(worker);
    assert_eq!(texts(&events), ["going to sleep"]);
    assert!(of_type(&events, "blocked").is_empty(), "{events:#?}");
    assert!(of_type(&events, "pausing").is_empty(), "{events:#?}");

    let wake = &paused["wake"];
    let remaining = wake["remaining_ms"].as_u64().unwrap();
    let at_ts = wake["at_ts"].as_u64().unwrap();
    assert!((400..=1500).contains(&remaining), "{paused}");
    // `remaining_ms` was measured when the snapshot was written, shortly
    // before the event.
    let ts = paused["ts"].as_u64().unwrap();
    assert!(
        at_ts >= ts && at_ts - remaining <= ts && ts - (at_ts - remaining) < 1000,
        "{paused}"
    );
    assert_eq!(paused["stats"]["threads"], 1);
    assert!(paused["stats"]["pause_latency_ms"].is_number());
    let state = read_state(&paused);
    assert_eq!(state["threads"][0]["parked"]["kind"], "sleep");
    assert_eq!(
        snapshot_files(&dirs.snapshots()),
        ["snap-1.bamlsnap", "snap-1.json"]
    );

    sleep_until_ms(at_ts);
    let worker = Worker::resume(
        &dirs,
        2,
        paused["snapshot_path"].as_str().unwrap(),
        &["--sleep-suspend-ms", "400"],
    );
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(texts(&events), ["woke up"]);
    let last = events.last().unwrap();
    assert_eq!(last["type"], "completed");
    assert_eq!(last["value"], "slept");
    let resumed = &events[index_of(&events, "resumed", is_type("resumed"))];
    let took = last["ts"].as_u64().unwrap() - resumed["ts"].as_u64().unwrap();
    assert!(took < 1000, "the expired sleep took {took} ms: {events:#?}");
}

/// A manual resume long before the deadline: the new worker sees a sleep that
/// is still over the threshold and suspends again, with the same deadline. A
/// resume with a threshold above the remaining time sleeps in the process.
#[test]
fn an_early_resume_suspends_again_until_the_remaining_sleep_is_short() {
    let dirs = RunDirs::new();
    // The sleep is long compared with the start of a worker process, which
    // compiles the project, so that the early resume really is early.
    let worker = start_with_threshold(&dirs, "durable_nap", r#"{"ms":20000}"#, "300");
    let (first, _) = finish_self_suspended(worker);

    let worker = Worker::resume(
        &dirs,
        2,
        first["snapshot_path"].as_str().unwrap(),
        &["--sleep-suspend-ms", "300"],
    );
    let (second, events) = finish_self_suspended(worker);
    assert!(texts(&events).is_empty(), "{events:#?}");
    assert_eq!(second["wake"]["at_ts"], first["wake"]["at_ts"]);
    assert!(
        second["wake"]["remaining_ms"].as_u64().unwrap()
            < first["wake"]["remaining_ms"].as_u64().unwrap()
    );
    assert_ne!(second["snapshot_path"], first["snapshot_path"]);

    // The rest of the sleep is below this threshold: the worker waits.
    let worker = Worker::resume(
        &dirs,
        3,
        second["snapshot_path"].as_str().unwrap(),
        &["--sleep-suspend-ms", "60000"],
    );
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert!(of_type(&events, "paused").is_empty());
    let last = events.last().unwrap();
    assert_eq!(last["value"], "slept");
    assert!(
        last["ts"].as_u64().unwrap() + 20 >= first["wake"]["at_ts"].as_u64().unwrap(),
        "the sleep did not end early: {events:#?}"
    );
}

/// The parent sleeps while four remote calls are outstanding: the run
/// suspends itself with all five threads. The results arrive while the run
/// has no process and are handed to the next worker in another order, one of
/// them twice. The report holds the nested class values of every call.
#[test]
fn a_fan_out_suspends_itself_and_completes_from_shuffled_results() {
    let dirs = RunDirs::new();
    let mut worker = start_with_threshold(
        &dirs,
        "durable_fan_out",
        r#"{"city":"Lisbon","ms":3000}"#,
        "400",
    );
    let mut calls = Vec::new();
    while calls.len() < 4 {
        calls.push(worker.wait_for("remote_call", is_type("remote_call")));
    }
    let (paused, events) = finish_self_suspended(worker);
    assert!(
        index_of(&events, "paused", is_type("paused"))
            > index_of(&events, "the last call", |e| e == &calls[3]),
        "the run suspends only after every call is announced"
    );
    assert_eq!(paused["stats"]["threads"], 5);
    let state = read_state(&paused);
    let mut kinds: Vec<&str> = state["threads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|thread| thread["parked"]["kind"].as_str().unwrap())
        .collect();
    kinds.sort_unstable();
    assert_eq!(
        kinds,
        [
            "remote_call",
            "remote_call",
            "remote_call",
            "remote_call",
            "sleep"
        ]
    );

    // The arguments are class values with an enum.
    let request = |vendor: &str| {
        calls
            .iter()
            .find(|call| call["args"]["request"]["vendor"] == vendor)
            .unwrap_or_else(|| panic!("no call for {vendor}: {calls:#?}"))
    };
    assert_eq!(
        request("charlie")["args"]["request"],
        json!({ "city": "Lisbon", "vendor": "charlie", "cabin": "Business", "nights": 3 })
    );
    let quotes = [
        quote("alfa", "Economy", 1),
        quote("bravo", "Economy", 2),
        quote("charlie", "Business", 3),
        quote("delta", "Business", 4),
    ];
    let result = |vendor: &str, index: usize| {
        json!({ "call_id": request(vendor)["call_id"], "value": quotes[index] }).to_string()
    };
    let shuffled = [
        result("charlie", 2),
        result("alfa", 0),
        result("delta", 3),
        result("alfa", 0),
        result("bravo", 1),
    ];
    let mut extra = vec!["--sleep-suspend-ms", "400"];
    for result in &shuffled {
        extra.extend_from_slice(&["--remote-result", result]);
    }
    sleep_until_ms(paused["wake"]["at_ts"].as_u64().unwrap());
    let worker = Worker::resume(&dirs, 2, paused["snapshot_path"].as_str().unwrap(), &extra);
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(of_type(&events, "remote_result_received").len(), 4);
    assert!(of_type(&events, "remote_call").is_empty(), "no call twice");
    assert!(of_type(&events, "remote_cancel").is_empty(), "{events:#?}");
    let last = events.last().unwrap();
    assert_eq!(last["type"], "completed");
    assert_eq!(
        last["value"],
        json!({
            "city": "Lisbon",
            "quotes": quotes,
            "cheapest": quotes[0],
            "by_vendor": {
                "alfa": quotes[0],
                "bravo": quotes[1],
                "charlie": quotes[2],
                "delta": quotes[3],
            },
            "cabins": ["Economy", "Economy", "Business", "Business"],
        })
    );
}

/// A race in one process: the winner's result arrives on stdin, the worker
/// reports `remote_cancel` for exactly the two losers, and the run then
/// suspends itself in its trailing sleep. A result for a loser that is handed
/// to the next worker with `--remote-result` is ignored, like one on stdin.
#[test]
fn race_losers_are_cancelled_and_their_late_results_are_ignored() {
    let dirs = RunDirs::new();
    let mut worker = start_with_threshold(
        &dirs,
        "durable_quote_race",
        r#"{"city":"Lisbon","tail_ms":3000}"#,
        "400",
    );
    let mut calls = Vec::new();
    while calls.len() < 3 {
        calls.push(worker.wait_for("remote_call", is_type("remote_call")));
    }
    let winner = calls
        .iter()
        .find(|call| call["args"]["request"]["vendor"] == "slow")
        .unwrap()
        .clone();
    let winning_quote = quote("slow", "Business", 3);
    worker.send(&json!({
        "type": "remote_result",
        "call_id": winner["call_id"],
        "value": winning_quote,
    }));
    let mut cancelled = Vec::new();
    while cancelled.len() < 2 {
        cancelled.push(worker.wait_for("remote_cancel", is_type("remote_cancel")));
    }
    // Late, on stdin.
    worker.send(&json!({
        "type": "remote_result",
        "call_id": cancelled[0]["call_id"],
        "value": quote("late", "Economy", 1),
    }));
    let (paused, events) = finish_self_suspended(worker);
    assert_eq!(paused["stats"]["threads"], 1, "the losers have ended");
    assert_eq!(of_type(&events, "remote_cancel").len(), 2);
    assert_eq!(of_type(&events, "remote_result_received").len(), 1);

    let mut cancelled_ids: Vec<&str> = cancelled
        .iter()
        .map(|event| event["call_id"].as_str().unwrap())
        .collect();
    cancelled_ids.sort_unstable();
    let mut loser_ids: Vec<&str> = calls
        .iter()
        .filter(|call| call["call_id"] != winner["call_id"])
        .map(|call| call["call_id"].as_str().unwrap())
        .collect();
    loser_ids.sort_unstable();
    assert_eq!(cancelled_ids, loser_ids);
    // Each `remote_cancel` names the thread that made the call.
    for event in &cancelled {
        let call = calls
            .iter()
            .find(|call| call["call_id"] == event["call_id"])
            .unwrap();
        assert_eq!(event["thread"], call["thread"], "{event}");
    }

    // Late, as an argument of the next worker.
    let late = json!({
        "call_id": cancelled[1]["call_id"],
        "value": quote("late", "Economy", 1),
    })
    .to_string();
    sleep_until_ms(paused["wake"]["at_ts"].as_u64().unwrap());
    let worker = Worker::resume(
        &dirs,
        2,
        paused["snapshot_path"].as_str().unwrap(),
        &["--remote-result", &late],
    );
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert!(of_type(&events, "remote_result_received").is_empty());
    assert!(of_type(&events, "remote_cancel").is_empty());
    assert!(
        stderr.contains("the run does not wait on that call"),
        "{stderr}"
    );
    assert_eq!(events.last().unwrap()["value"], winning_quote);
}

/// `with_timeout` around a remote call that never answers: the deadline
/// cancels the waiting thread, the worker reports `remote_cancel` for its
/// call, and the function returns a value built from the `Timeout` error.
#[test]
fn a_timeout_cancels_the_remote_wait_and_the_run_returns_the_timeout_message() {
    let dirs = RunDirs::new();
    let mut worker = start_with_threshold(
        &dirs,
        "durable_deadline",
        r#"{"city":"Lisbon","limit_ms":400}"#,
        "5000",
    );
    let call = worker.wait_for("remote_call", is_type("remote_call"));
    let cancel = worker.wait_for("remote_cancel", is_type("remote_cancel"));
    assert_eq!(cancel["call_id"], call["call_id"]);
    assert_eq!(cancel["thread"], call["thread"]);
    // Too late: the run does not wait on the call any more.
    worker.send(&json!({
        "type": "remote_result",
        "call_id": call["call_id"],
        "value": quote("sluggish", "Business", 3),
    }));
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert!(of_type(&events, "remote_result_received").is_empty());
    assert!(of_type(&events, "paused").is_empty());
    assert_eq!(
        events.last().unwrap()["value"],
        "no quote for Lisbon: operation timed out after 400ms"
    );
}

/// The deadline of `with_timeout` is a sleep like any other: the run suspends
/// itself with the timer thread, the waiting body and the root. The timer
/// fires in the next process, which cancels the remote wait there.
#[test]
fn a_timeout_longer_than_the_threshold_fires_after_a_self_suspend() {
    let dirs = RunDirs::new();
    let mut worker = start_with_threshold(
        &dirs,
        "durable_deadline",
        r#"{"city":"Lisbon","limit_ms":3000}"#,
        "400",
    );
    let call = worker.wait_for("remote_call", is_type("remote_call"));
    let (paused, events) = finish_self_suspended(worker);
    assert!(of_type(&events, "remote_cancel").is_empty());
    assert_eq!(paused["stats"]["threads"], 3, "root, body, timer");

    sleep_until_ms(paused["wake"]["at_ts"].as_u64().unwrap());
    let worker = Worker::resume(&dirs, 2, paused["snapshot_path"].as_str().unwrap(), &[]);
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    let cancels = of_type(&events, "remote_cancel");
    assert_eq!(cancels.len(), 1, "{events:#?}");
    assert_eq!(cancels[0]["call_id"], call["call_id"]);
    assert_eq!(cancels[0]["thread"], call["thread"]);
    assert_eq!(
        events.last().unwrap()["value"],
        "no quote for Lisbon: operation timed out after 3000ms"
    );
}

/// `Future.cancel` on a thread in a remote wait, and a parent thread that
/// fails while its child is in one: both report `remote_cancel`.
#[test]
fn a_cancelled_future_and_a_failing_parent_abandon_their_remote_calls() {
    for (function, value) in [
        ("durable_cancel_future", "cancelled true"),
        ("durable_failing_parent", "orphan failed with 503"),
    ] {
        let mut worker = Worker::start(function);
        let call = worker.wait_for("remote_call", is_type("remote_call"));
        let cancel = worker.wait_for("remote_cancel", is_type("remote_cancel"));
        assert_eq!(cancel["call_id"], call["call_id"], "{function}");
        assert_eq!(cancel["thread"], call["thread"], "{function}");
        let (code, events, stderr) = worker.finish();
        assert_eq!(code, 0, "{function} stderr: {stderr}\nevents: {events:#?}");
        assert_eq!(of_type(&events, "remote_cancel").len(), 1, "{function}");
        assert_eq!(events.last().unwrap()["value"], value, "{function}");
    }
}

/// A run that fails while a thread still waits for a remote result reports
/// the call as abandoned before its terminal event.
#[test]
fn a_failing_root_abandons_the_remote_calls_of_its_children() {
    let mut worker = Worker::start("durable_failing_root");
    let call = worker.wait_for("remote_call", is_type("remote_call"));
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 1, "stderr: {stderr}\nevents: {events:#?}");
    let cancels = of_type(&events, "remote_cancel");
    assert_eq!(cancels.len(), 1, "{events:#?}");
    assert_eq!(cancels[0]["call_id"], call["call_id"]);
    assert_eq!(cancels[0]["thread"], call["thread"]);
    let last = events.last().unwrap();
    assert_eq!(last["type"], "failed");
    assert!(
        last["error"].as_str().unwrap().contains("QuoteUnavailable"),
        "{last}"
    );
    assert!(
        index_of(&events, "remote_cancel", is_type("remote_cancel")) < events.len() - 1,
        "{events:#?}"
    );
}

/// `cancel` while the run sleeps in its process (the threshold `0` disables
/// the self-suspend): exit 130 without a snapshot.
#[test]
fn cancel_while_sleeping_exits_130_and_a_disabled_threshold_never_suspends() {
    let dirs = RunDirs::new();
    let mut worker = start_with_threshold(&dirs, "durable_nap", r#"{"ms":60000}"#, "0");
    worker.wait_for("the first line", |e| e["text"] == "going to sleep");
    std::thread::sleep(Duration::from_millis(700));
    worker.send(&json!({ "type": "cancel" }));
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 130, "stderr: {stderr}\nevents: {events:#?}");
    assert!(of_type(&events, "paused").is_empty(), "{events:#?}");
    assert_eq!(events.last().unwrap()["type"], "cancelled");
    assert!(snapshot_files(&dirs.snapshots()).is_empty());
}

/// A cancel that arrives while a fan-out sleeps over its remote waits reports
/// every outstanding call as abandoned.
#[test]
fn cancel_while_sleeping_over_remote_waits_abandons_every_call() {
    let dirs = RunDirs::new();
    let mut worker = start_with_threshold(
        &dirs,
        "durable_fan_out",
        r#"{"city":"Lisbon","ms":60000}"#,
        "0",
    );
    let mut calls = Vec::new();
    while calls.len() < 4 {
        calls.push(worker.wait_for("remote_call", is_type("remote_call")));
    }
    worker.send(&json!({ "type": "cancel" }));
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 130, "stderr: {stderr}\nevents: {events:#?}");
    let mut cancelled: Vec<&str> = of_type(&events, "remote_cancel")
        .iter()
        .map(|event| event["call_id"].as_str().unwrap())
        .collect();
    cancelled.sort_unstable();
    let mut announced: Vec<&str> = calls
        .iter()
        .map(|call| call["call_id"].as_str().unwrap())
        .collect();
    announced.sort_unstable();
    assert_eq!(cancelled, announced, "{events:#?}");
}

/// A function that is not durable never suspends itself, whatever the
/// threshold.
#[test]
fn a_run_that_is_not_durable_never_suspends_itself() {
    let dirs = RunDirs::new();
    let worker = start_with_threshold(&dirs, "plain_nap", r#"{"ms":900}"#, "100");
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert!(of_type(&events, "paused").is_empty(), "{events:#?}");
    assert_eq!(events.last().unwrap()["value"], "slept");
    assert!(snapshot_files(&dirs.snapshots()).is_empty());
}

/// A sleep that holds an open file cannot be written to a snapshot. The
/// worker reports `blocked` once, the thread sleeps in the process, and the
/// program continues. The next sleep holds no file and suspends the run.
#[test]
fn a_blocked_self_suspend_is_reported_once_and_the_sleep_runs_in_the_process() {
    let dirs = RunDirs::new();
    let file = dirs.home.path().join("note.txt");
    std::fs::write(&file, "kept").unwrap();
    let args = json!({ "path": file, "first_ms": 1200, "second_ms": 3000 }).to_string();
    let mut worker = start_with_threshold(&dirs, "durable_hold_file", &args, "300");
    let blocked = worker.wait_for("blocked", is_type("blocked"));
    assert!(
        blocked["reason"]
            .as_str()
            .unwrap()
            .contains("host resource"),
        "{blocked}"
    );
    assert!(
        blocked["path"]
            .as_array()
            .unwrap()
            .iter()
            .any(|element| element == "frame read_slowly"),
        "{blocked}"
    );
    let (paused, events) = finish_self_suspended(worker);
    assert_eq!(of_type(&events, "blocked").len(), 1, "{events:#?}");
    assert_eq!(texts(&events), ["read kept"], "the first sleep ran here");
    assert!(
        index_of(&events, "the log line", |e| e["text"] == "read kept")
            > index_of(&events, "blocked", is_type("blocked"))
    );
    assert_eq!(paused["stats"]["blocked_attempts"], 1);

    sleep_until_ms(paused["wake"]["at_ts"].as_u64().unwrap());
    let worker = Worker::resume(&dirs, 2, paused["snapshot_path"].as_str().unwrap(), &[]);
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(events.last().unwrap()["value"], "kept");
}

/// A requested pause reports `wake: null`, also when it lands in a long
/// sleep.
#[test]
fn a_requested_pause_reports_a_null_wake() {
    let dirs = RunDirs::new();
    let mut worker = start_with_threshold(&dirs, "durable_nap", r#"{"ms":60000}"#, "0");
    worker.wait_for("the first line", |e| e["text"] == "going to sleep");
    let (paused, _) = pause_and_finish(worker);
    assert!(paused.as_object().unwrap().contains_key("wake"), "{paused}");
    assert!(paused["wake"].is_null(), "{paused}");
}

// ============================================================================
// Ordered replay, call identities, and restored waits (phase 3 review)
// ============================================================================

/// The call id of a spawned thread names the thread by its place in the spawn
/// tree and the call by its number in that thread, so it does not depend on
/// the order in which the scheduler ran the threads.
#[test]
fn call_ids_name_the_spawn_path_and_the_call_number() {
    let dirs = RunDirs::new();
    let mut worker = start_with_threshold(
        &dirs,
        "durable_fan_out",
        r#"{"city":"Lisbon","ms":3000}"#,
        "400",
    );
    let mut calls = Vec::new();
    while calls.len() < 4 {
        calls.push(worker.wait_for("remote_call", is_type("remote_call")));
    }
    let _ = finish_self_suspended(worker);
    for (index, vendor) in ["alfa", "bravo", "charlie", "delta"].iter().enumerate() {
        let call = calls
            .iter()
            .find(|call| call["args"]["request"]["vendor"] == *vendor)
            .unwrap();
        assert_eq!(call["call_id"], format!("{RUN_ID}-c0.{index}-1"), "{call}");
    }
}

/// A snapshot taken while a fan-out announces its calls is executed twice: a
/// resume, and a recovery that starts again from the same snapshot. The
/// controller answers a call id it has seen from the stored result, by id
/// only, as the site server does. Every quote must still land in the slot of
/// its request.
#[test]
fn a_recovery_during_the_announcements_keeps_every_result_in_its_slot() {
    for _ in 0..3 {
        let dirs = RunDirs::new();
        let mut worker = Worker::spawn_segment(
            &dirs,
            1,
            &[
                "--start",
                "durable_fan_out",
                "--json-args",
                r#"{"city":"Lisbon","ms":1200}"#,
                "--sleep-suspend-ms",
                "0",
                "--auto-snapshot-ms",
                "0",
            ],
        );
        worker.wait_for("the first spawned thread", |event| {
            event["type"] == "thread_started" && event["parent_thread"].is_number()
        });
        let (paused, events) = pause_and_finish(worker);
        let snapshot = paused["snapshot_path"].as_str().unwrap().to_string();
        let mut stored: std::collections::BTreeMap<String, Value> = of_type(&events, "remote_call")
            .into_iter()
            .map(|call| {
                let vendor = call["args"]["request"]["vendor"].as_str().unwrap();
                (
                    call["call_id"].as_str().unwrap().to_string(),
                    quote(vendor, "Economy", 1),
                )
            })
            .collect();

        for segment in [2_u64, 3] {
            let offered: Vec<String> = stored
                .iter()
                .map(|(call_id, value)| json!({ "call_id": call_id, "value": value }).to_string())
                .collect();
            let mut extra = Vec::new();
            for result in &offered {
                extra.push("--remote-result");
                extra.push(result.as_str());
            }
            extra.extend(["--sleep-suspend-ms", "0", "--auto-snapshot-ms", "0"]);
            let mut worker = Worker::resume(&dirs, segment, &snapshot, &extra);
            let mut events = Vec::new();
            loop {
                let Some(event) = worker.next_event() else {
                    break;
                };
                if event["type"] == "remote_call" {
                    let call_id = event["call_id"].as_str().unwrap().to_string();
                    let vendor = event["args"]["request"]["vendor"].as_str().unwrap();
                    // By id only: a stored result wins over the request.
                    let value = stored
                        .entry(call_id.clone())
                        .or_insert_with(|| quote(vendor, "Economy", 1))
                        .clone();
                    worker.send(&json!({
                        "type": "remote_result", "call_id": call_id, "value": value,
                    }));
                }
                let done = event["type"] == "completed" || event["type"] == "failed";
                events.push(event);
                if done {
                    break;
                }
            }
            let (code, rest, stderr) = worker.finish();
            events.extend(rest);
            assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
            let completed = of_type(&events, "completed")[0];
            let vendors: Vec<&str> = completed["value"]["quotes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|quote| quote["vendor"].as_str().unwrap())
                .collect();
            assert_eq!(
                vendors,
                ["alfa", "bravo", "charlie", "delta"],
                "segment {segment}: {stored:#?}"
            );
        }
    }
}

/// A race is paused, all three results arrive while the run has no process,
/// and the run is resumed from that snapshot many times. The result that
/// arrived first wins every time, and the other two calls are abandoned.
#[test]
fn a_race_resumed_with_every_result_picks_the_first_arrival_every_time() {
    let dirs = RunDirs::new();
    let mut worker = Worker::spawn_segment(
        &dirs,
        1,
        &[
            "--start",
            "durable_quote_race",
            "--json-args",
            r#"{"city":"Lisbon","tail_ms":50}"#,
        ],
    );
    let mut calls = Vec::new();
    while calls.len() < 3 {
        calls.push(worker.wait_for("remote_call", is_type("remote_call")));
    }
    let (paused, _) = pause_and_finish(worker);
    let snapshot = paused["snapshot_path"].as_str().unwrap().to_string();
    let call_of = |vendor: &str| {
        calls
            .iter()
            .find(|call| call["args"]["request"]["vendor"] == vendor)
            .unwrap()["call_id"]
            .clone()
    };
    let base = now_ms();
    std::thread::sleep(Duration::from_millis(40));
    // Arrival order: slow, fast, medium. The arguments are passed in another
    // order, so only `ts` can tell the worker which came first.
    let results: Vec<String> = [("medium", 30), ("slow", 10), ("fast", 20)]
        .iter()
        .map(|(vendor, offset)| {
            json!({
                "call_id": call_of(vendor),
                "value": quote(vendor, "Economy", 1),
                "ts": base + offset,
            })
            .to_string()
        })
        .collect();
    for round in 0..12_u64 {
        let mut extra = Vec::new();
        for result in &results {
            extra.push("--remote-result");
            extra.push(result.as_str());
        }
        let worker = Worker::resume(&dirs, 2 + round, &snapshot, &extra);
        let (code, events, stderr) = worker.finish();
        assert_eq!(code, 0, "round {round}: {stderr}\n{events:#?}");
        let completed = events.last().unwrap();
        assert_eq!(completed["type"], "completed");
        assert_eq!(completed["value"]["vendor"], "slow", "round {round}");
        let received = of_type(&events, "remote_result_received");
        assert_eq!(received.len(), 1, "round {round}: {events:#?}");
        assert_eq!(received[0]["call_id"], call_of("slow"));
        let mut cancelled: Vec<&Value> = of_type(&events, "remote_cancel")
            .into_iter()
            .map(|event| &event["call_id"])
            .collect();
        cancelled.sort_by_key(|id| id.as_str().unwrap().to_string());
        let mut losers = [call_of("fast"), call_of("medium")];
        losers.sort_by_key(|id| id.as_str().unwrap().to_string());
        assert_eq!(
            cancelled,
            losers.iter().collect::<Vec<_>>(),
            "round {round}"
        );
    }
}

/// A run whose background thread sleeps for a long time suspends itself. A
/// resume long before the deadline, with the result the root waits for, must
/// deliver that result and complete. It must not suspend again with the
/// result unconsumed.
#[test]
fn an_early_resume_delivers_a_stored_result_before_it_would_suspend_again() {
    let dirs = RunDirs::new();
    let mut worker = start_with_threshold(
        &dirs,
        "durable_background",
        r#"{"city":"Lisbon","ms":40000}"#,
        "3000",
    );
    let call = worker.wait_for("remote_call", is_type("remote_call"));
    let (paused, _) = finish_self_suspended(worker);
    assert_eq!(paused["stats"]["threads"], 3);
    assert!(paused["wake"]["remaining_ms"].as_u64().unwrap() > 30_000);

    let result = json!({
        "call_id": call["call_id"],
        "value": quote("solo", "Business", 3),
        "ts": now_ms(),
    })
    .to_string();
    let worker = Worker::resume(
        &dirs,
        2,
        paused["snapshot_path"].as_str().unwrap(),
        &["--remote-result", &result, "--sleep-suspend-ms", "3000"],
    );
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(of_type(&events, "remote_result_received").len(), 1);
    assert!(of_type(&events, "paused").is_empty(), "{events:#?}");
    let completed = events.last().unwrap();
    assert_eq!(completed["type"], "completed");
    assert_eq!(completed["value"], quote("solo", "Business", 3));
}

/// A resumed worker tells its supervisor which remote calls its restored
/// threads wait on (`remote_wait`) and whether it already holds the result.
#[test]
fn a_resumed_worker_reports_its_restored_remote_waits() {
    let dirs = RunDirs::new();
    let mut worker = start_with_threshold(
        &dirs,
        "durable_fan_out",
        r#"{"city":"Lisbon","ms":2500}"#,
        "400",
    );
    let mut calls = Vec::new();
    while calls.len() < 4 {
        calls.push(worker.wait_for("remote_call", is_type("remote_call")));
    }
    let (paused, _) = finish_self_suspended(worker);
    let alfa = calls
        .iter()
        .find(|call| call["args"]["request"]["vendor"] == "alfa")
        .unwrap();
    let result =
        json!({ "call_id": alfa["call_id"], "value": quote("alfa", "Economy", 1) }).to_string();
    let mut worker = Worker::resume(
        &dirs,
        2,
        paused["snapshot_path"].as_str().unwrap(),
        &["--remote-result", &result, "--sleep-suspend-ms", "0"],
    );
    let mut waits = Vec::new();
    while waits.len() < 4 {
        waits.push(worker.wait_for("remote_wait", is_type("remote_wait")));
    }
    for call in &calls {
        if call["call_id"] != alfa["call_id"] {
            let vendor = call["args"]["request"]["vendor"].as_str().unwrap();
            worker.send(&json!({
                "type": "remote_result",
                "call_id": call["call_id"],
                "value": quote(vendor, "Economy", 1),
            }));
        }
    }
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    let mut waited: Vec<(&str, bool)> = waits
        .iter()
        .map(|wait| {
            assert_eq!(wait["function"], "remote_get_quote", "{wait}");
            assert!(wait["thread"].is_number(), "{wait}");
            (
                wait["call_id"].as_str().unwrap(),
                wait["has_result"].as_bool().unwrap(),
            )
        })
        .collect();
    waited.sort_unstable();
    let mut expected: Vec<(&str, bool)> = calls
        .iter()
        .map(|call| {
            (
                call["call_id"].as_str().unwrap(),
                call["call_id"] == alfa["call_id"],
            )
        })
        .collect();
    expected.sort_unstable();
    assert_eq!(waited, expected);
}

/// `hello` is the first event also when the arguments are not JSON.
#[test]
fn hello_precedes_the_failure_of_unparseable_arguments() {
    let worker = Worker::spawn(&["--start", "durable_nap", "--json-args", "{nope"]);
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 1, "{stderr}");
    assert_eq!(events[0]["type"], "hello", "{events:#?}");
    assert_eq!(events[0]["mode"], "start");
    assert_eq!(events[0]["function"], "durable_nap");
    assert_eq!(events.last().unwrap()["type"], "failed");
    assert!(
        events.last().unwrap()["error"]
            .as_str()
            .unwrap()
            .contains("--json-args is not valid JSON"),
        "{events:#?}"
    );
}

/// A sleep of exactly `--sleep-suspend-ms` suspends the run. The rule is
/// evaluated a moment after the sleep began, when a little less than the
/// threshold is left, so without a slack such a run suspended only sometimes.
#[test]
fn a_sleep_of_exactly_the_threshold_suspends_the_run() {
    let workers: Vec<(std::rc::Rc<RunDirs>, Worker)> = (0..6)
        .map(|_| {
            let dirs = RunDirs::new();
            let worker = start_with_threshold(&dirs, "durable_nap", r#"{"ms":1500}"#, "1500");
            (dirs, worker)
        })
        .collect();
    for (_dirs, worker) in workers {
        let (paused, _) = finish_self_suspended(worker);
        let remaining = paused["wake"]["remaining_ms"].as_u64().unwrap();
        assert!((1300..=1500).contains(&remaining), "{paused}");
    }
}

// ============================================================================
// Contract section 10.1: every arrow explains its own cause
// ============================================================================

/// The fixture source, so that a test names a line by what is written on it.
const FIXTURE_SOURCE: &str = include_str!("fixtures/worker_trip/baml_src/trip.baml");

/// The 1-based number of the only line of the fixture that reads exactly
/// `text`, indentation included. Two functions of the fixture contain the
/// same statement at different depths, so the whole line is the key.
fn fixture_line(text: &str) -> u64 {
    let mut found = None;
    for (index, line) in FIXTURE_SOURCE.lines().enumerate() {
        if line.trim_end() == text {
            assert!(found.is_none(), "`{text}` is on more than one line");
            found = Some(index as u64 + 1);
        }
    }
    found.unwrap_or_else(|| panic!("no line of the fixture reads `{text}`"))
}

/// Like [`fixture_line`], but only inside the body of `function`: two
/// functions of the fixture have the same `await` line.
fn fixture_line_in(function: &str, text: &str) -> u64 {
    let start = FIXTURE_SOURCE
        .lines()
        .position(|line| line.starts_with(&format!("function {function}(")))
        .unwrap_or_else(|| panic!("the fixture has no function `{function}`"));
    let offset = FIXTURE_SOURCE
        .lines()
        .skip(start)
        .position(|line| line.trim_end() == text)
        .unwrap_or_else(|| panic!("`{function}` has no line that reads `{text}`"));
    (start + offset) as u64 + 1
}

/// `(file, line)` of an event, for a comparison that prints both.
fn location(event: &Value) -> (Value, Value) {
    (event["file"].clone(), event["line"].clone())
}

fn at(line: u64) -> (Value, Value) {
    (json!(SOURCE_FILE), json!(line))
}

/// The three spawned calls of `durable_quote_race`, with the line each one
/// is written on.
fn race_call_sites() -> Vec<(&'static str, (Value, Value))> {
    [("fast", 1), ("medium", 2), ("slow", 3)]
        .into_iter()
        .map(|(vendor, nights)| {
            let line = fixture_line(&format!(
                r#"    let {vendor} = spawn {{ remote_get_quote(quote_request(city, "{vendor}", {nights})) }};"#
            ));
            (vendor, at(line))
        })
        .collect()
}

/// A race: every `remote_call` names the `spawn` line its call was written
/// on, every spawned thread names the same line as its `spawn` site, and each
/// loser's `remote_cancel` repeats the location of its own call with the
/// cause `future_cancel`.
#[test]
fn a_race_reports_the_call_site_the_spawn_site_and_the_cause_of_each_loser() {
    let dirs = RunDirs::new();
    let mut worker = start_with_threshold(
        &dirs,
        "durable_quote_race",
        r#"{"city":"Lisbon","tail_ms":100}"#,
        "0",
    );
    let mut calls = Vec::new();
    while calls.len() < 3 {
        calls.push(worker.wait_for("remote_call", is_type("remote_call")));
    }
    for (vendor, expected) in race_call_sites() {
        let call = calls
            .iter()
            .find(|call| call["args"]["request"]["vendor"] == vendor)
            .unwrap_or_else(|| panic!("no call for {vendor}: {calls:#?}"));
        assert_eq!(location(call), expected, "{vendor}: {call}");
    }

    let winner = calls
        .iter()
        .find(|call| call["args"]["request"]["vendor"] == "slow")
        .unwrap()
        .clone();
    worker.send(&json!({
        "type": "remote_result",
        "call_id": winner["call_id"],
        "value": quote("slow", "Business", 3),
    }));
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");

    // The root has no spawn site. Each thread that made a call names the
    // `spawn` it came from, and the thread `baml.future.race` spawns inside
    // the standard library names the innermost user frame, the `await`.
    let started = of_type(&events, "thread_started");
    assert_eq!(started[0]["parent_thread"], Value::Null);
    assert_eq!(location(started[0]), (Value::Null, Value::Null));
    let root = started[0]["thread"].clone();
    for (vendor, expected) in race_call_sites() {
        let call = calls
            .iter()
            .find(|call| call["args"]["request"]["vendor"] == vendor)
            .unwrap();
        let event = started
            .iter()
            .find(|event| event["thread"] == call["thread"])
            .unwrap_or_else(|| panic!("no thread_started for {vendor}: {started:#?}"));
        assert_eq!(event["parent_thread"], root, "{event}");
        assert_eq!(location(event), expected, "{vendor}: {event}");
    }
    let call_threads: Vec<&Value> = calls.iter().map(|call| &call["thread"]).collect();
    let race_thread = started
        .iter()
        .find(|event| event["parent_thread"] == root && !call_threads.contains(&&event["thread"]))
        .expect("the thread `baml.future.race` spawns");
    assert_eq!(
        location(race_thread),
        at(fixture_line_in(
            "durable_quote_race",
            "    let winner = await baml.future.race([fast, medium, slow]);"
        )),
        "{race_thread}"
    );

    let cancels = of_type(&events, "remote_cancel");
    assert_eq!(cancels.len(), 2, "{events:#?}");
    for cancel in &cancels {
        let call = calls
            .iter()
            .find(|call| call["call_id"] == cancel["call_id"])
            .unwrap_or_else(|| panic!("unknown call id: {cancel}"));
        assert_eq!(location(cancel), location(call), "{cancel}");
        assert_eq!(cancel["cause"], "future_cancel", "{cancel}");
    }
}

/// `with_timeout` cancels its body through a user cancel token, in the
/// process that started the run and in a process that resumed it: the call
/// site travels in the snapshot, so both name the line of the call.
#[test]
fn a_deadline_reports_a_token_cause_with_the_call_site_before_and_after_a_resume() {
    let call_site = at(fixture_line(
        r#"        remote_get_quote(quote_request(city, "sluggish", 3)).vendor"#,
    ));

    let dirs = RunDirs::new();
    let mut worker = start_with_threshold(
        &dirs,
        "durable_deadline",
        r#"{"city":"Lisbon","limit_ms":400}"#,
        "5000",
    );
    let call = worker.wait_for("remote_call", is_type("remote_call"));
    assert_eq!(location(&call), call_site, "{call}");
    let cancel = worker.wait_for("remote_cancel", is_type("remote_cancel"));
    assert_eq!(location(&cancel), call_site, "{cancel}");
    assert_eq!(cancel["cause"], "token", "{cancel}");
    let (code, _, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}");

    // The same deadline, but long enough that the run suspends itself first.
    // The process that reports the cancellation never made the call.
    let dirs = RunDirs::new();
    let mut worker = start_with_threshold(
        &dirs,
        "durable_deadline",
        r#"{"city":"Lisbon","limit_ms":3000}"#,
        "400",
    );
    worker.wait_for("remote_call", is_type("remote_call"));
    let (paused, events) = finish_self_suspended(worker);
    assert!(of_type(&events, "remote_cancel").is_empty());
    sleep_until_ms(paused["wake"]["at_ts"].as_u64().unwrap());
    let worker = Worker::resume(&dirs, 2, paused["snapshot_path"].as_str().unwrap(), &[]);
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert!(
        of_type(&events, "remote_call").is_empty(),
        "a resumed run does not announce the call again: {events:#?}"
    );
    let cancels = of_type(&events, "remote_cancel");
    assert_eq!(cancels.len(), 1, "{events:#?}");
    assert_eq!(location(cancels[0]), call_site, "{}", cancels[0]);
    assert_eq!(cancels[0]["cause"], "token", "{}", cancels[0]);
}

/// A thread whose parent fails is cancelled by the parent's token, and its
/// abandoned call says so.
#[test]
fn the_child_of_a_failing_parent_reports_the_parent_as_the_cause() {
    let mut worker = Worker::start("durable_failing_parent");
    let call = worker.wait_for("remote_call", is_type("remote_call"));
    assert_eq!(
        location(&call),
        at(fixture_line(
            r#"        let child = spawn { remote_get_quote(quote_request(city, "orphan", 2)).vendor };"#
        )),
        "{call}"
    );
    let cancel = worker.wait_for("remote_cancel", is_type("remote_cancel"));
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(location(&cancel), location(&call), "{cancel}");
    assert_eq!(cancel["cause"], "parent", "{cancel}");
}

/// `Future.cancel` on a thread that waits for a remote result.
#[test]
fn a_cancelled_future_reports_the_future_as_the_cause() {
    let mut worker = Worker::start("durable_cancel_future");
    let call = worker.wait_for("remote_call", is_type("remote_call"));
    let cancel = worker.wait_for("remote_cancel", is_type("remote_cancel"));
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");
    assert_eq!(location(&cancel), location(&call), "{cancel}");
    assert_eq!(
        location(&cancel),
        at(fixture_line(
            r#"    let pending = spawn { remote_get_quote(quote_request(city, "unwanted", 2)).vendor };"#
        )),
        "{cancel}"
    );
    assert_eq!(cancel["cause"], "future_cancel", "{cancel}");
}

/// A call the run never abandoned itself: the worker reports it when the run
/// ends, with the call site it recorded.
#[test]
fn a_call_abandoned_at_the_end_of_a_run_keeps_its_call_site() {
    let mut worker = Worker::start("durable_failing_root");
    let call = worker.wait_for("remote_call", is_type("remote_call"));
    assert_eq!(
        location(&call),
        at(fixture_line(
            r#"    let child = spawn { remote_get_quote(quote_request(city, "orphan", 2)).vendor };"#
        )),
        "{call}"
    );
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 1, "stderr: {stderr}\nevents: {events:#?}");
    let cancels = of_type(&events, "remote_cancel");
    assert_eq!(cancels.len(), 1, "{events:#?}");
    assert_eq!(location(cancels[0]), location(&call), "{}", cancels[0]);
    // Either the engine classified the cascade or the worker reported the
    // call when the run ended; both are honest answers here.
    // The report comes from the worker's sweep when the run ends before the
    // cancelled thread reaches its wait (`unknown`), and from the engine's
    // classification of the cascade when it does not (`parent`).
    assert!(
        ["parent", "unknown"].contains(&cancels[0]["cause"].as_str().unwrap()),
        "{}",
        cancels[0]
    );
}

/// A remote call that follows a loop reports the line the call is written on,
/// the same frame its `position` event describes, and not the loop body the
/// thread was in before.
#[test]
fn a_remote_call_after_a_loop_reports_the_frame_of_its_position_event() {
    let mut worker = Worker::start("durable_plan_trip");
    let call = worker.wait_for("remote_call", is_type("remote_call"));
    worker.send(&json!({
        "type": "remote_result",
        "call_id": call["call_id"],
        "value": "sunny in Lisbon",
    }));
    let (code, events, stderr) = worker.finish();
    assert_eq!(code, 0, "stderr: {stderr}\nevents: {events:#?}");

    let position = of_type(&events, "position")
        .into_iter()
        .find(|event| event["reason"] == "remote_call")
        .expect("a position for the remote call");
    assert_eq!(location(&call), location(position), "{call}");
    assert_eq!(call["thread"], position["thread"]);
    // Line 22 of the fixture, as `durable_run_answers_remote_call_and_completes`
    // expects of the `position` event.
    assert_eq!(location(&call), at(22), "{call}");
    let started = of_type(&events, "thread_started");
    assert_eq!(location(started[0]), (Value::Null, Value::Null));
}
