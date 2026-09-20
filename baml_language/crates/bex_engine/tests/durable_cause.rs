//! Durable functions proof of concept, contract section 10.1: why a remote
//! call was abandoned, and where it was made.
//!
//! The programs have the shape of the demo functions: a `race` over three
//! remote calls, a `with_timeout` around one, and a thread whose parent is
//! cancelled underneath it. The site never answers a call by itself
//! ([`Remote::Manual`]), so every test decides exactly when a result arrives.

mod common;
#[allow(dead_code)]
mod durable_harness;

use std::sync::{Arc, OnceLock};

use bex_engine::durable::{
    CancelCause, DurableThreadId, RemoteCancel, ResumeOptions, YieldPosition,
};
use durable_harness::{Compiled, Remote, Segment, Site, compiled, string};

const SOURCE: &str = r#"
function remote_quote(vendor: string) -> string {
  "quote from " + vendor
}

function remote_quote_caller(city: string) -> string {
  remote_quote(city + "-root")
}

function race_caller(city: string) -> string {
  let fast = spawn { remote_quote(city + "-fast") };
  let medium = spawn { remote_quote(city + "-medium") };
  let slow = spawn { remote_quote(city + "-slow") };
  await baml.future.race([fast, medium, slow])
}

function all_caller(city: string) -> string[] {
  let first = spawn { remote_quote(city + "-first") };
  let second = spawn { remote_quote(city + "-second") };
  await baml.future.all([first, second])
}

function deadline_caller(city: string) -> string {
  baml.future.with_timeout(baml.time.Duration.from_milliseconds(50n), () -> {
    remote_quote(city + "-slow")
  }) catch (e) {
    baml.errors.Timeout => "timed out",
  }
}

function cascade_caller(city: string) -> string {
  let outer = spawn {
    let inner = spawn { remote_quote(city + "-inner") };
    let mine = remote_quote(city + "-outer");
    mine + (await inner)
  };
  let gate = remote_quote(city + "-gate");
  outer.cancel();
  baml.sys.sleep(baml.time.Duration.from_milliseconds(300n));
  gate
}

function pausing_caller(city: string) -> string {
  let child = spawn {
    remote_quote(city + "-child")
  };
  await child
}
"#;

fn program() -> &'static Compiled {
    static COMPILED: OnceLock<Compiled> = OnceLock::new();
    compiled(&COMPILED, SOURCE)
}

/// The 1-based line of the only line of [`SOURCE`] that contains `needle`.
fn line_of(needle: &str) -> usize {
    let mut found = None;
    for (index, line) in SOURCE.lines().enumerate() {
        if line.contains(needle) {
            assert!(found.is_none(), "`{needle}` is on more than one line");
            found = Some(index + 1);
        }
    }
    found.unwrap_or_else(|| panic!("`{needle}` is not in the source"))
}

/// The call id of the announced call whose first string argument ends with
/// `suffix`.
fn call_id_ending_in(site: &Site, suffix: &str) -> String {
    let vendors = site.announced_vendors.lock().unwrap();
    vendors
        .iter()
        .find(|(_, vendor)| vendor.ends_with(suffix))
        .map(|(call_id, _)| call_id.clone())
        .unwrap_or_else(|| panic!("no call for `{suffix}`: {vendors:?}"))
}

fn reports(site: &Site) -> Vec<RemoteCancel> {
    site.cancel_reports.lock().unwrap().clone()
}

/// The one report for the call whose first string argument ends with `suffix`.
fn report_for(site: &Site, suffix: &str) -> RemoteCancel {
    let call_id = call_id_ending_in(site, suffix);
    let reports = reports(site);
    let mut matching = reports
        .iter()
        .filter(|report| report.call_id == call_id)
        .cloned();
    let report = matching
        .next()
        .unwrap_or_else(|| panic!("no cancel report for `{suffix}`: {reports:?}"));
    assert!(
        matching.next().is_none(),
        "`{suffix}` was reported twice: {reports:?}"
    );
    report
}

