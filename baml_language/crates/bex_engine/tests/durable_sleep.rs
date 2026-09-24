//! Durable functions, phase 3: durable sleep (contract section 9.2).
//!
//! A run suspends itself when every live thread waits in an operation that
//! can be re-issued, at least one thread sleeps, and the earliest sleep
//! deadline is at least the threshold away. The tests play the worker: they
//! wait for `BexEngine::durable_idle_sleep`, take a snapshot in
//! `SnapshotMode::SuspendIfIdle`, and resume the snapshot IN A FRESH ENGINE.

#![cfg(not(target_arch = "wasm32"))]

mod common;
#[allow(dead_code)]
mod durable_harness;

use std::{
    io::Write as _,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use bex_engine::{
    BexExternalValue, EngineError,
    durable::{ResumeOptions, SnapshotFailure, SnapshotMode},
};
use durable_harness::{
    Compiled, Remote, Segment, Site, compiled, ctx, new_engine, run_plain, snapshot, string,
};

const SOURCE: &str = r#"
enum Tier {
  Basic
  Premium
}

class Coord {
  lat float
  lon float
}

class Quote {
  vendor string
  price int
  tier Tier
  place Coord
  note string?
  tags map<string, int>
  alt int | string
}

class Node {
  name string
  next Node?
}

class Trip {
  city string
  quotes Quote[]
  best Quote?
  by_vendor map<string, Quote>
  tiers Tier[]
  ring string
}

function build_quote(vendor: string, price: int) -> Quote {
  let tier: Tier = Tier.Basic;
  if (price > 100) {
    tier = Tier.Premium;
  }
  let note: string? = null;
  if (price > 50) {
    note = "pricey " + vendor;
  }
  let alt: int | string = vendor;
  if (price % 2 == 0) {
    alt = price;
  }
  Quote {
    vendor: vendor,
    price: price,
    tier: tier,
    place: Coord { lat: 38.5, lon: -9.25 },
    note: note,
    tags: { "len": vendor.length(), "price": price },
    alt: alt,
  }
}

function new_ring() -> Node {
  let first = Node { name: "a", next: null };
  let second = Node { name: "b", next: first };
  first.next = second;
  first
}

function ring_label(n: Node) -> string {
  let label = n.name;
  let m = n.next;
  if (m != null) {
    label = label + ">" + m.name;
    let k = m.next;
    if (k != null) {
      label = label + ">" + k.name;
    }
  }
  label
}

function trip_of(city: string, quotes: Quote[], ring: Node) -> Trip {
  let best: Quote? = null;
  let by_vendor: map<string, Quote> = {};
  let tiers: Tier[] = [];
  for (let q in quotes) {
    by_vendor.set(q.vendor, q);
    tiers.push(q.tier);
    if (best == null) {
      best = q;
    } else if (q.price < best.price) {
      best = q;
    }
  }
  Trip {
    city: city,
    quotes: quotes,
    best: best,
    by_vendor: by_vendor,
    tiers: tiers,
    ring: ring_label(ring),
  }
}

function nap(ms: int) -> int {
  {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(ms));
    0
  } catch (e) {
    baml.errors.Io => 1
  }
}

function remote_quote(vendor: string, price: int) -> Quote {
  nap(5);
  build_quote(vendor, price)
}

// A class instance, a map, an enum and a cyclic graph are held across the sleep.
function nap_then(city: string, ms: int) -> Trip {
  let ring = new_ring();
  let quotes = [build_quote(city + "-1", 120), build_quote(city + "-2", 7)];
  nap(ms);
  trip_of(city, quotes, ring)
}

function fan_out_sleep(city: string, ms: int) -> Trip {
  let ring = new_ring();
  let fs = [
    spawn { remote_quote(city + "-1", 10) },
    spawn { remote_quote(city + "-2", 120) },
    spawn { remote_quote(city + "-3", 55) },
    spawn { remote_quote(city + "-4", 7) },
  ];
  nap(ms);
  let quotes = await baml.future.all(fs);
  trip_of(city, quotes, ring)
}

// The root waits in `await`; the sleep is the child's.
function await_sleeper(city: string, ms: int) -> Trip {
  let ring = new_ring();
  let child = spawn {
    let quote = build_quote(city + "-slept", 64);
    nap(ms);
    quote
  };
  let quote = await child;
  trip_of(city, [quote], ring)
}

