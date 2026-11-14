//! A future's span carries the error the future ended with: the recording's
//! `ThreadCompletion.error_cas_id`, served as `spans.error_value`. A future
//! that returned or was cancelled carries none.
mod support;

use std::path::Path;

use bex_engine::BexExternalValue;
use btel_recorder::proto::{self, span_event::Event};
use prost::Message as _;
use serde_json::{Value, json};
use support::*;

const SOURCE: &str = r#"
class Failure {
    code: int,
    reason: string,
}

function Thrower(n: int) -> int throws Failure {
    throw Failure { code: n, reason: "refused" }
}

function fine(n: int) -> int {
    n + 1
}

// A root future that fails with a thrown value.
function crash(n: int) -> int {
    Thrower(n)
}

// A root future that fails with a panic.
function panics(n: int) -> int {
    baml.sys.panic("out of options")
}

// A spawned future fails; the root future handles its error and returns.
function caught_child(n: int) -> int {
    let child = spawn { Thrower(n) };
    (await child) catch (e) {
        Failure => -1,
    }
}

class Flag {
    set: bool,
}

function until_cancelled(started: Flag) -> int {
    started.set = true;
    baml.sys.sleep(baml.time.Duration.from_milliseconds(60000));
    1
}

// A spawned future is cancelled while it runs.
function cancels(n: int) -> int {
    let stop = baml.spawn.CancelToken.new();
    let started = Flag { set: false };
    let running = spawn with stop { until_cancelled(started) };
    while (!started.set) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(1));
    }
    let _ = stop.cancel();
    (await baml.future.all_settled([running])).length()
}
"#;

async fn record(project: &Path) {
    let results = record_program(
        project,
        SOURCE,
        &[],
        &[
            ("fine", 1),
            ("crash", 7),
            ("panics", 0),
            ("caught_child", 9),
            ("cancels", 0),
        ],
    )
    .await;
    assert_eq!(results[0], Ok(BexExternalValue::Int(2)));
    assert!(results[1].is_err() && results[2].is_err(), "{results:?}");
    assert_eq!(results[3], Ok(BexExternalValue::Int(-1)));
    assert_eq!(results[4], Ok(BexExternalValue::Int(1)));
}

/// Every thread completion of the project's recordings, with the format
/// minor of the file that carries it.
fn completions(project: &Path) -> Vec<(u32, proto::ThreadCompletion)> {
    let mut found = Vec::new();
    for recording in std::fs::read_dir(project.join(".baml/btel/recordings")).unwrap() {
        let mut files: Vec<_> = std::fs::read_dir(recording.unwrap().path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "btel"))
            .collect();
        files.sort();
        for path in files {
            let file = proto::RecordingFile::decode(std::fs::read(path).unwrap().as_slice())
                .expect("a recording file");
            let minor = file.header.unwrap().format_minor;
            for section in file.spans.unwrap_or_default().sections {
                for event in section.events {
                    if let Some(Event::ThreadCompletion(done)) = event.event {
                        found.push((minor, done));
                    }
                }
            }
        }
    }
    found
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_future_carries_its_error_in_the_recording() {
    let project = tempfile::tempdir().unwrap();
    record(project.path()).await;
    let completions = completions(project.path());
    let errored = proto::InvocationOutcome::Errored as i32;
    let failed: Vec<_> = completions
        .iter()
        .filter(|(_, done)| done.outcome == errored)
        .collect();
    // `crash`, `panics` and the future `caught_child` spawned.
    assert_eq!(failed.len(), 3, "{completions:?}");
    for (minor, done) in &failed {
        assert!(done.error_cas_id.is_some(), "{done:?}");
        assert!(
            *minor >= btel_settings::encoding::THREAD_ERROR_FORMAT_MINOR,
            "a file with a thread error is minor {minor}"
        );
    }
    assert_eq!(failed.iter().filter(|(_, done)| done.panicked).count(), 1);
    // The futures that returned and the cancelled one name no error.
    let others: Vec<_> = completions
        .iter()
        .filter(|(_, done)| done.outcome != errored)
        .collect();
    assert!(
        others
            .iter()
            .any(|(_, done)| done.outcome == proto::InvocationOutcome::Cancelled as i32),
        "{completions:?}"
    );
    assert!(
        others
            .iter()
            .filter(|(_, done)| done.outcome == proto::InvocationOutcome::Ok as i32)
            .count()
            >= 3,
        "{completions:?}"
    );
    for (_, done) in others {
        assert_eq!(done.error_cas_id, None, "{done:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_future_span_serves_the_error_it_ended_with() {
    let project = tempfile::tempdir().unwrap();
    record(project.path()).await;
    let mut index = index(project.path());
    let rows = sql(
        &mut index,
        "SELECT span_name, parent_span_id IS NULL AS root, status, error_value IS NULL AS no_error,
           error_value['error'] AS error,
           error_value['stack_trace']['frames'][0]['function_name'] AS outermost
         FROM spans WHERE span_type = 'future' AND span_name != 'baml.gc'
         ORDER BY start_time",
    );
    let by_name = |name: &str| -> &Vec<Value> {
        let mut matching = rows
            .rows
            .iter()
            .filter(|row| row[0].as_str().is_some_and(|n| n.contains(name)));
        let row = matching
            .next()
            .unwrap_or_else(|| panic!("{name}: {:?}", rows.rows));
        assert!(matching.next().is_none(), "{name}: {:?}", rows.rows);
        row
    };

    // A root future that threw: the thrown value, and the stack it crossed,
    // outermost frame first.
    let crash = by_name("user.crash");
    assert_eq!(
        crash[1..4],
        [json!(1), json!("user_error"), json!(0)],
        "{crash:?}"
    );
    assert_eq!(crash[4]["$class"], json!("user.Failure"));
    assert_eq!(crash[4]["code"], json!(7));
    assert_eq!(crash[4]["reason"], json!("refused"));
    assert_eq!(crash[5], json!("user.crash"));

    // A root future that panicked.
    let panics = by_name("user.panics");
    assert_eq!(
        panics[1..4],
        [json!(1), json!("panic_error"), json!(0)],
        "{panics:?}"
    );
    assert_eq!(panics[4]["$class"], json!("baml.panics.UserPanic"));
    assert_eq!(panics[4]["message"], json!("out of options"));

    // A spawned future that threw, under a root future that returned.
    let child = by_name("lambda(caught_child");
    assert_eq!(
        child[1..4],
        [json!(0), json!("user_error"), json!(0)],
        "{child:?}"
    );
    assert_eq!(child[4]["code"], json!(9));
    assert_eq!(child[5], child[0]);

    // Futures that returned or were cancelled have no error.
    for (name, status) in [
        ("user.fine", "return"),
        ("user.caught_child", "return"),
        ("user.cancels", "return"),
        ("lambda(cancels", "cancel_error"),
    ] {
        let row = by_name(name);
        assert_eq!(row[2..5], [json!(status), json!(1), Value::Null], "{row:?}");
    }

    // The value is addressable in a predicate, like a function span's.
    let found = sql(
        &mut index,
        "SELECT span_name FROM spans
         WHERE span_type = 'future' AND error_value['error']['code'] = 7",
    );
    assert_eq!(found.rows, vec![vec![json!("user.crash")]]);
    // The whole value is a `baml.errors.Context`.
    let context = sql(
        &mut index,
        "SELECT error_value FROM spans WHERE span_type = 'future' AND span_name = 'user.crash'",
    );
    assert_eq!(context.rows[0][0]["$class"], json!("baml.errors.Context"));
}
