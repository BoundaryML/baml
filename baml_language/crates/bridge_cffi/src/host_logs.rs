//! Env-var-gated delivery of BAML structured logs to the host process.
//!
//! `baml-cli test --logs INFO` captures `$baml_log` events (the `log.info`,
//! `log.debug`, `log.warn`, and `log.error` builtins) and prints them while a
//! test runs. SDK bridges have no equivalent flag, so by default every log
//! event is dropped at the call boundary. This module gives every host SDK the
//! same behavior as the CLI flag through the `BAML_LOG` environment variable:
//! when it names a level (`error`, `warn`, `info`, or `debug`), each function
//! call made through [`crate::call_and_encode`] or
//! [`crate::call_handle_and_encode`] captures its log events and writes them to
//! stderr as `[LEVEL] body` lines, draining periodically so long-running calls
//! stream their logs live.
//!
//! `BAML_LOG` is read at the start of each function call, so a host process may
//! enable or disable log delivery between calls. Unset, empty, and `off` leave
//! log capture disabled. An unrecognized value warns once and leaves capture
//! disabled.

use std::{io::Write as _, time::Duration};

use bex_project::{CaptureDefaults, FunctionCallContext, TraceCaptureConfig, TraceCaptureProducer};

/// Environment variable that opts a host process into BAML log delivery.
pub const BAML_LOG_ENV_VAR: &str = "BAML_LOG";

/// Upper bound on captured-but-undrained log events per call. Matches the
/// budget `baml-cli test --logs` uses; the periodic drain keeps the queue far
/// below this in practice.
const MAX_PENDING_LOGS: usize = 100_000;

/// How often an in-flight call flushes captured logs to stderr.
const DRAIN_INTERVAL: Duration = Duration::from_millis(50);

/// Host-selected minimum log level, parsed from `BAML_LOG`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostLogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl HostLogLevel {
    /// Parse a `BAML_LOG` value. `Ok(None)` means logs stay off; `Err` means
    /// the value is unrecognized.
    fn parse(raw: &str) -> Result<Option<Self>, ()> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "off" | "none" | "0" | "false" => Ok(None),
            "error" => Ok(Some(Self::Error)),
            "warn" | "warning" => Ok(Some(Self::Warn)),
            "info" => Ok(Some(Self::Info)),
            "debug" => Ok(Some(Self::Debug)),
            _ => Err(()),
        }
    }

    /// Whether an event at `event_level` passes this threshold. Unknown and
    /// absent event levels rank as `info`, mirroring `baml-cli test --logs`.
    fn allows(self, event_level: Option<&str>) -> bool {
        let threshold = match self {
            Self::Error => 1,
            Self::Warn => 2,
            Self::Info => 3,
            Self::Debug => 4,
        };
        let event = match event_level.unwrap_or("info").to_ascii_lowercase().as_str() {
            "error" => 1,
            "warn" | "warning" => 2,
            "info" => 3,
            "debug" => 4,
            _ => 3,
        };
        event <= threshold
    }
}

fn env_level() -> Option<HostLogLevel> {
    let raw = std::env::var(BAML_LOG_ENV_VAR).unwrap_or_default();
    match HostLogLevel::parse(&raw) {
        Ok(level) => level,
        Err(()) => {
            static WARN_ONCE: std::sync::Once = std::sync::Once::new();
            WARN_ONCE.call_once(|| {
                eprintln!(
                    "WARN {BAML_LOG_ENV_VAR}={raw:?} is not a recognized log level \
                     (expected off, error, warn, info, or debug); BAML logs stay off"
                );
            });
            None
        }
    }
}

/// A per-call stderr log sink: the capture producer this call publishes into
/// plus the host-selected level filter.
pub(crate) struct HostLogSink {
    producer: TraceCaptureProducer,
    level: HostLogLevel,
}

