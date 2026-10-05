use std::{collections::BTreeMap, sync::Arc, time::Duration};

use bex_engine::{BexEngine, ProcessStatus};

use crate::{optional_duration::OptionalDuration, reporter::Reporter};

/// Default grace for the end-of-run wait on in-flight calls and orphaned
/// background futures before they are cancelled and abandoned.
const DEFAULT_SHUTDOWN_GRACE: Duration = Duration::from_secs(15);

/// The `--shutdown-timeout` flag shared by `baml run` and `baml test`.
#[derive(clap::Args, Clone, Copy, Debug)]
pub(crate) struct ShutdownArgs {
    /// Wait at most this long at exit for in-flight calls and background futures.
    ///
    /// Accepts durations such as `500ms`, `15s` or `2m`; `none` waits
    /// forever. Ctrl+C still cancels immediately.
    #[arg(
        long = "shutdown-timeout",
        value_name = "DURATION",
        default_value = "15s",
        help_heading = "Shutdown options"
    )]
    pub shutdown_timeout: OptionalDuration,
}

impl Default for ShutdownArgs {
    fn default() -> Self {
        Self {
            shutdown_timeout: OptionalDuration(Some(DEFAULT_SHUTDOWN_GRACE)),
        }
    }
}

pub(crate) fn shutdown_engine(
    rt: &tokio::runtime::Runtime,
    engine: &Arc<BexEngine>,
    reporter: &Reporter,
    status: ProcessStatus,
    timeout: OptionalDuration,
) {
    rt.block_on(shutdown_engine_future(engine, reporter, status, timeout));
}

/// The CLI exits after this: the recording ends with the process's status.
pub(crate) async fn shutdown_engine_future(
    engine: &Arc<BexEngine>,
    reporter: &Reporter,
    status: ProcessStatus,
    timeout: OptionalDuration,
) {
    engine.record_process_exit(status);
    engine
        .shutdown_with_deadline(
            timeout.0,
            |count| {
                reporter.status("Waiting", wait_message(count));
            },
            |leaks| {
                if !leaks.is_empty() {
                    reporter.warning(leak_message(leaks));
                }
            },
        )
        .await;
    if engine.initial_cloud_authorization_error().is_none()
        && let Some(Err(error)) = engine.telemetry_result()
    {
        reporter.warning(format_args!("telemetry recording failed: {error}"));
    }
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
         test or call finished without cleaning them up (--shutdown-timeout \
         adjusts the wait; `none` waits forever)",
        leaks.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_timeout_defaults_to_fifteen_seconds() {
        assert_eq!(
            ShutdownArgs::default().shutdown_timeout,
            OptionalDuration(Some(Duration::from_secs(15)))
        );
    }

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
