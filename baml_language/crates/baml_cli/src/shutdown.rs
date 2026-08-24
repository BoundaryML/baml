use std::{collections::BTreeMap, future::Future, sync::Arc, time::Duration};

use bex_engine::BexEngine;

use crate::reporter::Reporter;

/// Default grace for the end-of-run wait on orphaned background futures
/// before they are cancelled and abandoned. Override with
/// `BAML_SHUTDOWN_GRACE_MS`; `0` waits forever (the pre-deadline behavior).
const DEFAULT_SHUTDOWN_GRACE: Duration = Duration::from_secs(15);

fn shutdown_grace() -> Option<Duration> {
    match std::env::var("BAML_SHUTDOWN_GRACE_MS") {
        Ok(raw) => match raw.trim().parse::<u64>() {
            Ok(0) => None,
            Ok(ms) => Some(Duration::from_millis(ms)),
            Err(_) => Some(DEFAULT_SHUTDOWN_GRACE),
        },
        Err(_) => Some(DEFAULT_SHUTDOWN_GRACE),
    }
}

pub(crate) fn shutdown_engine(
    rt: &tokio::runtime::Runtime,
    engine: &Arc<BexEngine>,
    reporter: &Reporter,
) {
    rt.block_on(shutdown_engine_future(engine, reporter));
}

pub(crate) fn shutdown_engine_future<'a>(
    engine: &'a Arc<BexEngine>,
    reporter: &'a Reporter,
) -> impl Future<Output = ()> + 'a {
    engine.shutdown_with_deadline(
        shutdown_grace(),
        |count| {
            reporter.status("Waiting", wait_message(count));
        },
        |leaks| {
            if !leaks.is_empty() {
                reporter.warning(leak_message(leaks));
            }
        },
    )
}

fn wait_message(count: usize) -> String {
    let futures = if count == 1 { "future" } else { "futures" };
    format!("for {count} remaining BAML {futures} to finish (press Ctrl+C to cancel now)")
}

/// One warning line summarizing abandoned background futures, grouped by
/// spawn provenance: `abandoned 15 leaked background future(s):
/// user.llm_mock.mock_json_serve x12, user.http_server.serve x3 (...)`.
fn leak_message(leaks: &[bex_engine::LeakedFuture]) -> String {
    let mut by_origin: BTreeMap<&str, usize> = BTreeMap::new();
    for leak in leaks {
        *by_origin.entry(leak.origin.as_ref()).or_default() += 1;
    }
    let mut groups: Vec<(usize, &str)> = by_origin
        .into_iter()
        .map(|(origin, count)| (count, origin))
        .collect();
    groups.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    let listed = groups
        .iter()
        .map(|(count, origin)| {
            if *count == 1 {
                (*origin).to_string()
            } else {
                format!("{origin} x{count}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let futures = if leaks.len() == 1 {
        "future"
    } else {
        "futures"
    };
    format!(
        "abandoned {} leaked background {futures} spawned in: {listed} — the owning \
         test or call finished without cleaning them up (BAML_SHUTDOWN_GRACE_MS \
         adjusts the wait; 0 waits forever)",
        leaks.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_message_pluralizes_future_count() {
        assert_eq!(
            wait_message(1),
            "for 1 remaining BAML future to finish (press Ctrl+C to cancel now)"
        );
        assert_eq!(
            wait_message(2),
            "for 2 remaining BAML futures to finish (press Ctrl+C to cancel now)"
        );
    }

    #[test]
    fn leak_message_groups_by_origin_most_frequent_first() {
        let leak = |origin: &str| bex_engine::LeakedFuture {
            origin: origin.into(),
        };
        let msg = leak_message(&[
            leak("user.b.serve"),
            leak("user.a.serve"),
            leak("user.b.serve"),
        ]);
        assert!(
            msg.starts_with(
                "abandoned 3 leaked background futures spawned in: user.b.serve x2, user.a.serve"
            ),
            "{msg}"
        );
        let one = leak_message(&[leak("user.a.serve")]);
        assert!(
            one.starts_with("abandoned 1 leaked background future spawned in: user.a.serve"),
            "{one}"
        );
    }
}
