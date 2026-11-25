//! Printing a running program's `log.*` events to stdio, for `baml run`,
//! `baml test` and packed binaries.
//!
//! This is only the stdio sink. The VM records every `log.*` event into
//! telemetry on its own, whatever the level here.

#![expect(clippy::print_stdout, clippy::print_stderr)]

use std::ffi::OsStr;

use baml_env::EnvError;
use bex_engine::{
    FunctionCallContext, FunctionCallContextBuilder,
    logger::{LogLevel, TraceLogger},
};
use clap::ValueEnum;

/// The environment variable that sets the filter, under `--log` for the CLI
/// commands. A packed binary, whose arguments are the target's, reads it alone,
/// under the name its publisher chose (see [`crate::PackLog`]).
pub const LOG_ENV: &str = "BAML_LOG";

/// The most verbose `log.*` level printed.
///
/// Variants are declared least to most verbose, so the derived order is the
/// filter: an event prints when its level's filter is at most this one.
///
/// No default: each host chooses its own (`info` for the CLI, `off` for a
/// packed program).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum LogFilter {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    /// The same as `Debug`: no event is more verbose. Accepted because it is
    /// the conventional most-verbose setting.
    Trace,
}

impl LogFilter {
    /// The filter the variable `name` sets; `None` when it is unset.
    pub fn from_env(name: &str) -> Result<Option<Self>, EnvError> {
        baml_env::os_var(name)
            .as_deref()
            .map(|value| Self::from_env_value(name, value))
            .transpose()
    }

    /// Parsed as clap parses `--log`: a name, in any case, and nothing else.
    /// A value that is not unicode can match no name.
    fn from_env_value(name: &str, value: &OsStr) -> Result<Self, EnvError> {
        value
            .to_str()
            .and_then(|value| <Self as ValueEnum>::from_str(value, true).ok())
            .ok_or_else(|| EnvError::InvalidChoice {
                name: name.to_owned(),
                expected: Self::names().join(", "),
                value: value.to_string_lossy().into_owned(),
            })
    }

    fn names() -> Vec<String> {
        Self::value_variants()
            .iter()
            .filter_map(ValueEnum::to_possible_value)
            .map(|value| value.get_name().to_owned())
            .collect()
    }

    pub fn allows(self, level: LogLevel) -> bool {
        Self::least_admitting(level) <= self
    }

    fn least_admitting(level: LogLevel) -> Self {
        match level {
            LogLevel::Error => Self::Error,
            LogLevel::Warn => Self::Warn,
            LogLevel::Info => Self::Info,
            LogLevel::Debug => Self::Debug,
        }
    }
}

fn label(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "ERROR",
        LogLevel::Warn => "WARN",
        LogLevel::Info => "INFO",
        LogLevel::Debug => "DEBUG",
    }
}

#[derive(Clone, Copy)]
pub struct LogOutput {
    filter: LogFilter,
    command: &'static str,
    stream: Stream,
}

#[derive(Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

impl LogOutput {
    /// Logs of a program whose own output is the command's stdout (`run`).
    pub fn passthrough(filter: LogFilter, command: &'static str) -> Self {
        Self {
            filter,
            command,
            stream: Stream::Stdout,
        }
    }

    /// Logs kept off stdout, which carries something else: a command's result
    /// (`test`), or a packed program's own output.
    pub fn status(filter: LogFilter, command: &'static str) -> Self {
        Self {
            filter,
            command,
            stream: Stream::Stderr,
        }
    }

    pub fn call_context(
        self,
        builder: FunctionCallContextBuilder,
    ) -> (FunctionCallContext, Option<TraceLogger>) {
        if self.filter == LogFilter::Off {
            return (builder.build(), None);
        }

        let logger = TraceLogger::bounded(100_000);
        let context = builder.with_logger(logger.clone()).build();
        (context, Some(logger))
    }

    pub fn print(self, logger: Option<&TraceLogger>) {
        let Some(logger) = logger else {
            return;
        };
        let report = logger.drain_rendered_logs();
        for log in report.logs {
            if self.filter.allows(log.metadata.level) {
                let level = label(log.metadata.level);
                match self.stream {
                    Stream::Stdout => println!("[{level}] {}", log.body),
                    Stream::Stderr => eprintln!("[{level}] {}", log.body),
                }
            }
        }
        for failure in report.failures {
            eprintln!(
                "WARN {} log capture failed: {}",
                self.command, failure.diagnostic
            );
        }
    }

    /// Run `future` to completion, printing logs as they arrive.
    // Native only, like `baml_exec`'s `tokio` dependency.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn block_on<T>(
        self,
        rt: &tokio::runtime::Runtime,
        future: impl std::future::Future<Output = T>,
        logger: Option<&TraceLogger>,
    ) -> T {
        let Some(logger) = logger else {
            return rt.block_on(future);
        };
        rt.block_on(async {
            tokio::pin!(future);
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(50));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    result = &mut future => {
                        self.print(Some(logger));
                        break result;
                    }
                    _ = interval.tick() => self.print(Some(logger)),
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use bex_engine::{CallId, FunctionCallContextBuilder, logger::LogLevel};

    use super::{LogFilter, LogOutput};

    #[test]
    fn each_filter_admits_its_level_and_less_verbose_ones() {
        use LogLevel::{Debug, Error, Info, Warn};
        let cases = [
            (LogFilter::Off, [false, false, false, false]),
            (LogFilter::Error, [true, false, false, false]),
            (LogFilter::Warn, [true, true, false, false]),
            (LogFilter::Info, [true, true, true, false]),
            (LogFilter::Debug, [true, true, true, true]),
            (LogFilter::Trace, [true, true, true, true]),
        ];
        for (filter, expected) in cases {
            let admitted = [Error, Warn, Info, Debug].map(|level| filter.allows(level));
            assert_eq!(admitted, expected, "{filter:?}");
        }
    }

    #[test]
    fn env_value_parses_like_the_log_flag() {
        let parse = |value: &str| LogFilter::from_env_value("BAML_LOG", OsStr::new(value));
        assert_eq!(parse("off").unwrap(), LogFilter::Off);
        assert_eq!(parse("ERROR").unwrap(), LogFilter::Error);
        assert_eq!(parse("Debug").unwrap(), LogFilter::Debug);
        assert_eq!(parse("trace").unwrap(), LogFilter::Trace);

        // `--log` rejects these too: no trimming, and empty is not unset.
        for value in ["loud", "", " info"] {
            assert_eq!(
                parse(value).unwrap_err().to_string(),
                format!(
                    "BAML_LOG must be one of off, error, warn, info, debug, trace, got {value:?}"
                ),
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_env_value_is_an_invalid_choice() {
        use std::os::unix::ffi::OsStrExt as _;

        let error = LogFilter::from_env_value("BAML_LOG", OsStr::from_bytes(b"\xff")).unwrap_err();
        assert!(
            matches!(error, baml_env::EnvError::InvalidChoice { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn call_context_creates_producer_only_when_enabled() {
        let (_, producer) = LogOutput::status(LogFilter::Off, "test")
            .call_context(FunctionCallContextBuilder::new(CallId::next()));
        assert!(producer.is_none());

        let (_, producer) = LogOutput::status(LogFilter::Info, "test")
            .call_context(FunctionCallContextBuilder::new(CallId::next()));
        assert!(producer.is_some());
    }
}