fn site_of(report: &RemoteCancel) -> YieldPosition {
    report
        .site
        .clone()
        .unwrap_or_else(|| panic!("the report carries no call site: {report:?}"))
}

async fn wait_for_announced(site: &Site, count: usize) {
    site.wait_until("every call is announced", |site| {
        site.announced.lock().unwrap().len() == count
    })
    .await;
}

async fn wait_for_cancels(site: &Site, count: usize) {
    site.wait_until("the abandoned calls are reported", |site| {
        site.cancel_reports.lock().unwrap().len() == count
    })
    .await;
}

/// A `race` cancels its losers by cancelling their futures, and each loser
/// names the line its own call was written on.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_race_loser_reports_its_own_call_site_and_a_cancelled_future() {
    let site = Site::new(program(), Remote::Manual);
    let segment = Segment::start(&site, "race_caller", vec![string("Lisbon")], None);
    wait_for_announced(&site, 3).await;

    site.deliver(
        &call_id_ending_in(&site, "-fast"),
        Ok(string("quote from Lisbon-fast")),
    );
    wait_for_cancels(&site, 2).await;

    for (suffix, needle) in [
        (
            "-medium",
            r#"let medium = spawn { remote_quote(city + "-medium") };"#,
        ),
        (
            "-slow",
            r#"let slow = spawn { remote_quote(city + "-slow") };"#,
        ),
    ] {
        let report = report_for(&site, suffix);
        assert_eq!(
            report.cause,
            CancelCause::FutureCancel,
            "{suffix}: {report:?}"
        );
        assert_eq!(site_of(&report).line, line_of(needle), "{suffix}");
    }
    assert!(
        reports(&site)
            .iter()
            .all(|report| !report.call_id.ends_with("-fast")),
        "the winner is not abandoned"
    );
    assert_eq!(
        segment.finish().await.unwrap(),
        string("quote from Lisbon-fast")
    );
}

/// `baml.future.all` drops the inputs that are still pending after the first
/// error by cancelling their futures (`futures.map((g) -> { g.cancel() })`),
/// not by cancelling a parent thread: the inputs were spawned by the caller,
/// not by the helper thread that `all` runs in. The engine therefore reports
/// `future_cancel`, and each dropped input names its own call site.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_input_that_all_drops_after_an_error_reports_a_cancelled_future() {
    let site = Site::new(program(), Remote::Manual);
    let segment = Segment::start(&site, "all_caller", vec![string("Aveiro")], None);
    wait_for_announced(&site, 2).await;

    site.deliver(
        &call_id_ending_in(&site, "-first"),
        Err("the vendor is unavailable".to_string()),
    );
    wait_for_cancels(&site, 1).await;

    let report = report_for(&site, "-second");
    assert_eq!(report.cause, CancelCause::FutureCancel, "{report:?}");
    assert_eq!(
        site_of(&report).line,
        line_of(r#"  let second = spawn { remote_quote(city + "-second") };"#)
    );
    assert!(segment.finish().await.is_err(), "`all` rethrows the error");
}

/// `with_timeout` cancels its body through a user `baml.spawn.CancelToken`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_timeout_reports_a_cancel_token() {
    let site = Site::new(program(), Remote::Manual);
    let segment = Segment::start(&site, "deadline_caller", vec![string("Porto")], None);
    wait_for_announced(&site, 1).await;
    wait_for_cancels(&site, 1).await;

    let report = report_for(&site, "-slow");
    assert_eq!(report.cause, CancelCause::Token, "{report:?}");
    assert_eq!(
        site_of(&report).line,
        line_of(r#"    remote_quote(city + "-slow")"#)
    );
    assert_eq!(segment.finish().await.unwrap(), string("timed out"));
}

