#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{
    env::VarError,
    fs::File,
    future::Future,
    io::{self, BufWriter, Write as _},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use bex_engine::{
    FunctionCallContext, FunctionCallContextBuilder,
    logger::{TraceLogLevel, TraceLogger},
};
use clap::ValueEnum;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum LogLevel {
    #[default]
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    #[must_use]
    pub fn allows(self, event_level: Option<&str>) -> bool {
        self.trace_level().allows(event_level)
    }

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

    /// Read the CLI-compatible log threshold from `BAML_LOG`.
    ///
    /// Packed binaries do not have a top-level clap parser, so they use this
    /// helper to preserve the same case-insensitive values as `baml run`.
    pub fn from_env() -> Result<Self, String> {
        let value = match std::env::var("BAML_LOG") {
            Ok(value) => value,
            Err(VarError::NotPresent) => return Ok(Self::Off),
            Err(VarError::NotUnicode(_)) => {
                return Err("BAML_LOG must contain valid Unicode".to_string());
            }
        };
        Self::from_str(&value, true).map_err(|_| {
            format!(
                "invalid BAML_LOG value `{value}`; expected one of: off, error, warn, info, debug, trace"
            )
        })
    }
}

struct LogFile {
    path: PathBuf,
    writer: Option<BufWriter<File>>,
}

#[derive(Clone)]
pub struct LogOutput {
    terminal_level: LogLevel,
    file_level: LogLevel,
    file: Option<Arc<Mutex<LogFile>>>,
    command: &'static str,
}

impl LogOutput {
    #[must_use]
    pub fn new(level: LogLevel, command: &'static str) -> Self {
        Self {
            terminal_level: level,
            file_level: LogLevel::Off,
            file: None,
            command,
        }
    }

    /// Add a file sink to the terminal configuration.
    ///
    /// A file by itself captures every structured BAML log. When terminal
    /// logging has a threshold, the file uses that same threshold.
    pub fn with_file(level: LogLevel, command: &'static str, path: &Path) -> io::Result<Self> {
        let file = File::create(path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("failed to create log file {}: {error}", path.display()),
            )
        })?;
        Ok(Self {
            terminal_level: level,
            file_level: if level == LogLevel::Off {
                LogLevel::Trace
            } else {
                level
            },
            file: Some(Arc::new(Mutex::new(LogFile {
                path: path.to_path_buf(),
                writer: Some(BufWriter::new(file)),
            }))),
            command,
        })
    }

    pub fn call_context(
        &self,
        builder: FunctionCallContextBuilder,
    ) -> (FunctionCallContext, Option<TraceLogger>) {
        if self.terminal_level == LogLevel::Off && self.file.is_none() {
            return (builder.build(), None);
        }

        let capture_level = if self.file.is_some() {
            self.file_level
        } else {
            self.terminal_level
        };
        let logger = TraceLogger::bounded_with_log_level(100_000, capture_level.trace_level());
        let context = builder.with_logger(logger.clone()).build();
        (context, Some(logger))
    }

    pub fn print(&self, logger: Option<&TraceLogger>) {
        let Some(logger) = logger else {
            return;
        };
        let report = logger.drain_rendered_logs();
        let mut file_lines = Vec::new();
        let mut printed_to_terminal = false;
        for log in report.logs {
            let event_level = log.metadata.level.as_deref();
            let level = event_level.unwrap_or("info").to_ascii_uppercase();
            let line = format!("[{level}] {}", log.body);
            if self.terminal_level.allows(event_level) {
                println!("{line}");
                printed_to_terminal = true;
            }
            if self.file_level.allows(event_level) {
                file_lines.push(line);
            }
        }
        for failure in report.failures {
            let line = format!(
                "WARN {} log capture failed: {}",
                self.command, failure.diagnostic
            );
            eprintln!("{line}");
            file_lines.push(line);
        }
        if printed_to_terminal {
            let _ = std::io::stdout().flush();
        }
        self.write_file_lines(&file_lines);
    }

    fn write_file_lines(&self, lines: &[String]) {
        if lines.is_empty() {
            return;
        }
        let Some(file) = &self.file else {
            return;
        };
        let mut file = file
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let path = file.path.clone();
        let Some(writer) = file.writer.as_mut() else {
            return;
        };
        let result = lines
            .iter()
            .try_for_each(|line| writeln!(writer, "{line}"))
            .and_then(|()| writer.flush());
        if let Err(error) = result {
            eprintln!(
                "WARN {} log file {} failed: {error}",
                self.command,
                path.display()
            );
            file.writer = None;
        }
    }

    pub fn block_on<T>(
        &self,
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
    fn filters_at_or_above_threshold() {
        assert!(!LogLevel::Off.allows(Some("error")));
        assert!(LogLevel::Error.allows(Some("error")));
        assert!(!LogLevel::Error.allows(Some("warn")));
        assert!(LogLevel::Info.allows(Some("error")));
        assert!(LogLevel::Info.allows(Some("warning")));
        assert!(LogLevel::Info.allows(Some("info")));
        assert!(LogLevel::Info.allows(None));
        assert!(!LogLevel::Info.allows(Some("debug")));
        assert!(LogLevel::Debug.allows(Some("debug")));
        assert!(LogLevel::Trace.allows(Some("debug")));
    }

    #[test]
    fn call_context_creates_producer_only_when_enabled() {
        let (_, producer) = LogOutput::new(LogLevel::Off, "test")
            .call_context(FunctionCallContextBuilder::new(CallId::next()));
        assert!(producer.is_none());

        let (_, producer) = LogOutput::new(LogLevel::Info, "test")
            .call_context(FunctionCallContextBuilder::new(CallId::next()));
        let producer = producer.expect("enabled terminal logging");
        assert!(producer.captures_log_level(Some("info")));
        assert!(!producer.captures_log_level(Some("debug")));
    }

    #[test]
    fn file_sink_enables_capture_when_terminal_is_off() {
        let tmp = tempfile::tempdir().unwrap();
        let output =
            LogOutput::with_file(LogLevel::Off, "test", &tmp.path().join("run.log")).unwrap();
        let (_, producer) = output.call_context(FunctionCallContextBuilder::new(CallId::next()));
        let producer = producer.expect("enabled file logging");
        assert!(producer.captures_log_level(Some("debug")));
        assert!(producer.captures_log_level(Some("trace")));
    }
}
