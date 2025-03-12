use colored::*;
use lazy_static::lazy_static;
use serde::Serialize;
use std::env;
use std::io::{self, Write};
use std::sync::{Once, RwLock};
use thiserror::Error;

/// Static initialization guard
static INIT: Once = Once::new();

const DEFAULT_LOG_LEVEL: Level = Level::Info;

/// Logging levels in order of verbosity
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Disable all logging
    Off = 0,
    /// Critical errors that prevent program execution
    Error = 1,
    /// Concerning but non-fatal errors
    Warn = 2,
    /// General information about program execution
    Info = 3,
    /// Detailed information useful for debugging
    Debug = 4,
    /// Very detailed tracing information
    Trace = 5,
}

impl Level {
    /// Parse a level from a string
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "off" => Level::Off,
            "error" => Level::Error,
            "warn" => Level::Warn,
            "info" => Level::Info,
            "debug" => Level::Debug,
            "trace" => Level::Trace,
            _ => {
                bwarn!(
                    "Invalid BAML_LOG level: {}. Defaulting to {}",
                    s,
                    DEFAULT_LOG_LEVEL.colored()
                );
                DEFAULT_LOG_LEVEL // Default
            }
        }
    }

    /// Convert level to a human-readable string
    pub fn as_str(&self) -> &'static str {
        match self {
            Level::Off => "OFF",
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        }
    }

    /// Get a colored version of the level string
    fn colored(&self) -> ColoredString {
        match self {
            Level::Off => "OFF".normal(),
            Level::Error => "ERROR".bright_red(),
            Level::Warn => "WARN".yellow(),
            Level::Info => "INFO".bright_green(),
            Level::Debug => "DEBUG".cyan(),
            Level::Trace => "TRACE".normal(),
        }
    }
}

/// Style configuration for terminal output
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// Auto-detect terminal capabilities
    Auto,
    /// Always use colors
    Always,
    /// Never use colors
    Never,
}

impl ColorMode {
    /// Parse color mode from a string
    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "auto" => ColorMode::Auto,
            "always" => ColorMode::Always,
            "never" => ColorMode::Never,
            _ => ColorMode::Auto,
        }
    }
}

/// Configuration for the logger
struct LogConfig {
    /// Current log level
    level: Level,
    /// Whether to use JSON formatting
    use_json: bool,
    /// Color output mode
    color_mode: ColorMode,
    /// Whether initialization has completed
    initialized: bool,
}

lazy_static! {
    /// Thread-safe configuration with runtime modification support
    static ref CONFIG: RwLock<LogConfig> = RwLock::new({
        LogConfig {
            level: parse_level_from_env(),
            use_json: parse_json_from_env(),
            color_mode: parse_color_from_env(),
            initialized: false,
        }
    });
}

/// Parse log level from BAML_LOG environment variable
fn parse_level_from_env() -> Level {
    match env::var("BAML_LOG") {
        Ok(val) => Level::from_str(&val),
        Err(_) => DEFAULT_LOG_LEVEL,
    }
}

/// Parse JSON mode from BAML_LOG_JSON environment variable
fn parse_json_from_env() -> bool {
    match env::var("BAML_LOG_JSON") {
        Ok(val) => val.trim().eq_ignore_ascii_case("true") || val.trim() == "1",
        Err(_) => false,
    }
}

/// Parse color mode from BAML_LOG_STYLE environment variable
fn parse_color_from_env() -> ColorMode {
    match env::var("BAML_LOG_STYLE") {
        Ok(val) => ColorMode::from_str(&val),
        Err(_) => ColorMode::Auto,
    }
}

/// JSON-serializable log entry
#[derive(Serialize)]
struct LogEntry<'a> {
    /// Timestamp in ISO 8601 format
    timestamp: String,
    /// Log level as a string
    level: &'a str,
    /// Log message
    message: String,
    /// Optional module path
    #[serde(skip_serializing_if = "Option::is_none")]
    module_path: Option<&'a str>,
    /// Optional file name
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<&'a str>,
    /// Optional line number
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<u32>,
}