function spin(n: int) -> int {
  let i = 0;
  let acc = 0;
  while (i < n) {
    acc = acc + i % 7;
    i += 1;
  }
  acc
}

// The child computes for a while and only then reaches a wait.
function busy_sibling(city: string, n: int, ms: int) -> Trip {
  let ring = new_ring();
  let worker = spawn {
    let price = spin(n) % 90 + 1;
    remote_quote(city + "-busy", price)
  };
  nap(ms);
  let quote = await worker;
  trip_of(city, [quote], ring)
}

// The child's sleep is shorter than the threshold, the root's is longer.
function two_sleeps(city: string, short_ms: int, long_ms: int) -> Trip {
  let ring = new_ring();
  let child = spawn {
    nap(short_ms);
    build_quote(city + "-short", 33)
  };
  nap(long_ms);
  let quote = await child;
  trip_of(city, [quote], ring)
}

function read_slowly(path: string, ms: int) -> string {
  let file = baml.fs.open(path, "r");
  nap(ms);
  file.text()
}

// The first sleep holds an open file, the second one does not.
function hold_file(path: string, first_ms: int, second_ms: int) -> string {
  let text = read_slowly(path, first_ms);
  nap(second_ms);
  text
}

function queued_remote(city: string, ms: int) -> Trip {
  let ring = new_ring();
  let g = baml.spawn.TaskGroup.new(1, name = "one at a time");
  let fs: baml.future.Future<Quote, never>[] = [];
  let i = 0;
  while (i < 3) {
    let n = i;
    fs.push(spawn with baml.spawn.options(group = g) {
      remote_quote(city + "-" + n.to_string(), n * 60 + 1)
    });
    i += 1;
  }
  nap(ms);
  let quotes = await baml.future.all(fs);
  trip_of(city, quotes, ring)
}

// The future is cancelled while the host still announces the remote call.
function cancel_during_dispatch(city: string) -> string {
  let pending = spawn { remote_quote(city + "-unwanted", 9).vendor };
  nap(40);
  let did_cancel = pending.cancel();
  let outcome = (await pending) catch (e) {
    baml.panics.Cancelled => "cancelled"
  };
  outcome + " " + did_cancel.to_string()
}

function quick(city: string) -> string {
  city + "!"
}
"#;

fn program() -> &'static Compiled {
    static PROGRAM: OnceLock<Compiled> = OnceLock::new();
    compiled(&PROGRAM, SOURCE)
}

fn int(value: i64) -> BexExternalValue {
    BexExternalValue::Int(value)
}