impl HostLogSink {
    /// Drain every captured log event, writing lines that pass the level
    /// filter to stderr.
    fn drain(&self) {
        let report = self.producer.drain_rendered_logs();
        if report.logs.is_empty() && report.failures.is_empty() {
            return;
        }
        let mut stderr = std::io::stderr().lock();
        for log in report.logs {
            if self.level.allows(log.metadata.level.as_deref()) {
                let level = log
                    .metadata
                    .level
                    .as_deref()
                    .unwrap_or("info")
                    .to_ascii_uppercase();
                let _ = writeln!(stderr, "[{level}] {}", log.body);
            }
        }
        for failure in report.failures {
            let _ = writeln!(
                stderr,
                "WARN BAML log capture failed: {}",
                failure.diagnostic
            );
        }
    }
}

/// Attach a stderr log sink to a call context when `BAML_LOG` requests one.
///
/// Leaves the context untouched when logs are off or when the caller already
/// configured its own capture (a non-default `CaptureDefaults` means another
/// owner is draining this call's producer).
pub(crate) fn attach_env_log_sink(
    mut ctx: FunctionCallContext,
) -> (FunctionCallContext, Option<HostLogSink>) {
    if ctx.boundary.capture_defaults != CaptureDefaults::disabled() {
        return (ctx, None);
    }
    let Some(level) = env_level() else {
        return (ctx, None);
    };
    let producer = TraceCaptureProducer::new(TraceCaptureConfig::logs_only(MAX_PENDING_LOGS));
    ctx.boundary.capture_defaults = CaptureDefaults {
        values_enabled: false,
        logs_enabled: true,
    };
    ctx.value_capture = producer.clone();
    (ctx, Some(HostLogSink { producer, level }))
}

/// Await `future`, draining the sink to stderr on an interval while it runs
/// and once more after it completes so every log line lands before the call's
/// result is delivered to the host.
pub(crate) async fn drive_with_log_drain<T>(
    future: impl Future<Output = T>,
    sink: Option<&HostLogSink>,
) -> T {
    let Some(sink) = sink else {
        return future.await;
    };
    tokio::pin!(future);
    let mut interval = tokio::time::interval(DRAIN_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            result = &mut future => {
                sink.drain();
                break result;
            }
            _ = interval.tick() => sink.drain(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_levels_case_insensitively() {
        assert_eq!(HostLogLevel::parse("INFO"), Ok(Some(HostLogLevel::Info)));
        assert_eq!(
            HostLogLevel::parse(" debug "),
            Ok(Some(HostLogLevel::Debug))
        );
        assert_eq!(HostLogLevel::parse("Warning"), Ok(Some(HostLogLevel::Warn)));
        assert_eq!(HostLogLevel::parse("error"), Ok(Some(HostLogLevel::Error)));
    }

    #[test]
    fn parse_treats_unset_shapes_as_off() {
        for raw in ["", "off", "none", "0", "false", "OFF"] {
            assert_eq!(HostLogLevel::parse(raw), Ok(None), "raw={raw:?}");
        }
    }

    #[test]
    fn parse_rejects_unknown_values() {
        assert_eq!(HostLogLevel::parse("trace"), Err(()));
        assert_eq!(HostLogLevel::parse("2"), Err(()));
    }

    #[test]
    fn allows_matches_cli_threshold_semantics() {
        assert!(HostLogLevel::Info.allows(Some("error")));
        assert!(HostLogLevel::Info.allows(Some("info")));
        assert!(HostLogLevel::Info.allows(None));
        assert!(!HostLogLevel::Info.allows(Some("debug")));
        assert!(!HostLogLevel::Error.allows(Some("warn")));
        assert!(HostLogLevel::Debug.allows(Some("debug")));
        // Unknown event levels rank as info.
        assert!(HostLogLevel::Info.allows(Some("verbose")));
        assert!(!HostLogLevel::Warn.allows(Some("verbose")));
    }

    #[test]
    fn attach_respects_caller_owned_capture() {
        let ctx = bex_project::FunctionCallContextBuilder::new(sys_types::CallId(1))
            .with_capture_defaults(CaptureDefaults {
                values_enabled: false,
                logs_enabled: true,
            })
            .build();
        // Even with BAML_LOG set in the environment, a context whose capture
        // defaults are already configured is returned unchanged.
        let (ctx, sink) = attach_env_log_sink(ctx);
        assert!(sink.is_none());
        assert!(ctx.boundary.capture_defaults.logs_enabled);
    }
}