/// Error type for logging operations
#[derive(Debug, Error)]
pub enum LogError {
    /// Error writing to output
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// Error serializing to JSON
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// Error acquiring lock
    #[error("Failed to acquire lock")]
    LockError,

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),
}

/// Logger instance that can be customized
pub struct Logger {
    level: Level,
    use_json: bool,
    color_mode: ColorMode,
}

/// Builder for creating a custom logger instance
pub struct LoggerBuilder {
    level: Option<Level>,
    use_json: Option<bool>,
    color_mode: Option<ColorMode>,
}

impl LoggerBuilder {
    /// Create a new logger builder with default settings
    pub fn new() -> Self {
        Self {
            level: None,
            use_json: None,
            color_mode: None,
        }
    }

    /// Set the log level
    pub fn level(mut self, level: Level) -> Self {
        self.level = Some(level);
        self
    }

    /// Set the JSON formatting mode
    pub fn json(mut self, enable: bool) -> Self {
        self.use_json = Some(enable);
        self
    }

    /// Set the color mode
    pub fn color_mode(mut self, mode: ColorMode) -> Self {
        self.color_mode = Some(mode);
        self
    }

    /// Build a logger instance
    pub fn build(self) -> Logger {
        Logger {
            level: self.level.unwrap_or_else(parse_level_from_env),
            use_json: self.use_json.unwrap_or_else(parse_json_from_env),
            color_mode: self.color_mode.unwrap_or_else(parse_color_from_env),
        }
    }
}

impl Logger {
    /// Log a message at the specified level
    pub fn log(
        &self,
        level: Level,
        message: &str,
        module_path: Option<&str>,
        file: Option<&str>,
        line: Option<u32>,
    ) -> Result<(), LogError> {
        if level as usize > self.level as usize {
            return Ok(());
        }

        let level_str = level.as_str();
        let now = chrono::Local::now()
            .format("%Y-%m-%dT%H:%M:%S%.3f")
            .to_string();

        if self.use_json {
            let entry = LogEntry {
                timestamp: now,
                level: level_str,
                message: message.to_string(),
                module_path,
                file,
                line,
            };

            let json = serde_json::to_string(&entry)?;
            writeln!(io::stdout(), "{}", json)?;
        } else {
            // Configure color control based on mode
            match self.color_mode {
                ColorMode::Always => control::set_override(true),
                ColorMode::Never => control::set_override(false),
                ColorMode::Auto => {} // Use default detection
            }

            let location = if let (Some(file), Some(line)) = (file, line) {
                format!(" [{}:{}]", file, line)
            } else {
                String::new()
            };

            let module = if let Some(module) = module_path {
                format!(" ({})", module)
            } else {
                String::new()
            };

            writeln!(
                io::stdout(),
                "[{}] {}{}{}: {}",
                now,
                level.colored(),
                location,
                module,
                message
            )?;
        }

        Ok(())
    }
}

/// Initialize the logger
///
/// This function should be called at the start of your program.
/// It reads configuration from environment variables and sets up the logger.
pub fn init() -> Result<(), LogError> {
    let mut result = Ok(());

    INIT.call_once(|| {
        // Configure color mode
        if let Ok(mut config) = CONFIG.write() {
            // Update the initialized flag
            config.initialized = true;
        } else {
            result = Err(LogError::LockError);
        }
    });

    result
}

/// Set the log level at runtime
pub fn set_log_level(level: Level) -> Result<(), LogError> {
    match CONFIG.write() {
        Ok(mut config) => {
            let old_level = config.level;
            config.level = level;
            if old_level != level {
                println!("[BAML] Log level set to {}", level.colored());
            }
            Ok(())
        }
        Err(_) => Err(LogError::LockError),
    }
}

/// Set the JSON formatting mode at runtime
pub fn set_json_mode(enable: bool) -> Result<(), LogError> {
    match CONFIG.write() {
        Ok(mut config) => {
            config.use_json = enable;
            Ok(())
        }
        Err(_) => Err(LogError::LockError),
    }
}

/// Set the color mode at runtime
pub fn set_color_mode(mode: ColorMode) -> Result<(), LogError> {
    match CONFIG.write() {
        Ok(mut config) => {
            config.color_mode = mode;
            Ok(())
        }
        Err(_) => Err(LogError::LockError),
    }
}