fn unix_ms_now() -> u64 {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

async fn sleep_past(deadline_unix_ms: u64) {
    let remaining = deadline_unix_ms.saturating_sub(unix_ms_now());
    tokio::time::sleep(Duration::from_millis(remaining + 30)).await;
}

fn parked_kinds(state: &serde_json::Value) -> Vec<String> {
    let mut kinds: Vec<String> = state["threads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|thread| thread["parked"]["kind"].as_str().unwrap().to_string())
        .collect();
    kinds.sort();
    kinds
}

fn quotes_of(trip: &BexExternalValue) -> Vec<BexExternalValue> {
    let BexExternalValue::Instance { fields, .. } = trip else {
        panic!("a Trip: {trip:?}");
    };
    let BexExternalValue::Array { items, .. } = &fields["quotes"] else {
        panic!("quotes");
    };
    items.clone()
}

/// A single sleeping thread suspends itself. A resume before the deadline
/// suspends again with the same deadline, and a resume after the deadline
/// completes at once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_long_sleep_suspends_the_run_and_a_passed_deadline_completes_at_once() {
    let (expected, _) = run_plain(program(), "nap_then", vec![string("Porto"), int(5)]).await;
    let expected = expected.result.unwrap();

    let site = Site::new(program(), Remote::Manual);
    let started_ms = unix_ms_now();
    let first = Segment::start(&site, "nap_then", vec![string("Porto"), int(1500)], None);
    let (idle, report) = first.self_suspend(&site, 300).await;
    let report = report.expect("the run suspends itself");
    let wake = report.wake.expect("a self-suspend reports its sleep");
    assert_eq!(report.threads, 1);
    assert_eq!(wake.thread, idle.thread);
    assert_eq!(wake.deadline_unix_ms, idle.deadline_unix_ms);
    assert!(
        wake.deadline_unix_ms >= started_ms + 1500 && wake.deadline_unix_ms <= unix_ms_now() + 1500,
        "{wake:?}"
    );
    assert!(
        wake.remaining_ms <= 1500 && wake.remaining_ms >= 300,
        "{wake:?}"
    );
    assert!(wake.remaining_ms <= idle.remaining_ms, "{wake:?} {idle:?}");
    assert_eq!(parked_kinds(&report.committed.state), ["sleep"]);
    assert_eq!(
        first.finish().await.unwrap_err(),
        EngineError::DurableSuspended
    );

    // A manual resume long before the deadline: the new process sees a sleep
    // that is still over the threshold and suspends again.
    let early = Segment::resume(
        &site,
        "nap_then",
        &report.committed,
        None,
        ResumeOptions::default(),
    );
    let (_, again) = early.self_suspend(&site, 300).await;
    let again = again.expect("the resumed run suspends itself again");
    let wake_again = again.wake.expect("wake");
    assert_eq!(wake_again.deadline_unix_ms, wake.deadline_unix_ms);
    assert!(wake_again.remaining_ms <= wake.remaining_ms);
    assert_eq!(
        early.finish().await.unwrap_err(),
        EngineError::DurableSuspended
    );

    // The deadline passes while the run has no process.
    sleep_past(wake.deadline_unix_ms).await;
    let resumed_at = Instant::now();
    let late = Segment::resume(
        &site,
        "nap_then",
        &again.committed,
        None,
        ResumeOptions::default(),
    );
    assert_eq!(late.finish().await.unwrap(), expected);
    assert!(
        resumed_at.elapsed() < Duration::from_millis(1000),
        "an expired sleep completes at once, took {:?}",
        resumed_at.elapsed()
    );
}

/// The parent sleeps while four children wait for remote results: the run
/// suspends with all five threads. The controller is slow and sloppy: two
/// results arrive while the run has no process (last call first, each one
/// twice), two arrive after the resume.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_parent_sleeping_over_four_remote_waits_suspends_with_every_thread() {
    let (expected, _) = run_plain(program(), "fan_out_sleep", vec![string("Braga"), int(5)]).await;
    let expected = expected.result.unwrap();

    let site = Site::new(program(), Remote::Manual);
    let segment = Segment::start(
        &site,
        "fan_out_sleep",
        vec![string("Braga"), int(1200)],
        None,
    );
    let idle = tokio::time::timeout(
        Duration::from_secs(30),
        segment.engine.durable_idle_sleep(400),
    )
    .await
    .expect("the run becomes idle");
    // A thread that still dispatches its call is not waiting yet, so the rule
    // cannot hold before all four calls are known outside the process.
    assert_eq!(site.announced.lock().unwrap().len(), 4);
    let report = snapshot(
        &segment.engine,
        program().hash,
        SnapshotMode::SuspendIfIdle {
            min_remaining_ms: 400,
        },
        &site,
    )
    .await
    .expect("the run suspends itself");
    let wake = report.wake.expect("wake");
    assert_eq!(report.threads, 5);
    assert_eq!(wake.thread, idle.thread);
    assert_eq!(
        parked_kinds(&report.committed.state),
        [
            "remote_call",
            "remote_call",
            "remote_call",
            "remote_call",
            "sleep"
        ]
    );
    let root = report.committed.state["threads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|thread| thread["parked"]["kind"] == "sleep")
        .unwrap();
    assert_eq!(root["thread"].as_u64(), Some(wake.thread));
    assert!(root["parent_thread"].is_null(), "{root}");
    assert_eq!(
        segment.finish().await.unwrap_err(),
        EngineError::DurableSuspended
    );
    #[allow(clippy::print_stderr)]
    {
        eprintln!(
            "MEASURE self_suspend threads={} objects={} raw_bytes={} compressed_bytes={} \
             file_bytes={} park_ms={:.3} walk_ms={:.3} encode_ms={:.3} compress_ms={:.3}",
            report.threads,
            report.stats.objects,
            report.stats.raw_bytes,
            report.stats.compressed_bytes,
            report.committed.bytes.len(),
            report.pause_latency_ms,
            report.stats.walk_ms,
            report.stats.encode_ms,
            report.stats.compress_ms,
        );
    }

    let quotes = quotes_of(&expected);
    let mut announced = site.announced.lock().unwrap().clone();
    announced.sort_by_key(|(_, _, thread)| *thread);
    let answers: Vec<(String, BexExternalValue)> = announced
        .iter()
        .map(|(call_id, _, _)| call_id.clone())
        .zip(quotes)
        .collect();
    for (call_id, quote) in answers[2..].iter().rev() {
        site.deliver(call_id, Ok(quote.clone()));
        site.deliver(call_id, Ok(quote.clone()));
    }
    sleep_past(wake.deadline_unix_ms).await;
    let resumed = Segment::resume(
        &site,
        "fan_out_sleep",
        &report.committed,
        None,
        ResumeOptions::default(),
    );
    tokio::time::sleep(Duration::from_millis(120)).await;
    for (call_id, quote) in answers[..2].iter().rev() {
        site.deliver(call_id, Ok(quote.clone()));
    }
    assert_eq!(resumed.finish().await.unwrap(), expected);
    assert_eq!(site.announced.lock().unwrap().len(), 4, "no call twice");
    assert!(site.cancelled.lock().unwrap().is_empty());
}

/// A thread inside `await` counts as waiting. The sleep that the run suspends
/// for belongs to the child.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_root_in_await_and_a_sleeping_child_suspend_together() {
    let (expected, _) = run_plain(program(), "await_sleeper", vec![string("Faro"), int(5)]).await;
    let expected = expected.result.unwrap();

    let site = Site::new(program(), Remote::Manual);
    let segment = Segment::start(
        &site,
        "await_sleeper",
        vec![string("Faro"), int(1000)],
        None,
    );
    let (_, report) = segment.self_suspend(&site, 300).await;
    let report = report.expect("the run suspends itself");
    let wake = report.wake.expect("wake");
    assert_eq!(report.threads, 2);
    assert_eq!(parked_kinds(&report.committed.state), ["runnable", "sleep"]);
    let sleeper = report.committed.state["threads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|thread| thread["parked"]["kind"] == "sleep")
        .unwrap();
    assert_eq!(sleeper["thread"].as_u64(), Some(wake.thread));
    assert!(!sleeper["parent_thread"].is_null(), "the child sleeps");
    assert_eq!(
        segment.finish().await.unwrap_err(),
        EngineError::DurableSuspended
    );

    sleep_past(wake.deadline_unix_ms).await;
    let resumed = Segment::resume(
        &site,
        "await_sleeper",
        &report.committed,
        None,
        ResumeOptions::default(),
    );
    assert_eq!(resumed.finish().await.unwrap(), expected);
}

/// One thread that computes keeps the run in its process, however long the
/// other thread sleeps. The rule holds once the busy thread waits too.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_busy_thread_prevents_the_suspend_until_it_waits() {
    let site = Site::new(program(), Remote::Manual);
    let segment = Segment::start(
        &site,
        "busy_sibling",
        vec![string("Evora"), int(150_000), int(4000)],
        None,
    );
    let idle = tokio::time::timeout(
        Duration::from_secs(30),
        segment.engine.durable_idle_sleep(200),
    )
    .await
    .expect("the run becomes idle");
    // The busy thread announces its remote call after its computation. The
    // root sleeps from the first moment on, so a rule that ignored the busy
    // thread would hold long before the call exists.
    assert_eq!(
        site.announced.lock().unwrap().len(),
        1,
        "the rule held while a thread was computing"
    );
    let report = snapshot(
        &segment.engine,
        program().hash,
        SnapshotMode::SuspendIfIdle {
            min_remaining_ms: 200,
        },
        &site,
    )
    .await
    .expect("the run suspends itself");
    assert_eq!(report.wake.unwrap().thread, idle.thread);
    assert_eq!(
        parked_kinds(&report.committed.state),
        ["remote_call", "sleep"]
    );
    assert_eq!(
        segment.finish().await.unwrap_err(),
        EngineError::DurableSuspended
    );
}

