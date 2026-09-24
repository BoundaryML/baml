//! Durable functions, phase 3: the ordered replay of a restored run.
//!
//! A run that had no process for a while can come back to several waits that
//! are already satisfied: remote results that arrived and sleeps whose
//! deadline passed. An uninterrupted run would have seen them one at a time,
//! in the order of their times. These tests check that a resumed run sees the
//! same order, whatever the task scheduler does, and that the identifiers a
//! host derives for remote calls do not depend on the scheduler either.

#![cfg(not(target_arch = "wasm32"))]

mod common;
#[allow(dead_code)]
mod durable_harness;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, OnceLock},
    time::Duration,
};

use bex_engine::{
    BexExternalValue, EngineError,
    durable::{ResumeOptions, SnapshotFailure, SnapshotMode},
};
use durable_harness::{
    Compiled, Remote, Segment, Site, compiled, ctx, new_engine, snapshot, string, unix_ms_now,
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

class Board {
  city string
  quotes Quote[]
  by_vendor map<string, Quote>
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

// The ring (a cycle) and the map live across the pause next to the futures.
function race_three(city: string) -> string {
  let ring = new_ring();
  let seen: map<string, int> = { "start": 1 };
  let a = spawn { remote_quote(city + "-a", 10) };
  let b = spawn { remote_quote(city + "-b", 120) };
  let c = spawn { remote_quote(city + "-c", 55) };
  let winner = await baml.future.race([a, b, c]);
  seen.set(winner.vendor, winner.price);
  let next = ring.next;
  let ring_name = "";
  if (next != null) {
    ring_name = next.name;
  }
  winner.vendor
    + " tier=" + winner.tier.to_string()
    + " a=" + a.is_cancelled().to_string()
    + " b=" + b.is_cancelled().to_string()
    + " c=" + c.is_cancelled().to_string()
    + " ring=" + ring_name
    + " seen=" + seen.length().to_string()
}

function deadline(city: string, limit_ms: int) -> string {
  let answer = baml.future.with_timeout(baml.time.Duration.from_milliseconds(limit_ms), () -> {
    remote_quote(city, 77).vendor
  }) catch (e) {
    let timeout: baml.errors.Timeout => "timed out",
  };
  answer
}

// A thread that sleeps in the background keeps the run suspended for a long
// time. The root only needs the remote result.
function background_sleeper(city: string, ms: int) -> string {
  let bg = spawn {
    nap(ms);
    1
  };
  let child = spawn { remote_quote(city, 9) };
  let quote = await child;
  quote.vendor + " " + quote.price.to_string()
}

// After the first result the root sleeps `gap_ms` and then cancels the second
// call when it is still pending.
function patient(city: string, gap_ms: int) -> string {
  let first = spawn { remote_quote(city + "-first", 1) };
  let second = spawn { remote_quote(city + "-second", 2) };
  let one = await first;
  nap(gap_ms);
  let cancelled = second.cancel();
  let two = (await second) catch (e) {
    baml.panics.Cancelled => build_quote("nobody", 0)
  };
  one.vendor + " then " + two.vendor + " cancelled=" + cancelled.to_string()
}

// Three levels of threads. Each leaf makes two remote calls.
function leaf(name: string) -> Quote[] {
  let one = remote_quote(name + "-1", 3);
  let two = remote_quote(name + "-2", 4);
  [one, two]
}

function branch(name: string) -> Quote[] {
  let left = spawn { leaf(name + "-l") };
  let right = spawn { leaf(name + "-r") };
  let quotes: Quote[] = [];
  for (let q in await left) {
    quotes.push(q);
  }
  for (let q in await right) {
    quotes.push(q);
  }
  quotes
}

function tree(city: string) -> Board {
  let ring = new_ring();
  let north = spawn { branch(city + "-n") };
  let south = spawn { branch(city + "-s") };
  let quotes: Quote[] = [];
  let by_vendor: map<string, Quote> = {};
  for (let q in await north) {
    quotes.push(q);
    by_vendor.set(q.vendor, q);
  }
  for (let q in await south) {
    quotes.push(q);
    by_vendor.set(q.vendor, q);
  }
  Board { city: city, quotes: quotes, by_vendor: by_vendor, ring: ring.name }
}
"#;

fn program() -> &'static Compiled {
    static PROGRAM: OnceLock<Compiled> = OnceLock::new();
    compiled(&PROGRAM, SOURCE)
}

fn int(value: i64) -> BexExternalValue {
    BexExternalValue::Int(value)
}

/// A `Quote` instance as the remote side would return it.
async fn quote(vendor: &str, price: i64) -> BexExternalValue {
    new_engine(program())
        .call_function("build_quote", vec![string(vendor), int(price)], ctx(), true)
        .await
        .expect("build_quote runs")
}

/// The announced call whose `vendor` argument ends with `suffix`.
fn call_with_suffix(site: &Site, calls: &BTreeMap<String, String>, suffix: &str) -> String {
    let _ = site;
    calls
        .iter()
        .find(|(_, vendor)| vendor.ends_with(suffix))
        .map(|(call_id, _)| call_id.clone())
        .unwrap_or_else(|| panic!("no call for {suffix}: {calls:?}"))
}

/// Pause `function` once `calls` remote calls are announced. Returns the
/// snapshot and the vendor argument of every call by call id.
async fn pause_after_calls(
    site: &Arc<Site>,
    function: &'static str,
    args: Vec<BexExternalValue>,
    calls: usize,
) -> (durable_harness::Snapshot, BTreeMap<String, String>) {
    let segment = Segment::start(site, function, args, None);
    site.wait_until("the calls are announced", |site| {
        site.announced.lock().unwrap().len() == calls
    })
    .await;
    let report = segment.pause(site).await.expect("the run is live");
    assert_eq!(
        segment.finish().await.unwrap_err(),
        EngineError::DurableSuspended
    );
    let vendors = site.announced_vendors.lock().unwrap().clone();
    (report.committed, vendors)
}

/// All three results of a race arrive while the run has no process. The
/// resumed run must pick the one that arrived first, every time, and abandon
/// the other two. Before the ordered replay the winner depended on which
/// restored thread the scheduler ran first.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_race_decided_without_a_process_picks_the_first_arrival_every_time() {
    let site = Site::new(program(), Remote::Manual);
    let (snapshot, calls) = pause_after_calls(&site, "race_three", vec![string("Beja")], 3).await;
    let now = unix_ms_now();
    // Arrival order b, c, a: neither the spawn order nor the call id order.
    let order = ["-b", "-c", "-a"];
    for (index, suffix) in order.iter().enumerate() {
        let call_id = call_with_suffix(&site, &calls, suffix);
        let vendor = format!("Beja{suffix}");
        site.deliver_at(
            &call_id,
            Ok(quote(&vendor, 120).await),
            now - 9000 + 1000 * index as u64,
        );
    }
    for round in 0..24 {
        site.cancelled.lock().unwrap().clear();
        site.received.lock().unwrap().clear();
        let resumed = Segment::resume(
            &site,
            "race_three",
            &snapshot,
            None,
            ResumeOptions::default(),
        );
        let result = resumed.finish().await.unwrap();
        assert_eq!(
            result,
            string("Beja-b tier=Premium a=true b=false c=true ring=b seen=2"),
            "round {round}"
        );
        assert_eq!(
            *site.received.lock().unwrap(),
            vec![call_with_suffix(&site, &calls, "-b")],
            "round {round}: only the winner's result is taken"
        );
        // The losers are cancelled by `race` on threads of their own, and the
        // engine does not order the root's result against their teardown: the
        // run can report its value before both notifications have landed. The
        // worker drains its threads before it reports a terminal event, so a
        // site server sees both; a test that drives the engine directly has to
        // wait for them. Waiting cannot hide a missing cancellation, because a
        // notification that never arrives still fails the assertion below.
        site.wait_until("both losers report their cancellation", |site| {
            site.cancelled.lock().unwrap().len() >= 2
        })
        .await;
        let cancelled: BTreeSet<String> = site
            .cancelled
            .lock()
            .unwrap()
            .iter()
            .map(|(call_id, _)| call_id.clone())
            .collect();
        assert_eq!(
            cancelled,
            [
                call_with_suffix(&site, &calls, "-a"),
                call_with_suffix(&site, &calls, "-c")
            ]
            .into_iter()
            .collect(),
            "round {round}: the losers are abandoned although their results had arrived"
        );
    }
}

