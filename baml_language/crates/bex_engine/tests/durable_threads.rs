//! Durable functions, phase 3: snapshots of runs with several threads,
//! futures, cancellation, task groups, and native continuation frames.
//!
//! Every test pauses a run, restores it INTO A FRESH ENGINE (the stand-in for
//! a new process), and compares the result and the announced remote calls with
//! an uninterrupted run. See `durable_harness/mod.rs` for the harness.

#![cfg(not(target_arch = "wasm32"))]

mod common;
mod durable_harness;

use std::{
    sync::{Arc, OnceLock, atomic::Ordering},
    time::Duration,
};

use bex_engine::{
    BexExternalValue, EngineError,
    durable::{ParkedKind, ResumeOptions, SnapshotMode, YieldReason},
};
use durable_harness::{
    Compiled, Remote, Segment, Site, compiled, run_plain, run_with_pauses, snapshot, string,
};

const SOURCE: &str = r#"
enum Mood {
  Calm
  Stormy
}

class Coord {
  lat float
  lon float
}

class Box<T> {
  item T
}

class Quote {
  vendor string
  price int
  mood Mood
  place Coord
  note string?
  tags map<string, int>
  big bigint
  alt int | string
  boxed Box<Coord>
}

class QuoteFailed {
  vendor string
  code int
}

class Node {
  name string
  next Node?
}

class Report {
  quotes Quote[]
  total int
  ring string
  moods Mood[]
  by_vendor map<string, Quote>
  cheapest Quote?
}