/// The earliest sleep decides. While the child's short sleep runs, the run
/// stays in its process; when it is over, the root's long sleep suspends it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_sleep_below_the_threshold_postpones_the_suspend() {
    let (expected, _) = run_plain(
        program(),
        "two_sleeps",
        vec![string("Tomar"), int(5), int(10)],
    )
    .await;
    let expected = expected.result.unwrap();

    let site = Site::new(program(), Remote::Manual);
    let segment = Segment::start(
        &site,
        "two_sleeps",
        vec![string("Tomar"), int(250), int(1500)],
        None,
    );
    let (idle, report) = segment.self_suspend(&site, 600).await;
    let report = report.expect("the run suspends itself");
    // The child has finished its sleep and has ended: only the root is left.
    assert_eq!(report.threads, 1, "{}", report.committed.state);
    let wake = report.wake.unwrap();
    assert_eq!(wake.thread, idle.thread);
    assert!(
        wake.remaining_ms <= 1500 - 250 + 20,
        "the suspend waited for the short sleep: {wake:?}"
    );
    assert_eq!(
        segment.finish().await.unwrap_err(),
        EngineError::DurableSuspended
    );
    sleep_past(wake.deadline_unix_ms).await;
    let resumed = Segment::resume(
        &site,
        "two_sleeps",
        &report.committed,
        None,
        ResumeOptions::default(),
    );
    assert_eq!(resumed.finish().await.unwrap(), expected);
}