/// Cancelling a spawned thread's future cancels the thread (`future_cancel`)
/// and cascades into the thread it had spawned (`parent`). Both wait on their
/// own remote call, and each names its own call site.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cancelled_thread_and_its_child_report_different_causes() {
    let site = Site::new(program(), Remote::Manual);
    let segment = Segment::start(&site, "cascade_caller", vec![string("Faro")], None);
    // The root's own call is the gate: once it is announced together with the
    // two calls of the spawned threads, both of those threads are parked in
    // their waits, and answering the gate releases `outer.cancel()`.
    wait_for_announced(&site, 3).await;
    site.deliver(
        &call_id_ending_in(&site, "-gate"),
        Ok(string("quote from Faro-gate")),
    );
    wait_for_cancels(&site, 2).await;

    let outer = report_for(&site, "-outer");
    assert_eq!(outer.cause, CancelCause::FutureCancel, "{outer:?}");
    assert_eq!(
        site_of(&outer).line,
        line_of(r#"    let mine = remote_quote(city + "-outer");"#)
    );

    let inner = report_for(&site, "-inner");
    assert_eq!(inner.cause, CancelCause::Parent, "{inner:?}");
    assert_eq!(
        site_of(&inner).line,
        line_of(r#"    let inner = spawn { remote_quote(city + "-inner") };"#)
    );
    assert_ne!(outer.thread, inner.thread);

    assert_eq!(
        segment.finish().await.unwrap(),
        string("quote from Faro-gate")
    );
}

/// The call site travels in the snapshot: a process that never made the call
/// reports the line it was made on.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_call_site_survives_a_pause_and_is_reported_after_a_resume() {
    let site = Site::new(program(), Remote::Manual);
    let first = Segment::start(&site, "pausing_caller", vec![string("Braga")], None);
    wait_for_announced(&site, 1).await;
    let report = first
        .pause(&site)
        .await
        .expect("the run pauses while the child waits for its result");
    assert!(
        reports(&site).is_empty(),
        "a pause abandons no call: {:?}",
        reports(&site)
    );
    let child: DurableThreadId = first
        .host
        .started
        .lock()
        .unwrap()
        .iter()
        .find_map(|(thread, parent)| parent.map(|_| *thread))
        .expect("the run spawned a thread");
    let announced_site = site
        .announced_sites
        .lock()
        .unwrap()
        .values()
        .next()
        .cloned()
        .flatten()
        .expect("the announcement carried a call site");
    let _ = first.finish().await;

    // The site server decided while the run had no process that the child is
    // cancelled. The resumed run must still name where the call was made.
    let second = Segment::resume(
        &site,
        "pausing_caller",
        &report.committed,
        None,
        ResumeOptions {
            cancel_threads: vec![child],
            ..ResumeOptions::default()
        },
    );
    wait_for_cancels(&site, 1).await;

    let report = report_for(&site, "-child");
    assert_eq!(
        site_of(&report).line,
        line_of(r#"    remote_quote(city + "-child")"#)
    );
    assert_eq!(site_of(&report), announced_site);
    assert_eq!(report.cause, CancelCause::FutureCancel, "{report:?}");
    let _ = second.finish().await;
}

/// The cancellation of the run itself does not name a token that tells the
/// cases apart, so it stays `unknown`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cancelled_run_reports_an_unknown_cause() {
    let site = Site::new(program(), Remote::Manual);
    let cancel = bex_engine::CancellationToken::new();
    let engine = durable_harness::new_engine(program());
    let _host = durable_harness::Host::install(&site, &engine, None);
    let run = tokio::spawn({
        let engine = Arc::clone(&engine);
        let ctx = bex_engine::FunctionCallContextBuilder::new(sys_types::CallId::next())
            .with_cancel_token(cancel.clone())
            .build();
        async move {
            engine
                .call_function("remote_quote_caller", vec![string("Evora")], ctx, true)
                .await
        }
    });
    wait_for_announced(&site, 1).await;
    cancel.cancel();
    wait_for_cancels(&site, 1).await;

    let report = report_for(&site, "-root");
    assert_eq!(report.cause, CancelCause::Unknown, "{report:?}");
    assert_eq!(
        site_of(&report).line,
        line_of(r#"  remote_quote(city + "-root")"#)
    );
    let _ = run.await.expect("the run task does not panic");
}