function build_quote(vendor: string, price: int) -> Quote throws QuoteFailed {
  if (price < 0) {
    throw QuoteFailed { vendor: vendor, code: price };
  }
  let mood: Mood = Mood.Calm;
  if (price > 100) {
    mood = Mood.Stormy;
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
    mood: mood,
    place: Coord { lat: 38.5, lon: -9.25 },
    note: note,
    tags: { "len": vendor.length(), "price": price },
    big: 12345678901234567890n,
    alt: alt,
    boxed: Box { item: Coord { lat: 1.5, lon: 2.5 } },
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

function make_quote(vendor: string, price: int, delay_ms: int) -> Quote throws QuoteFailed {
  nap(delay_ms);
  build_quote(vendor, price)
}

function remote_quote(vendor: string, price: int, delay_ms: int) -> Quote throws QuoteFailed {
  nap(delay_ms);
  build_quote(vendor, price)
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

function report_of(quotes: Quote[], ring: Node) -> Report {
  let total = 0;
  let moods: Mood[] = [];
  let by_vendor: map<string, Quote> = {};
  let cheapest: Quote? = null;
  for (let q in quotes) {
    total += q.price;
    moods.push(q.mood);
    by_vendor.set(q.vendor, q);
    if (cheapest == null) {
      cheapest = q;
    } else if (q.price < cheapest.price) {
      cheapest = q;
    }
  }
  Report {
    quotes: quotes,
    total: total,
    ring: ring_label(ring),
    moods: moods,
    by_vendor: by_vendor,
    cheapest: cheapest,
  }
}

function new_ring() -> Node {
  let first = Node { name: "a", next: null };
  let second = Node { name: "b", next: first };
  first.next = second;
  first
}

function spawn_await(city: string) -> Quote {
  let pending = spawn { make_quote(city, 120, 30) };
  nap(10);
  let quote = await pending;
  nap(5);
  quote
}

function nested(city: string) -> int {
  let outer = spawn {
    let inner = spawn {
      let innermost = spawn { make_quote(city + "-deep", 7, 20) };
      (await innermost).price + 1
    };
    nap(8);
    (await inner) * 10
  };
  nap(4);
  (await outer) + 3
}

function fan_out(city: string) -> Report {
  let ring = new_ring();
  let fs = [
    spawn { make_quote(city + "-1", 10, 30) },
    spawn { make_quote(city + "-2", 120, 50) },
    spawn { make_quote(city + "-3", 55, 20) },
    spawn { make_quote(city + "-4", 7, 40) },
  ];
  nap(25);
  let quotes = await baml.future.all(fs);
  report_of(quotes, ring)
}

function remote_fan_out(city: string) -> Report {
  let ring = new_ring();
  let fs = [
    spawn { remote_quote(city + "-1", 10, 30) },
    spawn { remote_quote(city + "-2", 120, 60) },
    spawn { remote_quote(city + "-3", 55, 10) },
    spawn { remote_quote(city + "-4", 7, 45) },
  ];
  nap(20);
  let quotes = await baml.future.all(fs);
  report_of(quotes, ring)
}

function settled(city: string) -> string[] {
  let fs = [
    spawn { make_quote(city + "-ok", 10, 30) },
    spawn { make_quote(city + "-bad", -4, 15) },
    spawn {
      nap(20);
      if (city.length() > 0) {
        baml.sys.panic("vendor exploded");
      }
      make_quote(city, 1, 1)
    },
  ];
  let outcomes = await baml.future.all_settled(fs);
  let lines: string[] = [];
  for (let outcome in outcomes) {
    match (outcome) {
      let ok: baml.future.Success<Quote> => {
        lines.push("ok " + ok.value.vendor);
      },
      let failure: baml.future.Failure<QuoteFailed> => {
        lines.push("failed " + failure.error.vendor + " " + failure.error.code.to_string());
      },
      let panicked: baml.future.Panicked => {
        lines.push("panicked");
      },
    }
  }
  lines
}

function race_of(city: string, medium_ms: int, slow_ms: int) -> string {
  let fast = spawn { make_quote(city + "-fast", 1, 20) };
  let medium = spawn { make_quote(city + "-medium", 2, medium_ms) };
  let slow = spawn { make_quote(city + "-slow", 3, slow_ms) };
  let winner = await baml.future.race([fast, medium, slow]);
  nap(30);
  winner.vendor
    + " medium_cancelled=" + medium.is_cancelled().to_string()
    + " slow_cancelled=" + slow.is_cancelled().to_string()
}

// The losers sleep far longer than any time a test run spends without a
// process, so that a loser's deadline never passes before the race is decided.
function race_local(city: string) -> string {
  race_of(city, 4000, 6000)
}

// All three deadlines pass while the test keeps the run without a process.
function race_short(city: string) -> string {
  race_of(city, 150, 250)
}

function race_remote(city: string) -> string {
  let fast = spawn { remote_quote(city + "-fast", 1, 30) };
  let medium = spawn { remote_quote(city + "-medium", 2, 400) };
  let slow = spawn { remote_quote(city + "-slow", 3, 600) };
  let winner = await baml.future.race([fast, medium, slow]);
  nap(30);
  winner.vendor
    + " medium_cancelled=" + medium.is_cancelled().to_string()
    + " slow_cancelled=" + slow.is_cancelled().to_string()
}

function any_all_failed(city: string) -> int {
  let fs = [
    spawn { make_quote(city, -1, 30) },
    spawn { make_quote(city, -20, 10) },
    spawn { make_quote(city, -300, 20) },
  ];
  let quote = (await baml.future.any(fs)) catch (e) {
    let all: baml.future.AllFailed<QuoteFailed> => {
      let sum = 0;
      for (let failure in all.errors) {
        sum = sum * 1000 - failure.code;
      }
      return sum;
    }
  };
  quote.price
}

function deadline(limit_ms: int, work_ms: int) -> string {
  let answer = baml.future.with_timeout(baml.time.Duration.from_milliseconds(limit_ms), () -> {
    make_quote("slow vendor", 9, work_ms).vendor
  }) catch (e) {
    let timeout: baml.errors.Timeout => "timed out: " + timeout.message,
    let failed: QuoteFailed => "failed",
  };
  nap(15);
  answer
}

function cancel_future(city: string) -> string {
  let victim = spawn {
    nap(5000);
    "finished " + city
  };
  nap(15);
  let did_cancel = victim.cancel();
  nap(25);
  let outcome = (await victim) catch (e) {
    baml.panics.Cancelled => "cancelled " + city
  };
  outcome + " " + did_cancel.to_string() + " " + victim.is_cancelled().to_string()
}

function wait_for_child(city: string) -> string {
  let child = spawn {
    nap(120);
    "finished " + city
  };
  nap(20);
  (await child) catch (e) {
    baml.panics.Cancelled => "cancelled " + city
  }
}

function wait_for_remote_child(city: string) -> string {
  let child = spawn { remote_quote(city, 5, 50).vendor };
  nap(20);
  (await child) catch (e) {
    baml.panics.Cancelled => "cancelled " + city
  }
}

function grouped(city: string) -> int[] {
  let g = baml.spawn.TaskGroup.new(2, name = "quotes");
  let fs: baml.future.Future<int, never>[] = [];
  let i = 0;
  while (i < 5) {
    let n = i;
    fs.push(spawn with baml.spawn.options(group = g) {
      nap(30);
      n * 10 + g.limit()
    });
    i += 1;
  }
  let results = await baml.future.all(fs);
  results.push(g.active_count());
  results.push(g.queued_count());
  results
}

function parked_error(city: string) -> int {
  let failing = spawn { make_quote(city, -3, 10) };
  nap(60);
  let code = (await failing) catch (e) {
    let failed: QuoteFailed => failed.code
  };
  match (code) {
    let quote: Quote => quote.price,
    let number: int => number,
  }
}

function shared_future(city: string) -> int {
  let shared = spawn { make_quote(city, 5, 50) };
  let first = spawn { (await shared).price + 1 };
  let second = spawn { (await shared).price + 2 };
  (await first) * 100 + (await second)
}

function token_any(city: string) -> string {
  let a = baml.spawn.CancelToken.new();
  let b = baml.spawn.CancelToken.new();
  let either = baml.spawn.CancelToken.any([a, b]);
  let worker = spawn with baml.spawn.options(cancel = either) {
    nap(5000);
    "finished " + city
  };
  nap(20);
  b.cancel();
  let outcome = (await worker) catch (e) {
    baml.panics.Cancelled => "cancelled " + city
  };
  outcome
    + " a=" + a.is_cancelled().to_string()
    + " b=" + b.is_cancelled().to_string()
    + " either=" + either.is_cancelled().to_string()
}

function walkers(city: string) -> string {
  let pause = () -> { nap(2) };
  let doubled = [3, 1, 2].map((n) -> { pause(); n * 2 });
  let kept = doubled.filter((n) -> { pause(); n > 2 });
  let sum = kept.reduce((acc, n) -> { pause(); acc + n }, 0);
  let found = doubled.find((n) -> { pause(); n == 4 });
  let last = doubled.find_last((n) -> { pause(); n > 1 });
  let some = doubled.some((n) -> { pause(); n == 6 });
  let every = doubled.every((n) -> { pause(); n > 0 });
  let flat = doubled.flat_map((n) -> { pause(); [n, n + 1] });
  let sorted = [5, 3, 9, 1].sort_by((a: int, b: int) -> baml.ops.Ordering throws never {
    pause();
    a.cmp(b)
  });
  city + " " + doubled.to_string() + " " + kept.to_string() + " " + sum.to_string()
    + " " + (found ?? -1).to_string() + " " + (last ?? -1).to_string()
    + " " + some.to_string() + " " + every.to_string() + " " + flat.to_string()
    + " " + sorted.to_string()
}
"#;

static COMPILED: OnceLock<Compiled> = OnceLock::new();

fn program() -> &'static Compiled {
    compiled(&COMPILED, SOURCE)
}

/// Pause `function` at every yield in turn and check each resumed run against
/// the uninterrupted run. Returns how many of the runs were actually moved.
async fn pause_at_every_yield(
    function: &'static str,
    args: Vec<BexExternalValue>,
    collect: bool,
) -> usize {
    let (expected, yields) = run_plain(program(), function, args.clone()).await;
    let expected_result = expected
        .result
        .unwrap_or_else(|error| panic!("`{function}` runs without a pause: {error}"));
    assert!(yields > 0, "`{function}` yields at least once");
    let mut moved = 0;
    // A few numbers beyond the count: the interleaving of threads varies.
    for k in 0..yields + 2 {
        let outcome = run_with_pauses(program(), function, args.clone(), &[k], collect).await;
        assert_eq!(
            outcome.result.as_ref().ok(),
            Some(&expected_result),
            "`{function}` paused at yield {k}: {:?}",
            outcome.result
        );
        assert_eq!(
            outcome.remote_calls, expected.remote_calls,
            "`{function}` paused at yield {k} announces the same remote calls"
        );
        if outcome.segments > 1 {
            moved += 1;
        }
    }
    assert!(
        moved * 2 >= yields,
        "`{function}`: only {moved} of {yields} pauses were served"
    );
    moved
}

// ─── Property-style coverage: a pause at EVERY yield ─────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn spawn_and_await_pause_at_every_yield_of_parent_and_child() {
    pause_at_every_yield("spawn_await", vec![string("Lisbon")], false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn nested_spawns_pause_at_every_yield() {
    pause_at_every_yield("nested", vec![string("Porto")], true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fan_out_of_class_results_pauses_at_every_yield() {
    pause_at_every_yield("fan_out", vec![string("Faro")], true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_remote_fan_out_pauses_at_every_yield() {
    pause_at_every_yield("remote_fan_out", vec![string("Braga")], false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn all_settled_with_a_typed_error_and_a_panic_pauses_at_every_yield() {
    pause_at_every_yield("settled", vec![string("Evora")], false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn any_with_all_inputs_failed_pauses_at_every_yield() {
    pause_at_every_yield("any_all_failed", vec![string("Tomar")], false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_race_pauses_at_every_yield_before_and_after_the_losers_are_cancelled() {
    pause_at_every_yield("race_local", vec![string("Beja")], false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn array_combinators_with_sleeping_callbacks_pause_at_every_yield() {
    pause_at_every_yield("walkers", vec![string("Sines")], true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_task_group_with_a_queue_pauses_at_every_yield() {
    pause_at_every_yield("grouped", vec![string("Viseu")], false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_future_awaited_by_two_threads_pauses_at_every_yield() {
    pause_at_every_yield("shared_future", vec![string("Guarda")], false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn future_cancel_pauses_at_every_yield() {
    pause_at_every_yield("cancel_future", vec![string("Aveiro")], false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_composite_cancel_token_pauses_at_every_yield() {
    pause_at_every_yield("token_any", vec![string("Leiria")], false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_error_parked_in_an_unawaited_future_pauses_at_every_yield() {
    pause_at_every_yield("parked_error", vec![string("Silves")], false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_timeout_pauses_at_every_yield() {
    // The deadline fires: the work takes longer than the limit.
    pause_at_every_yield(
        "deadline",
        vec![BexExternalValue::Int(40), BexExternalValue::Int(5000)],
        false,
    )
    .await;
    // The work finishes first, and the deadline thread is cancelled.
    pause_at_every_yield(
        "deadline",
        vec![BexExternalValue::Int(5000), BexExternalValue::Int(30)],
        false,
    )
    .await;
}

// ─── Targeted scenarios ──────────────────────────────────────────────────────

/// The state dump and the restore report describe every thread: ids, parents,
/// the futures they settle, and how each one is parked.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_four_thread_fan_out_is_described_and_restored_thread_by_thread() {
    let (expected, _) = run_plain(program(), "fan_out", vec![string("Faro")]).await;
    // The whole program runs for about 50 ms. On a loaded machine the pause
    // can land after the root has gone on to `baml.future.all`, or after the
    // run has ended; such a round is repeated.
    let mut rounds = 0;
    let (site, report, first_started) = loop {
        rounds += 1;
        assert!(rounds <= 20, "no pause landed while five threads ran");
        let site = Site::new(program(), Remote::Execute);
        let segment = Segment::start(&site, "fan_out", vec![string("Faro")], None);
        segment
            .host
            .wait_until("five threads run", |host| {
                host.started.lock().unwrap().len() >= 5
            })
            .await;
        let report = segment.pause(&site).await;
        let first_started = segment.host.started.lock().unwrap().clone();
        let outcome = segment.finish().await;
        match report {
            Some(report) if report.threads == 5 && first_started.len() == 5 => {
                assert_eq!(outcome.unwrap_err(), EngineError::DurableSuspended);
                break (site, report, first_started);
            }
            _ => {}
        }
    };

    let threads = report.committed.state["threads"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(threads.len(), 5);
    let root = threads
        .iter()
        .find(|thread| thread["parent_thread"].is_null())
        .expect("a root thread");
    assert!(root["settles_future"].is_null());
    for thread in threads.iter().filter(|t| !t["parent_thread"].is_null()) {
        assert_eq!(thread["parent_thread"], root["thread"]);
        assert!(thread["settles_future"].is_number(), "{thread}");
        assert_eq!(thread["cancelled"], false);
    }
    // The root holds the four futures in `fs`.
    assert_eq!(
        report.committed.state["heap"]["by_kind"]["future"]["count"], 4,
        "{}",
        report.committed.state["heap"]
    );

    let resumed = Segment::resume(
        &site,
        "fan_out",
        &report.committed,
        None,
        ResumeOptions::default(),
    );
    let host = Arc::clone(&resumed.host);
    let restored = Arc::clone(&resumed.restored);
    assert_eq!(resumed.finish().await.unwrap(), expected.result.unwrap());
    let info = restored.lock().unwrap().clone().expect("restored");
    assert_eq!(info.threads, 5);
    assert_eq!(info.pending_futures, 4);
    assert_eq!(info.thread_infos[0].parent, None);
    // Every thread reports its start with the id and parent it had before,
    // the root first.
    let mut restarted = host.started.lock().unwrap().clone();
    assert_eq!(restarted[0], (info.root_thread, None));
    restarted.sort_unstable();
    let mut original = first_started;
    original.sort_unstable();
    assert_eq!(restarted[..5], original[..]);
    // `baml.future.all` spawns its thread after the restore. It gets an id
    // above every restored id.
    assert_eq!(restarted[5..], [(6, Some(info.root_thread))]);
    // A spawned thread reports its end after it has settled its future, so
    // the last report can follow the root's result.
    host.wait_until("every thread has ended", |host| {
        host.ended.lock().unwrap().len() == 6
    })
    .await;
}

/// A chain of four pauses through one fan-out program, with a forced major
/// collection in every resumed engine.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fan_out_survives_a_chain_of_four_pauses() {
    let (expected, _) = run_plain(program(), "remote_fan_out", vec![string("Braga")]).await;
    let outcome = run_with_pauses(
        program(),
        "remote_fan_out",
        vec![string("Braga")],
        &[2, 1, 2, 1],
        true,
    )
    .await;
    assert_eq!(outcome.result.unwrap(), expected.result.unwrap());
    assert_eq!(outcome.remote_calls, expected.remote_calls);
    assert_eq!(outcome.segments, 5, "four pauses were served");
}

/// `race` over remote calls: the losers' threads are cancelled, the host
/// hears which calls the run abandoned, and late results for them change
/// nothing. The pause lands before the race is decided.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn race_losers_are_cancelled_after_a_pause_and_their_remote_calls_are_abandoned() {
    let site = Site::new(program(), Remote::Manual);
    let segment = Segment::start(&site, "race_remote", vec![string("Beja")], None);
    site.wait_until("three calls are announced", |site| {
        site.announced.lock().unwrap().len() == 3
    })
    .await;
    let report = segment.pause(&site).await.expect("live");
    assert_eq!(report.threads, 5, "root, three racers, and the race thread");
    assert_eq!(
        segment.finish().await.unwrap_err(),
        EngineError::DurableSuspended
    );
    assert!(site.cancelled.lock().unwrap().is_empty());

    // The three racers announce concurrently. Thread ids grow in spawn order,
    // which is fast, medium, slow.
    let mut announced = site.announced.lock().unwrap().clone();
    announced.sort_by_key(|(_, _, thread)| *thread);
    let call_of = |suffix: &str| {
        announced[match suffix {
            "fast" => 0,
            "medium" => 1,
            _ => 2,
        }]
        .clone()
    };
    let resumed = Segment::resume(
        &site,
        "race_remote",
        &report.committed,
        None,
        ResumeOptions::default(),
    );
    let quote = run_plain(program(), "spawn_await", vec![string("winner")])
        .await
        .0
        .result
        .unwrap();
    let (winner_call, _, winner_thread) = call_of("fast");
    site.deliver(&winner_call, Ok(quote.clone()));
    site.wait_until("both losers are abandoned", |site| {
        site.cancelled.lock().unwrap().len() == 2
    })
    .await;
    // A late and a duplicate result for abandoned calls.
    site.deliver(&call_of("medium").0, Ok(quote.clone()));
    site.deliver(&call_of("slow").0, Ok(quote.clone()));
    site.deliver(&winner_call, Ok(quote));
    let result = resumed.finish().await.unwrap();
    assert_eq!(
        result,
        string("winner medium_cancelled=true slow_cancelled=true")
    );
    let cancelled = site.cancelled.lock().unwrap().clone();
    assert!(
        cancelled
            .iter()
            .all(|(call, thread)| *call != winner_call && *thread != winner_thread),
        "{cancelled:?}"
    );
    assert_eq!(*site.received.lock().unwrap(), vec![winner_call]);
    assert!(
        site.announced.lock().unwrap().len() == 3,
        "the resumed run announces no call again"
    );
}

/// Remote waits in several threads: the results arrive while the run has no
/// process, in another order than the calls, and one of them twice.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn remote_results_in_another_order_and_with_duplicates_reach_the_right_threads() {
    let (expected, _) = run_plain(program(), "remote_fan_out", vec![string("Braga")]).await;
    let site = Site::new(program(), Remote::Manual);
    let segment = Segment::start(&site, "remote_fan_out", vec![string("Braga")], None);
    site.wait_until("four calls are announced", |site| {
        site.announced.lock().unwrap().len() == 4
    })
    .await;
    let report = segment.pause(&site).await.expect("live");
    assert_eq!(
        segment.finish().await.unwrap_err(),
        EngineError::DurableSuspended
    );
    let parked: Vec<String> = report.committed.state["threads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|thread| thread["parked"]["kind"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        parked.iter().filter(|kind| *kind == "remote_call").count(),
        4,
        "{parked:?}"
    );

    // The snapshot says which call each thread waits on. Answer every call
    // with the quote its own arguments produce, last call first.
    let expected_report = expected.result.unwrap();
    let BexExternalValue::Instance { fields, .. } = &expected_report else {
        panic!("a Report");
    };
    let BexExternalValue::Array { items: quotes, .. } = &fields["quotes"] else {
        panic!("quotes");
    };
    // Threads announce concurrently, so map call ids to quotes through the
    // spawn order: thread ids grow with the spawn order.
    let mut announced = site.announced.lock().unwrap().clone();
    announced.sort_by_key(|(_, _, thread)| *thread);
    for ((call_id, _, _), quote) in announced.iter().zip(quotes).rev() {
        site.deliver(call_id, Ok(quote.clone()));
        site.deliver(call_id, Ok(quote.clone()));
    }
    let resumed = Segment::resume(
        &site,
        "remote_fan_out",
        &report.committed,
        None,
        ResumeOptions::default(),
    );
    assert_eq!(resumed.finish().await.unwrap(), expected_report);
    assert_eq!(site.announced.lock().unwrap().len(), 4);
    assert_eq!(site.received.lock().unwrap().len(), 4);
}

/// A slow controller: every remote dispatch is delayed, the pause lands
/// before any remote run has started, and the results arrive in the resumed
/// process.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_slow_dispatch_between_sites_does_not_lose_a_call() {
    let (expected, _) = run_plain(program(), "remote_fan_out", vec![string("Braga")]).await;
    let site = Site::new(program(), Remote::Execute);
    *site.dispatch_delay.lock().unwrap() = Duration::from_millis(250);
    let segment = Segment::start(&site, "remote_fan_out", vec![string("Braga")], None);
    site.wait_until("four calls are announced", |site| {
        site.announced.lock().unwrap().len() == 4
    })
    .await;
    let report = segment.pause(&site).await.expect("live");
    assert_eq!(
        segment.finish().await.unwrap_err(),
        EngineError::DurableSuspended
    );
    let resumed = Segment::resume(
        &site,
        "remote_fan_out",
        &report.committed,
        None,
        ResumeOptions::default(),
    );
    assert_eq!(resumed.finish().await.unwrap(), expected.result.unwrap());
    assert_eq!(site.announced.lock().unwrap().len(), 4);
}

/// A thread is cancelled while the run has no process. The resume delivers
/// the cancellation before any thread runs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cancellation_decided_without_a_process_is_delivered_at_the_resume() {
    let site = Site::new(program(), Remote::Execute);
    let segment = Segment::start(&site, "wait_for_child", vec![string("Mafra")], None);
    // Wait for the CHILD's own sleep, not for any thread's: the root reaches a
    // sleep too, and pausing on that one leaves the child parked somewhere
    // else, which is what the assertions at the end of this test are about.
    segment
        .host
        .wait_until("the child sleeps", |host| {
            let started = host.started.lock().unwrap();
            let Some((child, _)) = started.get(1) else {
                return false;
            };
            let child = *child;
            drop(started);
            host.saw_on(child, YieldReason::SysOp, Some("baml.sys.sleep"))
        })
        .await;
    let report = segment.pause(&site).await.expect("live");
    let child = segment.host.started.lock().unwrap()[1].0;
    assert_eq!(
        segment.finish().await.unwrap_err(),
        EngineError::DurableSuspended
    );

    // Without the option the child finishes.
    let plain = Segment::resume(
        &site,
        "wait_for_child",
        &report.committed,
        None,
        ResumeOptions::default(),
    );
    assert_eq!(plain.finish().await.unwrap(), string("finished Mafra"));

    let cancelling = Segment::resume(
        &site,
        "wait_for_child",
        &report.committed,
        None,
        ResumeOptions {
            cancel_threads: vec![child],
            ..ResumeOptions::default()
        },
    );
    let restored = Arc::clone(&cancelling.restored);
    assert_eq!(
        cancelling.finish().await.unwrap(),
        string("cancelled Mafra")
    );
    let info = restored.lock().unwrap().clone().unwrap();
    let child_info = info
        .thread_infos
        .iter()
        .find(|thread| thread.thread == child)
        .unwrap();
    assert!(child_info.cancelled);
    assert!(matches!(child_info.parked, ParkedKind::Sleep { .. }));
}

/// A thread that waits on a remote call is cancelled while the run has no
/// process. Its wait is not issued again, and the host hears that the call is
/// abandoned.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cancelled_remote_wait_is_not_restored_as_a_live_wait() {
    let site = Site::new(program(), Remote::Manual);
    let segment = Segment::start(&site, "wait_for_remote_child", vec![string("Mafra")], None);
    site.wait_until("the call is announced", |site| {
        site.announced.lock().unwrap().len() == 1
    })
    .await;
    let report = segment.pause(&site).await.expect("live");
    let (call_id, _, child) = site.announced.lock().unwrap()[0].clone();
    assert_eq!(
        segment.finish().await.unwrap_err(),
        EngineError::DurableSuspended
    );

    let resumed = Segment::resume(
        &site,
        "wait_for_remote_child",
        &report.committed,
        None,
        ResumeOptions {
            cancel_threads: vec![child],
            ..ResumeOptions::default()
        },
    );
    assert_eq!(resumed.finish().await.unwrap(), string("cancelled Mafra"));
    assert_eq!(
        *site.cancelled.lock().unwrap(),
        vec![(call_id.clone(), child)]
    );
    assert!(
        site.received.lock().unwrap().is_empty(),
        "the cancelled thread never asked for the result"
    );
    // A result that arrives afterwards finds nobody.
    site.deliver(&call_id, Ok(string("late")));
}

/// A task group with limit 2 and five members, paused while two run and three
/// are queued: the queued threads are part of the snapshot, the restored group
/// keeps its limit, its name, and its queue order.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_task_group_paused_mid_way_keeps_its_queue() {
    let (expected, _) = run_plain(program(), "grouped", vec![string("Viseu")]).await;
    let site = Site::new(program(), Remote::Execute);
    let segment = Segment::start(&site, "grouped", vec![string("Viseu")], None);
    segment
        .host
        .wait_until("two members sleep and the root awaits", |host| {
            host.started.lock().unwrap().len() >= 3 && host.saw(YieldReason::Await, None)
        })
        .await;
    let report = segment.pause(&site).await.expect("live");
    assert_eq!(
        segment.finish().await.unwrap_err(),
        EngineError::DurableSuspended
    );
    let kinds: Vec<&str> = report.committed.state["threads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|thread| thread["parked"]["kind"].as_str().unwrap())
        .collect();
    let queued = kinds.iter().filter(|kind| **kind == "queued").count();
    assert!(queued >= 1, "some members still wait for a slot: {kinds:?}");
    assert!(
        report.committed.state.to_string().contains("task group"),
        "the dump shows the group"
    );

    let resumed = Segment::resume(
        &site,
        "grouped",
        &report.committed,
        None,
        ResumeOptions::default(),
    );
    assert_eq!(resumed.finish().await.unwrap(), expected.result.unwrap());
}

/// An automatic snapshot (the run keeps going) of a fan-out is a valid
/// starting point, and the original run is not disturbed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_automatic_snapshot_of_a_fan_out_can_be_resumed() {
    let (expected, _) = run_plain(program(), "fan_out", vec![string("Faro")]).await;
    let expected = expected.result.unwrap();
    let site = Site::new(program(), Remote::Execute);
    let segment = Segment::start(&site, "fan_out", vec![string("Faro")], None);
    segment
        .host
        .wait_until("five threads run", |host| {
            host.started.lock().unwrap().len() >= 5
        })
        .await;
    let mode = SnapshotMode::KeepRunning {
        park_timeout: Duration::from_secs(5),
    };
    let report = snapshot(&segment.engine, program().hash, mode, &site)
        .await
        .expect("a fan-out is a clean point");
    assert_eq!(segment.finish().await.unwrap(), expected);
    let resumed = Segment::resume(
        &site,
        "fan_out",
        &report.committed,
        None,
        ResumeOptions::default(),
    );
    assert_eq!(resumed.finish().await.unwrap(), expected);
}

/// Sizes and timings of a snapshot of a four-thread fan-out (plus the root),
/// printed for the phase report.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn measure_a_four_thread_fan_out_snapshot() {
    let site = Site::new(program(), Remote::Execute);
    let segment = Segment::start(&site, "fan_out", vec![string("Faro")], None);
    segment
        .host
        .wait_until("five threads run", |host| {
            host.started.lock().unwrap().len() >= 5
        })
        .await;
    let report = segment.pause(&site).await.expect("live");
    assert_eq!(
        segment.finish().await.unwrap_err(),
        EngineError::DurableSuspended
    );
    let resumed = Segment::resume(
        &site,
        "fan_out",
        &report.committed,
        None,
        ResumeOptions::default(),
    );
    let restored = Arc::clone(&resumed.restored);
    resumed.finish().await.unwrap();
    let info = restored.lock().unwrap().clone().unwrap();
    #[allow(clippy::print_stderr)]
    {
        eprintln!(
            "MEASURE fan_out threads={} objects={} raw_bytes={} compressed_bytes={} file_bytes={} \
         pause_latency_ms={:.3} walk_ms={:.3} encode_ms={:.3} compress_ms={:.3} decode_ms={:.3}",
            report.threads,
            report.stats.objects,
            report.stats.raw_bytes,
            report.stats.compressed_bytes,
            report.committed.bytes.len(),
            report.pause_latency_ms,
            report.stats.walk_ms,
            report.stats.encode_ms,
            report.stats.compress_ms,
            info.decode_ms,
        );
    }
    assert_eq!(site.call_counter.load(Ordering::Relaxed), 0);
}

/// Three racers sleep 20, 150 and 250 ms. The run has no process until all
/// three deadlines have passed, so every sleep completes at once after the
/// resume. They still complete in deadline order, and the 20 ms racer wins as
/// in an uninterrupted run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sleeps_that_expired_without_a_process_complete_in_deadline_order() {
    let mut conclusive = 0;
    for _ in 0..10 {
        let site = Site::new(program(), Remote::Execute);
        let segment = Segment::start(&site, "race_short", vec![string("Beja")], None);
        segment
            .host
            .wait_until("the three racers sleep", |host| {
                host.yield_log
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|(_, _, op)| op.as_deref() == Some("baml.sys.sleep"))
                    .count()
                    >= 3
            })
            .await;
        // The fast racer sleeps only 20 ms. On a loaded machine the pause can
        // land after that sleep; such a round says nothing about the order of
        // expired sleeps and is repeated.
        let Some(report) = segment.pause(&site).await else {
            segment.finish().await.unwrap();
            continue;
        };
        assert_eq!(
            segment.finish().await.unwrap_err(),
            EngineError::DurableSuspended
        );
        let sleeping = report.committed.state["threads"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|thread| thread["parked"]["kind"] == "sleep")
            .count();
        if sleeping != 3 {
            continue;
        }
        conclusive += 1;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let resumed = Segment::resume(
            &site,
            "race_short",
            &report.committed,
            None,
            ResumeOptions::default(),
        );
        assert_eq!(
            resumed.finish().await.unwrap(),
            string("Beja-fast medium_cancelled=true slow_cancelled=true")
        );
        if conclusive == 5 {
            break;
        }
    }
    assert!(
        conclusive >= 1,
        "no round paused the run while all three racers slept"
    );
}