/// `with_timeout` across a gap without a process: a result that arrived
/// before the deadline wins, and a result that arrived after the deadline
/// loses against the timer, as in an uninterrupted run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_timeout_decided_without_a_process_follows_the_recorded_times() {
    for (arrival_offset_ms, expected) in [(-400_i64, "Lagos"), (400, "timed out")] {
        let site = Site::new(program(), Remote::Manual);
        let started = unix_ms_now();
        let (snapshot, calls) =
            pause_after_calls(&site, "deadline", vec![string("Lagos"), int(1500)], 1).await;
        let timer_deadline = snapshot.state["threads"]
            .as_array()
            .unwrap()
            .iter()
            .find(|thread| thread["parked"]["kind"] == "sleep")
            .map(|thread| {
                serde_json::from_str::<serde_json::Value>(
                    thread["parked"]["detail"].as_str().unwrap_or("{}"),
                )
                .ok()
                .and_then(|detail| detail["deadline_unix_ms"].as_u64())
                .unwrap_or(started + 1500)
            })
            .expect("the timer thread sleeps");
        let call_id = calls.keys().next().unwrap().clone();
        let arrival =
            u64::try_from(i64::try_from(timer_deadline).unwrap() + arrival_offset_ms).unwrap();
        // Resume only once both the deadline and the arrival are in the past.
        let wait_until = timer_deadline.max(arrival) + 50;
        tokio::time::sleep(Duration::from_millis(
            wait_until.saturating_sub(unix_ms_now()),
        ))
        .await;
        site.deliver_at(&call_id, Ok(quote("Lagos", 77).await), arrival);
        for round in 0..8 {
            let resumed =
                Segment::resume(&site, "deadline", &snapshot, None, ResumeOptions::default());
            assert_eq!(
                resumed.finish().await.unwrap(),
                string(expected),
                "arrival {arrival_offset_ms} ms from the deadline, round {round}"
            );
        }
    }
}

