//! Delivery of BAML `log.*` events for native SDK calls.
//!
//! The engine deliberately captures structured logs only when a call boundary
//! opts in. Native SDK bridges share this module so Go, Python, TypeScript, and
//! Java calls all use the same `BAML_LOG` threshold and stderr format.

use std::future::Future;

use bex_project::{FunctionCallContext, FunctionCallContextBuilder};

#[cfg(not(target_arch = "wasm32"))]
use std::{
    io::{self, Write},
    time::Duration,
};

#[cfg(not(target_arch = "wasm32"))]
use bex_project::{
    CaptureDefaults, RenderedTraceLog, TraceCaptureConfig, TraceCaptureProducer,
    TraceLogDrainReport,
};

#[cfg(not(target_arch = "wasm32"))]
const MAX_PENDING_LOGS: usize = 100_000;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Off,
}

#[cfg(not(target_arch = "wasm32"))]
impl LogLevel {
    fn parse(raw: Option<&str>) -> Self {
        let Some(raw) = raw else {
            return Self::Off;
        };
        match raw.trim().to_ascii_uppercase().as_str() {
            "OFF" => Self::Off,
            "ERROR" => Self::Error,
            "WARN" | "WARNING" => Self::Warn,
            "DEBUG" | "TRACE" => Self::Debug,
            "" | "INFO" => Self::Info,
            _ => Self::Info,
        }
    }

    fn allows(self, event_level: Option<&str>) -> bool {
        if self == Self::Off {
            return false;
        }
        Self::parse_event(event_level) <= self
    }

    fn parse_event(raw: Option<&str>) -> Self {
        match raw.unwrap_or("info").to_ascii_lowercase().as_str() {
            "error" => Self::Error,
            "warn" | "warning" => Self::Warn,
            "debug" | "trace" => Self::Debug,
            _ => Self::Info,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn configured_level() -> LogLevel {
    LogLevel::parse(std::env::var("BAML_LOG").ok().as_deref())
}

/// Opt native SDK calls into the engine's structured-log capture when
/// `BAML_LOG` is set to a level other than `OFF`. Web SDKs do not have a
/// process stderr and retain their existing browser-specific path.
pub(crate) fn configure_call_context(
    builder: FunctionCallContextBuilder,
) -> FunctionCallContextBuilder {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if configured_level() == LogLevel::Off {
            return builder;
        }
        let producer = TraceCaptureProducer::new(TraceCaptureConfig::logs_only(MAX_PENDING_LOGS));
        builder
            .with_capture_defaults(CaptureDefaults {
                values_enabled: false,
                logs_enabled: true,
            })
            .with_value_capture(producer)
    }

    #[cfg(target_arch = "wasm32")]
    builder
}

pub(crate) struct SdkLogCapture {
    #[cfg(not(target_arch = "wasm32"))]
    producer: TraceCaptureProducer,
    #[cfg(not(target_arch = "wasm32"))]
    level: LogLevel,
}

impl SdkLogCapture {
    pub(crate) fn from_call_context(context: &FunctionCallContext) -> Option<Self> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if !context.boundary.capture_defaults.logs_enabled {
                return None;
            }
            Some(Self {
                producer: context.value_capture.clone(),
                level: configured_level(),
            })
        }

        #[cfg(target_arch = "wasm32")]
        {
            let _ = context;
            None
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn drain_to_stderr(&self) {
        let stderr = io::stderr();
        let mut stderr = stderr.lock();
        write_report(self.level, self.producer.drain_rendered_logs(), &mut stderr);
        // Ignore broken stderr: there is no safer diagnostic channel here.
        let _ = stderr.flush();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_after_root(self) {
        self.drain_to_stderr();
        if !self.producer.has_other_handles() {
            // Observing the last producer is stable: without another handle,
            // nobody can clone one. Drain once more to close the race with a
            // producer that published immediately before dropping its handle.
            self.drain_to_stderr();
            return;
        }

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(50));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                self.drain_to_stderr();
                if !self.producer.has_other_handles() {
                    self.drain_to_stderr();
                    return;
                }
            }
        });
    }
}

/// Poll while the SDK call is running so long-running BAML functions expose
/// logs promptly, then perform a final drain before the host receives its
/// result. The engine future is already converted to a panic-bearing Result by
/// the caller, so the final drain also runs after a BAML runtime panic.
pub(crate) async fn run_with_log_capture<F>(capture: Option<SdkLogCapture>, future: F) -> F::Output
where
    F: Future,
{
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(capture) = capture else {
            return future.await;
        };
        let result = {
            tokio::pin!(future);
            let mut interval = tokio::time::interval(Duration::from_millis(50));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    result = &mut future => break result,
                    _ = interval.tick() => capture.drain_to_stderr(),
                }
            }
        };
        // The root future is dropped before checking producer ownership. Any
        // remaining handles belong to spawned children, including detached
        // children that are allowed to outlive the root call.
        capture.finish_after_root();
        result
    }

    #[cfg(target_arch = "wasm32")]
    {
        let _ = capture;
        future.await
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn write_report(writer_level: LogLevel, report: TraceLogDrainReport, writer: &mut impl Write) {
    for log in report.logs {
        if writer_level.allows(log.metadata.level.as_deref()) {
            write_log(log, writer);
        }
    }
    for failure in report.failures {
        let _ = writeln!(
            writer,
            "[BAML WARN] Failed to render BAML log event: {}",
            failure.diagnostic
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn write_log(log: RenderedTraceLog, writer: &mut impl Write) {
    let level = log
        .metadata
        .level
        .as_deref()
        .unwrap_or("info")
        .to_ascii_uppercase();
    let _ = writeln!(writer, "[BAML {level}] {}", log.body);
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn log_level_requires_the_env_var_and_accepts_aliases() {
        assert_eq!(LogLevel::parse(None), LogLevel::Off);
        assert_eq!(LogLevel::parse(Some("")), LogLevel::Info);
        assert_eq!(LogLevel::parse(Some(" warning ")), LogLevel::Warn);
        assert_eq!(LogLevel::parse(Some("TRACE")), LogLevel::Debug);
        assert_eq!(LogLevel::parse(Some("not-a-level")), LogLevel::Info);
    }

    #[test]
    fn log_level_filters_less_severe_events() {
        assert!(LogLevel::Info.allows(Some("error")));
        assert!(LogLevel::Info.allows(Some("warn")));
        assert!(LogLevel::Info.allows(Some("info")));
        assert!(!LogLevel::Info.allows(Some("debug")));
        assert!(!LogLevel::Off.allows(Some("error")));
        assert!(LogLevel::Debug.allows(Some("debug")));
    }

    #[test]
    fn rendered_log_uses_the_sdk_stderr_shape() {
        let mut output = Vec::new();
        write_log(
            RenderedTraceLog {
                metadata: bex_project::TraceLogMetadata {
                    level: Some("warn".to_string()),
                    source: None,
                    timestamp_ms: 0,
                    message_preview: None,
                },
                body: r#"{"attempt": 2}"#.to_string(),
            },
            &mut output,
        );
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "[BAML WARN] {\"attempt\": 2}\n"
        );
    }
}