/// A sleep shorter than the threshold, and any sleep with the threshold `0`,
/// never satisfy the rule. An attempt that is made anyway ends with `NotIdle`
/// and leaves the run untouched.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_short_sleep_and_a_disabled_rule_never_suspend() {
    let (expected, _) = run_plain(program(), "nap_then", vec![string("Beja"), int(5)]).await;
    let expected = expected.result.unwrap();

    for threshold in [5000, 0] {
        let site = Site::new(program(), Remote::Manual);
        let segment = Segment::start(&site, "nap_then", vec![string("Beja"), int(600)], None);
        let engine = Arc::clone(&segment.engine);
        segment
            .host
            .wait_until("the run sleeps", |host| {
                host.saw(
                    bex_engine::durable::YieldReason::SysOp,
                    Some("baml.sys.sleep"),
                )
            })
            .await;
        let failure = snapshot(
            &engine,
            program().hash,
            SnapshotMode::SuspendIfIdle {
                min_remaining_ms: threshold,
            },
            &site,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(failure, SnapshotFailure::NotIdle),
            "threshold {threshold}: {failure:?}"
        );
        tokio::select! {
            idle = engine.durable_idle_sleep(threshold) => {
                panic!("threshold {threshold}: the rule held for {idle:?}");
            }
            result = segment.finish() => assert_eq!(result.unwrap(), expected),
        }
    }
}

/// A pause that was requested carries no wake time, also when it lands in a
/// long sleep.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_requested_pause_of_a_sleeping_run_reports_no_wake() {
    let site = Site::new(program(), Remote::Manual);
    let segment = Segment::start(&site, "nap_then", vec![string("Beja"), int(3000)], None);
    segment.engine.durable_idle_sleep(500).await;
    let report = segment.pause(&site).await.expect("live");
    assert_eq!(report.wake, None);
    assert_eq!(
        segment.finish().await.unwrap_err(),
        EngineError::DurableSuspended
    );
}

/// Runs that are not the durable run never satisfy the rule: a call on an
/// engine without a host, and a helper call next to a finished run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn only_the_durable_run_suspends_itself() {
    let (expected, _) = run_plain(program(), "nap_then", vec![string("Lagos"), int(5)]).await;
    let expected = expected.result.unwrap();

    let plain = new_engine(program());
    tokio::select! {
        idle = plain.durable_idle_sleep(100) => panic!("no host: the rule held for {idle:?}"),
        result = plain.call_function("nap_then", vec![string("Lagos"), int(500)], ctx(), true) => {
            assert_eq!(result.unwrap(), expected);
        }
    }

    let site = Site::new(program(), Remote::Manual);
    let run = Segment::start(&site, "quick", vec![string("Lagos")], None);
    let engine = Arc::clone(&run.engine);
    assert_eq!(run.finish().await.unwrap(), string("Lagos!"));
    tokio::select! {
        idle = engine.durable_idle_sleep(100) => panic!("helper call: the rule held for {idle:?}"),
        result = engine.call_function("nap_then", vec![string("Lagos"), int(500)], ctx(), true) => {
            assert_eq!(result.unwrap(), expected);
        }
    }
}