pub fn set_from_env(env_vars: &std::collections::HashMap<String, String>) -> Result<(), LogError> {
    if let Some(level) = env_vars
        .get("BAML_LOG")
        .map(|s| Level::from_str(s.as_str()))
    {
        set_log_level(level)?;
    }
    if let Some(use_json) = env_vars.get("BAML_LOG_JSON") {
        set_json_mode(use_json.trim().eq_ignore_ascii_case("true") || use_json.trim() == "1")?;
    }
    if let Some(color_mode) = env_vars
        .get("BAML_LOG_STYLE")
        .map(|s| ColorMode::from_str(s))
    {
        set_color_mode(color_mode)?;
    }
    Ok(())
}

/// Reload configuration from environment variables
pub fn reload_from_env() -> Result<(), LogError> {
    match CONFIG.write() {
        Ok(mut config) => {
            config.level = parse_level_from_env();
            config.use_json = parse_json_from_env();
            config.color_mode = parse_color_from_env();
            Ok(())
        }
        Err(_) => Err(LogError::LockError),
    }
}

/// Internal function used by logging macros
pub fn log_internal(
    level: Level,
    message: &str,
    module_path: Option<&str>,
    file: Option<&str>,
    line: Option<u32>,
) {
    // Ensure the logger is initialized
    let _ = INIT.call_once(|| {
        if let Ok(mut config) = CONFIG.write() {
            config.initialized = true;
        }
    });

    // Create a temporary logger with the current config
    let logger = match CONFIG.read() {
        Ok(config) => Logger {
            level: config.level,
            use_json: config.use_json,
            color_mode: config.color_mode,
        },
        Err(_) => return, // Can't get config, skip logging
    };

    // Log the message
    let _ = logger.log(level, message, module_path, file, line);
}

pub trait Loggable {
    fn as_baml_log_string(&self) -> String;
    fn as_baml_log_json(&self) -> Result<serde_json::Value, LogError>;
}

/// Internal function used by event logging macros
pub fn log_event_internal<T: Loggable>(
    level: Level,
    payload: &T,
    module_path: Option<&str>,
    file: Option<&str>,
    line: Option<u32>,
) {
    // Ensure the logger is initialized
    let _ = INIT.call_once(|| {
        if let Ok(mut config) = CONFIG.write() {
            config.initialized = true;
        }
    });

    // Create a temporary logger with the current config
    let config = match CONFIG.read() {
        Ok(config) => config,
        Err(_) => return, // Can't get config, skip logging
    };

    // Skip if level is not enabled
    if level as usize > config.level as usize {
        return;
    }

    let level_str = level.as_str();
    let now = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%.3f")
        .to_string();

    if config.use_json {
        // In JSON mode, use the payload directly
        if let Ok(json_value) = payload.as_baml_log_json() {
            let mut event_json = serde_json::Map::new();
            event_json.insert("timestamp".to_string(), serde_json::Value::String(now));
            event_json.insert(
                "level".to_string(),
                serde_json::Value::String(level_str.to_string()),
            );

            // Add all payload fields to the event
            if let serde_json::Value::Object(payload_map) = json_value {
                for (key, value) in payload_map {
                    event_json.insert(key, value);
                }
            } else {
                event_json.insert("payload".to_string(), json_value);
            }

            let json_str = serde_json::to_string(&event_json).unwrap_or_default();
            let _ = writeln!(io::stdout(), "{}", json_str);
        }
    } else {
        // In regular mode, convert payload to a debug string
        let payload_str = payload.as_baml_log_string();
        // multi-line payloads should be indented
        let payload_str = if payload_str.contains("\n") {
            payload_str
                .lines()
                .map(|line| format!("    {}", line))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            payload_str
        };

        // Configure color control based on mode
        match config.color_mode {
            ColorMode::Always => control::set_override(true),
            ColorMode::Never => control::set_override(false),
            ColorMode::Auto => {} // Use default detection
        }

        let _ = writeln!(
            io::stdout(),
            "{} [BAML 🐑 {}] {}",
            now,
            level.colored(),
            payload_str.trim()
        );
    }
}
