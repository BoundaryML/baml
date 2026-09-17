#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{future::Future, io::Write as _, time::Duration};

use bex_engine::{
    FunctionCallContext, FunctionCallContextBuilder,
    logger::{TraceLogLevel, TraceLogger},
};
use clap::ValueEnum;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum LogLevel {
    #[default]
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    const fn trace_level(self) -> TraceLogLevel {
        match self {
            Self::Off => TraceLogLevel::Off,
            Self::Error => TraceLogLevel::Error,
            Self::Warn => TraceLogLevel::Warn,
            Self::Info => TraceLogLevel::Info,
            Self::Debug => TraceLogLevel::Debug,
            Self::Trace => TraceLogLevel::Trace,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct LogOutput {
    level: LogLevel,
    command: &'static str,
}

impl LogOutput {
    pub(crate) fn new(level: LogLevel, command: &'static str) -> Self {
        Self { level, command }
    }

    pub(crate) fn call_context(
        self,
        builder: FunctionCallContextBuilder,
    ) -> (FunctionCallContext, Option<TraceLogger>) {
        if self.level == LogLevel::Off {
            return (builder.build(), None);
        }

        let logger = TraceLogger::bounded_with_log_level(100_000, self.level.trace_level());
        let context = builder.with_logger(logger.clone()).build();
        (context, Some(logger))
    }

    pub(crate) fn print(self, logger: Option<&TraceLogger>) {
        let Some(logger) = logger else {
            return;
        };
        let report = logger.drain_rendered_logs();
        for log in report.logs {
            let level = log
                .metadata
                .level
                .as_deref()
                .unwrap_or("info")
                .to_ascii_uppercase();
            println!("[{level}] {}", log.body);
        }
        for failure in report.failures {
            eprintln!(
                "WARN {} log capture failed: {}",
                self.command, failure.diagnostic
            );
        }
        let _ = std::io::stdout().flush();
    }

    pub(crate) fn block_on<T>(
        self,
        rt: &tokio::runtime::Runtime,
        future: impl Future<Output = T>,
        logger: Option<&TraceLogger>,
    ) -> T {
        let Some(logger) = logger else {
            return rt.block_on(future);
        };
        rt.block_on(async {
            tokio::pin!(future);
            let mut interval = tokio::time::interval(Duration::from_millis(50));
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
    use bex_engine::{CallId, FunctionCallContextBuilder};

    use super::{LogLevel, LogOutput};

    #[test]
    fn uses_the_shared_engine_filter() {
        assert!(!LogLevel::Off.trace_level().allows(Some("error")));
        assert!(LogLevel::Error.trace_level().allows(Some("error")));
        assert!(!LogLevel::Error.trace_level().allows(Some("warn")));
        assert!(LogLevel::Info.trace_level().allows(Some("error")));
        assert!(LogLevel::Info.trace_level().allows(Some("warning")));
        assert!(LogLevel::Info.trace_level().allows(Some("info")));
        assert!(LogLevel::Info.trace_level().allows(None));
        assert!(!LogLevel::Info.trace_level().allows(Some("debug")));
        assert!(LogLevel::Debug.trace_level().allows(Some("debug")));
        assert!(LogLevel::Trace.trace_level().allows(Some("debug")));
    }

    #[test]
    fn call_context_creates_producer_only_when_enabled() {
        let (_, producer) = LogOutput::new(LogLevel::Off, "test")
            .call_context(FunctionCallContextBuilder::new(CallId::next()));
        assert!(producer.is_none());

        let (_, producer) = LogOutput::new(LogLevel::Info, "test")
            .call_context(FunctionCallContextBuilder::new(CallId::next()));
        assert!(producer.is_some());
    }
}