/// A resume long before the background sleep ends, with the awaited result
/// already stored: the run must take the result and complete. It must not be
/// reported idle while the result waits for its thread.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_early_resume_takes_a_stored_result_instead_of_suspending_again() {
    let site = Site::new(program(), Remote::Manual);
    let segment = Segment::start(
        &site,
        "background_sleeper",
        vec![string("Tavira"), int(40_000)],
        None,
    );
    let (idle, report) = segment.self_suspend(&site, 3000).await;
    assert!(idle.remaining_ms > 30_000);
    let report = report.expect("the run suspends itself");
    assert_eq!(report.threads, 3);
    assert_eq!(
        segment.finish().await.unwrap_err(),
        EngineError::DurableSuspended
    );
    let call_id = site.announced.lock().unwrap()[0].0.clone();
    site.deliver(&call_id, Ok(quote("Tavira", 9).await));

    let resumed = Segment::resume(
        &site,
        "background_sleeper",
        &report.committed,
        None,
        ResumeOptions::default(),
    );
    // Play the worker: try to suspend as soon as the rule holds.
    let engine = Arc::clone(&resumed.engine);
    let suspender = tokio::spawn({
        let site = Arc::clone(&site);
        async move {
            engine.durable_idle_sleep(3000).await;
            snapshot(
                &engine,
                site_hash(),
                SnapshotMode::SuspendIfIdle {
                    min_remaining_ms: 3000,
                },
                &site,
            )
            .await
        }
    });
    let result = tokio::time::timeout(Duration::from_secs(20), resumed.finish())
        .await
        .expect("the resumed run completes instead of suspending again");
    assert_eq!(result.unwrap(), string("Tavira 9"));
    assert_eq!(*site.received.lock().unwrap(), vec![call_id]);
    suspender.abort();
    match suspender.await {
        Err(_) | Ok(Err(SnapshotFailure::RunEnded | SnapshotFailure::NotIdle)) => {}
        Ok(other) => panic!("the run must not suspend again: {other:?}"),
    }
}

fn site_hash() -> [u8; 32] {
    program().hash
}