/// A sleep that holds a value no snapshot can carry: the attempt is refused
/// once, the thread keeps sleeping in its process, and the rule does not hold
/// again for that sleep. The next sleep, without the file, suspends the run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_blocked_self_suspend_falls_back_to_sleeping_in_process() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(file, "kept across the sleep").unwrap();
    let path = file.path().to_string_lossy().into_owned();

    let site = Site::new(program(), Remote::Manual);
    let segment = Segment::start(
        &site,
        "hold_file",
        vec![string(&path), int(900), int(1200)],
        None,
    );
    let (first_idle, refused) = segment.self_suspend(&site, 300).await;
    let SnapshotFailure::Blocked { reason, path, .. } = refused.unwrap_err() else {
        panic!("expected Blocked");
    };
    assert!(reason.contains("host resource"), "{reason}");
    assert!(
        path.iter().any(|element| element.contains("read_slowly")),
        "{path:?}"
    );

    // The rule holds again only for the second sleep.
    let (second_idle, report) = segment.self_suspend(&site, 300).await;
    assert!(
        second_idle.deadline_unix_ms > first_idle.deadline_unix_ms,
        "{first_idle:?} {second_idle:?}"
    );
    assert!(
        unix_ms_now() >= first_idle.deadline_unix_ms,
        "the first sleep ran to its end in the process"
    );
    let report = report.expect("the second sleep suspends the run");
    assert_eq!(
        segment.finish().await.unwrap_err(),
        EngineError::DurableSuspended
    );
    sleep_past(report.wake.unwrap().deadline_unix_ms).await;
    let resumed = Segment::resume(
        &site,
        "hold_file",
        &report.committed,
        None,
        ResumeOptions::default(),
    );
    assert_eq!(
        resumed.finish().await.unwrap(),
        string("kept across the sleep")
    );
}

/// Threads that wait for a slot of a task group are waiting threads. The run
/// suspends with one remote wait, two queued threads and the sleeping root.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queued_threads_of_a_task_group_do_not_prevent_the_suspend() {
    let (expected, _) = run_plain(program(), "queued_remote", vec![string("Guarda"), int(5)]).await;
    let expected = expected.result.unwrap();

    let site = Site::new(program(), Remote::Execute);
    *site.dispatch_delay.lock().unwrap() = Duration::from_millis(1500);
    let segment = Segment::start(
        &site,
        "queued_remote",
        vec![string("Guarda"), int(1000)],
        None,
    );
    let (_, report) = segment.self_suspend(&site, 300).await;
    let report = report.expect("the run suspends itself");
    assert_eq!(
        parked_kinds(&report.committed.state),
        ["queued", "queued", "remote_call", "sleep"]
    );
    assert_eq!(
        segment.finish().await.unwrap_err(),
        EngineError::DurableSuspended
    );
    *site.dispatch_delay.lock().unwrap() = Duration::ZERO;
    sleep_past(report.wake.unwrap().deadline_unix_ms).await;
    let resumed = Segment::resume(
        &site,
        "queued_remote",
        &report.committed,
        None,
        ResumeOptions::default(),
    );
    assert_eq!(resumed.finish().await.unwrap(), expected);
    assert_eq!(site.announced.lock().unwrap().len(), 3);
}

/// A slow controller: the host needs 300 ms to announce a remote call, and
/// the calling thread is cancelled in the meantime. The engine waits for the
/// announcement, so it knows the call id and reports the call as abandoned.
/// Dropping the announcement half way would leave a remote run that nobody
/// cancels.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_thread_cancelled_while_its_call_is_announced_reports_the_call_as_abandoned() {
    let site = Site::new(program(), Remote::Manual);
    *site.announce_delay.lock().unwrap() = Duration::from_millis(300);
    let segment = Segment::start(&site, "cancel_during_dispatch", vec![string("Sines")], None);
    // `Future.cancel` settles the future at once, so the root does not wait
    // for the cancelled thread, which is still inside the announcement.
    assert_eq!(segment.finish().await.unwrap(), string("cancelled true"));
    site.wait_until("the abandoned call is reported", |site| {
        !site.cancelled.lock().unwrap().is_empty()
    })
    .await;
    let announced = site.announced.lock().unwrap().clone();
    let cancelled = site.cancelled.lock().unwrap().clone();
    assert_eq!(announced.len(), 1);
    assert_eq!(
        cancelled,
        [(announced[0].0.clone(), announced[0].2)],
        "the abandoned call is reported with its id and thread"
    );
    assert!(site.received.lock().unwrap().is_empty());
}