/// A sleep that a thread starts during the replay begins at the replay's
/// virtual time. The root sleeps 400 ms after the first result and then
/// cancels the second call. With the second result recorded 1200 ms after the
/// first, the cancel comes first; with the second result recorded 100 ms after
/// the first, the result comes first. Both are what an uninterrupted run does.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_sleep_started_during_the_replay_is_ordered_against_recorded_results() {
    for (second_after_ms, expected) in [
        (1200_u64, "Faro-first then nobody cancelled=true"),
        (100, "Faro-first then Faro-second cancelled=false"),
    ] {
        let site = Site::new(program(), Remote::Manual);
        let (snapshot, calls) =
            pause_after_calls(&site, "patient", vec![string("Faro"), int(400)], 2).await;
        // The results arrive after the snapshot was written and before the
        // resume, which is the only time a recorded result can come from.
        let base = unix_ms_now() + 20;
        tokio::time::sleep(Duration::from_millis(second_after_ms + 60)).await;
        site.deliver_at(
            &call_with_suffix(&site, &calls, "-first"),
            Ok(quote("Faro-first", 1).await),
            base,
        );
        site.deliver_at(
            &call_with_suffix(&site, &calls, "-second"),
            Ok(quote("Faro-second", 2).await),
            base + second_after_ms,
        );
        for round in 0..6 {
            let started = std::time::Instant::now();
            let resumed =
                Segment::resume(&site, "patient", &snapshot, None, ResumeOptions::default());
            assert_eq!(
                resumed.finish().await.unwrap(),
                string(expected),
                "second result {second_after_ms} ms after the first, round {round}"
            );
            if second_after_ms > 400 {
                assert!(
                    started.elapsed() < Duration::from_millis(350),
                    "the 400 ms sleep ended in the past, so it is not slept again: {:?}",
                    started.elapsed()
                );
            }
        }
    }
}

/// Restored threads are announced parent first, and the identity a host gets
/// for a remote call (`thread_path`, `call_index`) names the same call in the
/// first execution and in every execution that starts again from a snapshot
/// taken while the calls were being announced.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn call_identities_and_thread_announcements_do_not_depend_on_the_scheduler() {
    // The identities of an uninterrupted run.
    let reference = Site::new(program(), Remote::Execute);
    let board = Segment::start(&reference, "tree", vec![string("Evora")], None)
        .finish()
        .await
        .unwrap();
    let identities = |site: &Site| -> BTreeMap<(String, u64), String> {
        site.announced_paths
            .lock()
            .unwrap()
            .iter()
            .map(|(path, index, _)| (path.clone(), *index))
            .zip(
                site.announced_vendors_in_order
                    .lock()
                    .unwrap()
                    .iter()
                    .cloned(),
            )
            .collect()
    };
    let expected = identities(&reference);
    assert_eq!(expected.len(), 8, "{expected:?}");
    assert_eq!(expected[&("0.0.0".to_string(), 1)], "Evora-n-l-1");
    assert_eq!(expected[&("0.0.1".to_string(), 2)], "Evora-n-r-2");
    assert_eq!(expected[&("0.1.0".to_string(), 2)], "Evora-s-l-2");

    for pause_after in [1_usize, 2, 3, 5] {
        let site = Site::new(program(), Remote::Execute);
        *site.dispatch_delay.lock().unwrap() = Duration::from_millis(40);
        let segment = Segment::start(&site, "tree", vec![string("Evora")], None);
        site.wait_until("some calls are announced", |site| {
            site.announced.lock().unwrap().len() >= pause_after
        })
        .await;
        let Some(report) = segment.pause(&site).await else {
            continue;
        };
        assert_eq!(
            segment.finish().await.unwrap_err(),
            EngineError::DurableSuspended
        );
        // Two executions from the same snapshot: a resume and a recovery.
        for _ in 0..2 {
            let resumed = Segment::resume(
                &site,
                "tree",
                &report.committed,
                None,
                ResumeOptions::default(),
            );
            let host = Arc::clone(&resumed.host);
            assert_eq!(resumed.finish().await.unwrap(), board);
            // Every identity maps to the same request as in the reference
            // run, in every execution.
            let seen = site
                .announced_paths
                .lock()
                .unwrap()
                .iter()
                .map(|(path, index, _)| (path.clone(), *index))
                .zip(
                    site.announced_vendors_in_order
                        .lock()
                        .unwrap()
                        .iter()
                        .cloned(),
                )
                .collect::<Vec<_>>();
            for (identity, vendor) in &seen {
                assert_eq!(&expected[identity], vendor, "{identity:?}");
            }
            // A restored thread is never announced before its parent.
            let started = host.started.lock().unwrap().clone();
            let mut known = BTreeSet::new();
            let restored: BTreeSet<u64> = started.iter().map(|(thread, _)| *thread).collect();
            for (thread, parent) in &started {
                if let Some(parent) = parent {
                    assert!(
                        known.contains(parent) || !restored.contains(parent),
                        "thread {thread} was announced before its parent {parent}: {started:?}"
                    );
                }
                known.insert(*thread);
            }
        }
    }
}
