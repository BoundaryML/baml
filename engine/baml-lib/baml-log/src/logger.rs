use std::{
    collections::{HashMap, HashSet},
    env,
    fmt::{self, Display},
    io::{self, Write},
    str::FromStr,
    sync::{Once, RwLock},
};

use colored::*;
use lazy_static::lazy_static;
use thiserror::Error;

/// Static initialization guard
static INIT: Once = Once::new();

/// Default values for configuration
mod defaults {
    use super::*;
    pub const LOG_LEVEL: Level = Level::Info;
    pub const MAX_MESSAGE_LENGTH: usize = 64_000;
    pub const USE_JSON: bool = false;
    pub const COLOR_MODE: ColorMode = ColorMode::Auto;
    pub const RUNNING_IN_LSP: bool = false;
}

/// Logging levels in order of verbosity
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Fatal errors that prevent program execution
    Fatal = 0,
    /// Disable all logging
    Off = 1,
    /// Critical errors that prevent program execution
    Error = 2,
    /// Concerning but non-fatal errors
    Warn = 3,
    /// General information about program execution
    Info = 4,
    /// Detailed information useful for debugging
    Debug = 5,
    /// Very detailed tracing information
    Trace = 6,
}

impl Level {
    /// Convert level to a human-readable string
    pub fn as_str(&self) -> &'static str {
        match self {
            Level::Fatal => "FATAL",
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
            Level::Fatal => "FATAL".bright_red(),
            Level::Off => "OFF".normal(),
            Level::Error => "ERROR".bright_red(),
            Level::Warn => "WARN".yellow(),
            Level::Info => "INFO".bright_green(),
            Level::Debug => "DEBUG".cyan(),
            Level::Trace => "TRACE".normal(),
        }
    }

    fn is_at_least(&self, level: Level) -> bool {
        self >= &level
    }
}

impl Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for Level {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "off" => Ok(Level::Off),
            "error" => Ok(Level::Error),
            "warn" => Ok(Level::Warn),
            "info" => Ok(Level::Info),
            "debug" => Ok(Level::Debug),
            "trace" => Ok(Level::Trace),
            _ => {
                // Instead of using bwarn! macro here (which would cause circular dependency),
                // we just return default for parse failures
                Ok(defaults::LOG_LEVEL)
            }
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

impl Display for ColorMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ColorMode::Auto => write!(f, "auto"),
            ColorMode::Always => write!(f, "always"),
            ColorMode::Never => write!(f, "never"),
        }
    }
}

impl FromStr for ColorMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(ColorMode::Auto),
            "always" => Ok(ColorMode::Always),
            "never" => Ok(ColorMode::Never),
            _ => Ok(defaults::COLOR_MODE),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaxMessageLength {
    Unlimited,
    Limited { max_length: usize },
}

impl MaxMessageLength {
    pub fn maybe_truncate_to(&self, length: usize) -> Option<usize> {
        match self {
            MaxMessageLength::Unlimited => None,
            MaxMessageLength::Limited { max_length } => {
                if length > *max_length {
                    Some(*max_length)
                } else {
                    None
                }
            }
        }
    }
}

impl From<usize> for MaxMessageLength {
    fn from(value: usize) -> Self {
        if value == 0 {
            MaxMessageLength::Unlimited
        } else {
            MaxMessageLength::Limited { max_length: value }
        }
    }
}

/// Trait for config values that can be loaded from environment
pub trait ConfigValue: Sized {
    /// Environment variable name
    fn env_name() -> &'static str;

    /// Get value from environment or return default
    fn from_env() -> Self {
        let env_value = env::var(Self::env_name()).ok();
        env_value
            .and_then(|v| Self::parse_value(&v))
            .unwrap_or_else(|| Self::default_value())
    }

    /// Parse string value
    fn parse_value(value: &str) -> Option<Self>;

    /// Default value
    fn default_value() -> Self;

    /// Update from HashMap of environment variables
    fn from_map(map: &HashMap<String, String>) -> Option<Self> {
        map.get(Self::env_name()).and_then(|v| Self::parse_value(v))
    }
}

/// Specific config value implementations
pub struct LogLevelConfig(Option<Level>);
impl ConfigValue for LogLevelConfig {
    fn env_name() -> &'static str {
        "BAML_LOG"
    }

    fn parse_value(value: &str) -> Option<Self> {
        Some(LogLevelConfig(Some(value.parse().ok()?)))
    }

    fn default_value() -> Self {
        LogLevelConfig(None)
    }
}

impl From<LogLevelConfig> for Level {
    fn from(config: LogLevelConfig) -> Self {
        config
            .0
            .or_else(|| {
                env::var(LogLevelConfig::env_name())
                    .ok()
                    .and_then(|v| v.parse().ok())
            })
            .unwrap_or(defaults::LOG_LEVEL)
    }
}

pub struct JsonModeConfig(Option<String>);
impl ConfigValue for JsonModeConfig {
    fn env_name() -> &'static str {
        "BAML_LOG_JSON"
    }

    fn parse_value(value: &str) -> Option<Self> {
        Some(JsonModeConfig(Some(value.to_string())))
    }

    fn default_value() -> Self {
        JsonModeConfig(None)
    }
}

impl From<JsonModeConfig> for bool {
    fn from(config: JsonModeConfig) -> Self {
        config
            .0
            .or_else(|| env::var(JsonModeConfig::env_name()).ok())
            .map(|val| val.trim().eq_ignore_ascii_case("true") || val.trim() == "1")
            .unwrap_or(defaults::USE_JSON)
    }
}

pub struct ColorModeConfig(Option<String>);
impl ConfigValue for ColorModeConfig {
    fn env_name() -> &'static str {
        "BAML_LOG_COLOR_MODE"
    }

    fn parse_value(value: &str) -> Option<Self> {
        Some(ColorModeConfig(Some(value.to_string())))
    }

    fn default_value() -> Self {
        ColorModeConfig(None)
    }
}

impl From<ColorModeConfig> for ColorMode {
    fn from(config: ColorModeConfig) -> Self {
        config
            .0
            .or_else(|| env::var(ColorModeConfig::env_name()).ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults::COLOR_MODE)
    }
}

pub struct LspConfig(Option<String>);
impl ConfigValue for LspConfig {
    fn env_name() -> &'static str {
        "BAML_LOG_LSP"
    }

    fn parse_value(value: &str) -> Option<Self> {
        Some(LspConfig(Some(value.to_string())))
    }

    fn default_value() -> Self {
        LspConfig(None)
    }
}

impl From<LspConfig> for bool {
    fn from(config: LspConfig) -> Self {
        config
            .0
            .or_else(|| env::var(LspConfig::env_name()).ok())
            .map(|val| val.trim().eq_ignore_ascii_case("true") || val.trim() == "1")
            .unwrap_or(defaults::RUNNING_IN_LSP)
    }
}
pub struct MaxMessageLengthConfig(Option<String>);
impl ConfigValue for MaxMessageLengthConfig {
    fn env_name() -> &'static str {
        "BAML_LOG_MAX_MESSAGE_LENGTH"
    }

    fn parse_value(value: &str) -> Option<Self> {
        Some(MaxMessageLengthConfig(Some(value.to_string())))
    }

    fn default_value() -> Self {
        MaxMessageLengthConfig(None)
    }
}

impl From<MaxMessageLengthConfig> for MaxMessageLength {
    fn from(config: MaxMessageLengthConfig) -> Self {
        config
            .0
            .or_else(|| env::var(MaxMessageLengthConfig::env_name()).ok())
            .and_then(|val| {
                if val.is_empty() {
                    Some(MaxMessageLength::Unlimited)
                } else {
                    val.parse::<usize>().ok().map(|len| len.into())
                }
            })
            .unwrap_or_else(|| defaults::MAX_MESSAGE_LENGTH.into())
    }
}

/// Configuration for the logger
#[derive(Debug, Clone, PartialEq)]
struct LogConfig {
    /// Current log level
    level: Level,
    /// Whether to use JSON formatting
    use_json: bool,
    /// Color output mode
    color_mode: ColorMode,
    /// Maximum log message length
    max_message_length: MaxMessageLength,
    /// Whether initialization has completed
    initialized: bool,
    /// Whether we are running in the context of our LSP, which will prevent us from writing to stdout
    running_in_lsp: bool,
}

impl LogConfig {
    /// Create a new config with values from environment
    fn from_env() -> Self {
        Self {
            level: LogLevelConfig::from_env().into(),
            use_json: JsonModeConfig::from_env().into(),
            color_mode: ColorModeConfig::from_env().into(),
            max_message_length: MaxMessageLengthConfig::from_env().into(),
            initialized: false,
            running_in_lsp: false,
        }
    }

    /// Update config from environment
    fn reload_from_env(&mut self) {
        self.level = LogLevelConfig::from_env().into();
        self.use_json = JsonModeConfig::from_env().into();
        self.color_mode = ColorModeConfig::from_env().into();
        self.max_message_length = MaxMessageLengthConfig::from_env().into();
        self.running_in_lsp = LspConfig::from_env().into();
    }

    /// Update config from a HashMap of environment variables
    fn update_from_map(&mut self, vars: &HashMap<String, String>) {
        if let Some(level_config) = LogLevelConfig::from_map(vars) {
            self.level = level_config.into();
        }

        if let Some(json_config) = JsonModeConfig::from_map(vars) {
            self.use_json = json_config.into();
        }

        if let Some(color_config) = ColorModeConfig::from_map(vars) {
            self.color_mode = color_config.into();
        }

        if let Some(length_config) = MaxMessageLengthConfig::from_map(vars) {
            self.max_message_length = length_config.into();
        }
    }

    fn to_logger(&self) -> Logger {
        Logger {
            level: self.level,
            use_json: self.use_json,
            color_mode: self.color_mode,
            max_message_length: self.max_message_length,
            running_in_lsp: self.running_in_lsp,
        }
    }
}

lazy_static! {
    /// Thread-safe configuration with runtime modification support
    static ref CONFIG: RwLock<LogConfig> = RwLock::new(LogConfig::from_env());
    static ref LOGGED_LINES: RwLock<HashSet<(Option<String>, Option<String>, Option<u32>)>> = RwLock::new(HashSet::new());
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

// /// JSON-serializable log entry
// #[derive(Serialize)]
// struct LogEntry<'a> {
//     /// Timestamp in ISO 8601 format
//     timestamp: String,
//     /// Log level as a string
//     level: &'a str,
//     /// Log message
//     message: String,
//     /// Optional module path
//     #[serde(skip_serializing_if = "Option::is_none")]
//     module_path: Option<&'a str>,
//     /// Optional file name
//     #[serde(skip_serializing_if = "Option::is_none")]
//     file: Option<&'a str>,
//     /// Optional line number
//     #[serde(skip_serializing_if = "Option::is_none")]
//     line: Option<u32>,
// }

/// Logger instance that can be customized
pub struct Logger {
    level: Level,
    use_json: bool,
    color_mode: ColorMode,
    max_message_length: MaxMessageLength,
    running_in_lsp: bool,
}

/// Trait for creating a type-safe configuration API
pub trait ConfigSetting<T> {
    /// Get the current value
    fn get() -> T;

    /// Set the value
    fn set(value: T) -> Result<(), LogError>;
}

/// Type-safe configuration settings
pub struct LogLevel;
impl ConfigSetting<Level> for LogLevel {
    fn get() -> Level {
        match CONFIG.read() {
            Ok(config) => config.level,
            Err(_) => defaults::LOG_LEVEL,
        }
    }

    #[allow(clippy::print_stdout)]
    fn set(value: Level) -> Result<(), LogError> {
        match CONFIG.write() {
            Ok(mut config) => {
                let old_level = config.level;
                config.level = value;
                if old_level != value && value != Level::Off {
                    println!("[BAML] Log level set to {}", value.colored());
                }
                Ok(())
            }
            Err(_) => Err(LogError::LockError),
        }
    }
}

pub struct JsonMode;
impl ConfigSetting<bool> for JsonMode {
    fn get() -> bool {
        match CONFIG.read() {
            Ok(config) => config.use_json,
            Err(_) => defaults::USE_JSON,
        }
    }

    fn set(value: bool) -> Result<(), LogError> {
        match CONFIG.write() {
            Ok(mut config) => {
                config.use_json = value;
                Ok(())
            }
            Err(_) => Err(LogError::LockError),
        }
    }
}

pub struct LogColorMode;
impl ConfigSetting<ColorMode> for LogColorMode {
    fn get() -> ColorMode {
        match CONFIG.read() {
            Ok(config) => config.color_mode,
            Err(_) => defaults::COLOR_MODE,
        }
    }

    fn set(value: ColorMode) -> Result<(), LogError> {
        match CONFIG.write() {
            Ok(mut config) => {
                config.color_mode = value;
                Ok(())
            }
            Err(_) => Err(LogError::LockError),
        }
    }
}

pub struct LogMaxMessageLength;
impl ConfigSetting<MaxMessageLength> for LogMaxMessageLength {
    fn get() -> MaxMessageLength {
        match CONFIG.read() {
            Ok(config) => config.max_message_length,
            Err(_) => defaults::MAX_MESSAGE_LENGTH.into(),
        }
    }

    fn set(value: MaxMessageLength) -> Result<(), LogError> {
        match CONFIG.write() {
            Ok(mut config) => {
                config.max_message_length = value;
                Ok(())
            }
            Err(_) => Err(LogError::LockError),
        }
    }
}

/// Initialize the logger
pub fn init() -> Result<(), LogError> {
    let mut result = Ok(());

    INIT.call_once(|| {
        if let Ok(mut config) = CONFIG.write() {
            config.reload_from_env();
            config.initialized = true;
        } else {
            result = Err(LogError::LockError);
        }
    });

    result
}

/// Get the current log level
pub fn get_log_level() -> Level {
    LogLevel::get()
}

/// Set the log level at runtime
pub fn set_log_level(level: Level) -> Result<(), LogError> {
    LogLevel::set(level)
}

/// Set the JSON formatting mode at runtime
pub fn set_json_mode(enable: bool) -> Result<(), LogError> {
    JsonMode::set(enable)
}

/// Set the color mode at runtime
pub fn set_color_mode(mode: ColorMode) -> Result<(), LogError> {
    LogColorMode::set(mode)
}

/// Set the max message length at runtime
pub fn set_max_message_length(max_length: usize) -> Result<(), LogError> {
    LogMaxMessageLength::set(max_length.into())
}

/// Update configuration from a map of environment variables
pub fn set_from_env(env_vars: &HashMap<String, String>) -> Result<(), LogError> {
    match CONFIG.write() {
        Ok(mut config) => {
            config.update_from_map(env_vars);
            Ok(())
        }
        Err(_) => Err(LogError::LockError),
    }
}

pub fn set_running_in_lsp(running_in_lsp: bool) -> Result<(), LogError> {
    match CONFIG.write() {
        Ok(mut config) => {
            config.running_in_lsp = running_in_lsp;
            Ok(())
        }
        Err(_) => Err(LogError::LockError),
    }
}

/// Reload all configuration from environment variables
pub fn reload_from_env() -> Result<(), LogError> {
    match CONFIG.write() {
        Ok(mut config) => {
            config.reload_from_env();
            Ok(())
        }
        Err(_) => Err(LogError::LockError),
    }
}

/// Trait for objects that can be logged
pub trait Loggable {
    fn as_baml_log_string(&self, max_message_length: &MaxMessageLength) -> String;
    fn as_baml_log_json(
        &self,
        max_message_length: &MaxMessageLength,
    ) -> Result<serde_json::Value, LogError>;
}

/// Internal function used by logging macros, that that line is only logged once
pub fn log_internal_once(
    level: Level,
    message: &str,
    module_path: Option<&str>,
    file: Option<&str>,
    line: Option<u32>,
) {
    let key = (module_path.map(String::from), file.map(String::from), line);

    let mut logged_lines = LOGGED_LINES.write().unwrap();
    if !logged_lines.contains(&key) {
        logged_lines.insert(key);
        log_internal(level, message, module_path, file, line);
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
    INIT.call_once(|| {
        if let Ok(mut config) = CONFIG.write() {
            config.initialized = true;
        }
    });

    // Create a temporary logger with the current config
    let logger = match CONFIG.read() {
        Ok(config) => config.clone(),
        Err(_) => {
            log::error!("Can't get config, skip logging");
            return;
        }
    }
    .to_logger();

    if !logger.level.is_at_least(level) {
        return;
    }

    let now = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%.3f")
        .to_string();
    // Log the message
    logger.log(now, level, message, module_path, file, line);
}

impl Logger {
    fn log(
        &self,
        now: String,
        level: Level,
        message: &str,
        _module_path: Option<&str>,
        _file: Option<&str>,
        _line: Option<u32>,
    ) {
        // Configure color control based on mode
        match self.color_mode {
            ColorMode::Always => control::set_override(true),
            ColorMode::Never => control::set_override(false),
            ColorMode::Auto => {} // Use default detection
        }

        if self.running_in_lsp {
            // When running in the context of our LSP, we can't write to stdout since that will
            // mess up the LSP communication protocol, which uses stdout/stderr for communication.
            match level {
                Level::Fatal => {
                    log::error!("{} [BAML {}] {}", now, level.colored(), message.trim())
                }
                Level::Error => {
                    log::error!("{} [BAML {}] {}", now, level.colored(), message.trim())
                }
                Level::Warn => {
                    log::warn!("{} [BAML {}] {}", now, level.colored(), message.trim())
                }
                Level::Info => {
                    log::info!("{} [BAML {}] {}", now, level.colored(), message.trim())
                }
                Level::Debug => {
                    log::debug!("{} [BAML {}] {}", now, level.colored(), message.trim())
                }
                Level::Trace => {
                    log::trace!("{} [BAML {}] {}", now, level.colored(), message.trim())
                }
                Level::Off => {}
            }
        } else {
            let _ = writeln!(
                io::stdout(),
                "{} [BAML {}] {}",
                now,
                level.colored(),
                message.trim()
            );
        }
    }
}

/// Internal function used by event logging macros
pub fn log_event_internal<T: Loggable>(
    level: Level,
    payload: &T,
    _module_path: Option<&str>,
    _file: Option<&str>,
    _line: Option<u32>,
) {
    // Ensure the logger is initialized
    INIT.call_once(|| {
        if let Ok(mut config) = CONFIG.write() {
            config.initialized = true;
        }
    });

    // Create a temporary logger with the current config
    let config = match CONFIG.read() {
        Ok(config) => config.clone(),
        Err(_) => return, // Can't get config, skip logging
    }
    .to_logger();

    // Skip if level is not enabled
    if !config.level.is_at_least(level) {
        return;
    }

    let level_str = level.as_str();
    let now = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%.3f")
        .to_string();

    if config.use_json {
        // In JSON mode, use the payload directly
        if let Ok(json_value) = payload.as_baml_log_json(&config.max_message_length) {
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
            let _ = writeln!(io::stdout(), "{json_str}");
        }
    } else {
        // In regular mode, convert payload to a debug string
        let payload_str = payload.as_baml_log_string(&config.max_message_length);
        // multi-line payloads should be indented
        let payload_str = if payload_str.contains('\n') {
            payload_str
                .lines()
                .map(|line| format!("    {line}"))
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

        if !config.running_in_lsp {
            let _ = writeln!(
                io::stdout(),
                "{} [BAML {}] {}",
                now,
                level.colored(),
                payload_str.trim()
            );
        } else {
            match level {
                Level::Fatal => {
                    log::error!("{} [BAML {}] {}", now, level.colored(), payload_str.trim())
                }
                Level::Error => {
                    log::error!("{} [BAML {}] {}", now, level.colored(), payload_str.trim())
                }
                Level::Warn => {
                    log::warn!("{} [BAML {}] {}", now, level.colored(), payload_str.trim())
                }
                Level::Info => {
                    log::info!("{} [BAML {}] {}", now, level.colored(), payload_str.trim())
                }
                Level::Debug => {
                    log::debug!("{} [BAML {}] {}", now, level.colored(), payload_str.trim())
                }
                Level::Trace => {
                    log::trace!("{} [BAML {}] {}", now, level.colored(), payload_str.trim())
                }
                Level::Off => {}
            }
        }
    }
}

#[cfg(test)]
#[serial_test::serial]
mod tests {

    use super::*;

    // Helper to temporarily set environment variables for testing
    struct EnvGuard {
        vars: Vec<(String, Option<String>)>,
    }

    impl EnvGuard {
        fn new() -> Self {
            Self { vars: Vec::new() }
        }

        fn set(&mut self, key: &str, value: &str) {
            let old_value = env::var(key).ok();
            self.vars.push((key.to_string(), old_value));
            env::set_var(key, value);
        }

        fn remove(&mut self, key: &str) {
            let old_value = env::var(key).ok();
            self.vars.push((key.to_string(), old_value));
            env::remove_var(key);
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in self.vars.iter().rev() {
                match value {
                    Some(v) => env::set_var(key, v),
                    None => env::remove_var(key),
                }
            }
        }
    }

    #[test]
    fn test_level_from_str() {
        assert_eq!(Level::from_str("off").unwrap(), Level::Off);
        assert_eq!(Level::from_str("OFF").unwrap(), Level::Off);
        assert_eq!(Level::from_str("error").unwrap(), Level::Error);
        assert_eq!(Level::from_str("ERROR").unwrap(), Level::Error);
        assert_eq!(Level::from_str("warn").unwrap(), Level::Warn);
        assert_eq!(Level::from_str("info").unwrap(), Level::Info);
        assert_eq!(Level::from_str("debug").unwrap(), Level::Debug);
        assert_eq!(Level::from_str("trace").unwrap(), Level::Trace);

        // Invalid values should return default
        assert_eq!(Level::from_str("invalid").unwrap(), defaults::LOG_LEVEL);
        assert_eq!(Level::from_str("").unwrap(), defaults::LOG_LEVEL);
    }

    // Tests for MaxMessageLength
    #[test]
    fn test_max_message_length_from_usize() {
        assert_eq!(MaxMessageLength::from(0), MaxMessageLength::Unlimited);
        assert_eq!(
            MaxMessageLength::from(100),
            MaxMessageLength::Limited { max_length: 100 }
        );
        assert_eq!(
            MaxMessageLength::from(1000),
            MaxMessageLength::Limited { max_length: 1000 }
        );
    }

    #[test]
    fn test_max_message_length_maybe_truncate_to() {
        let unlimited = MaxMessageLength::Unlimited;
        assert_eq!(unlimited.maybe_truncate_to(100), None);
        assert_eq!(unlimited.maybe_truncate_to(1000000), None);

        let limited = MaxMessageLength::Limited { max_length: 100 };
        assert_eq!(limited.maybe_truncate_to(50), None);
        assert_eq!(limited.maybe_truncate_to(100), None);
        assert_eq!(limited.maybe_truncate_to(150), Some(100));
        assert_eq!(limited.maybe_truncate_to(1000), Some(100));
    }

    // Tests for ConfigValue implementations
    #[test]
    fn test_log_level_config_from_env() {
        let mut guard = EnvGuard::new();

        guard.set("BAML_LOG", "debug");
        let level: Level = LogLevelConfig::from_env().into();
        assert_eq!(level, Level::Debug);

        guard.set("BAML_LOG", "error");
        let level: Level = LogLevelConfig::from_env().into();
        assert_eq!(level, Level::Error);

        guard.remove("BAML_LOG");
        let level: Level = LogLevelConfig::from_env().into();
        assert_eq!(level, defaults::LOG_LEVEL);
    }

    #[test]
    fn test_json_mode_config_from_env() {
        let mut guard = EnvGuard::new();

        guard.set("BAML_LOG_JSON", "true");
        let json_mode: bool = JsonModeConfig::from_env().into();
        assert!(json_mode);

        guard.set("BAML_LOG_JSON", "TRUE");
        let json_mode: bool = JsonModeConfig::from_env().into();
        assert!(json_mode);

        guard.set("BAML_LOG_JSON", "1");
        let json_mode: bool = JsonModeConfig::from_env().into();
        assert!(json_mode);

        guard.set("BAML_LOG_JSON", "false");
        let json_mode: bool = JsonModeConfig::from_env().into();
        assert!(!json_mode);

        guard.set("BAML_LOG_JSON", "0");
        let json_mode: bool = JsonModeConfig::from_env().into();
        assert!(!json_mode);

        guard.remove("BAML_LOG_JSON");
        let json_mode: bool = JsonModeConfig::from_env().into();
        assert_eq!(json_mode, defaults::USE_JSON);
    }

    #[test]
    fn test_color_mode_config_from_env() {
        let mut guard = EnvGuard::new();

        guard.set("BAML_LOG_COLOR_MODE", "always");
        let color_mode: ColorMode = ColorModeConfig::from_env().into();
        assert_eq!(color_mode, ColorMode::Always);

        guard.set("BAML_LOG_COLOR_MODE", "never");
        let color_mode: ColorMode = ColorModeConfig::from_env().into();
        assert_eq!(color_mode, ColorMode::Never);

        guard.remove("BAML_LOG_COLOR_MODE");
        let color_mode: ColorMode = ColorModeConfig::from_env().into();
        assert_eq!(color_mode, defaults::COLOR_MODE);
    }

    #[test]
    fn test_max_message_length_config_from_env() {
        let mut guard = EnvGuard::new();

        guard.set("BAML_LOG_MAX_MESSAGE_LENGTH", "500");
        let max_length: MaxMessageLength = MaxMessageLengthConfig::from_env().into();
        assert_eq!(max_length, MaxMessageLength::Limited { max_length: 500 });

        guard.set("BAML_LOG_MAX_MESSAGE_LENGTH", "0");
        let max_length: MaxMessageLength = MaxMessageLengthConfig::from_env().into();
        assert_eq!(max_length, MaxMessageLength::Unlimited);

        guard.set("BAML_LOG_MAX_MESSAGE_LENGTH", "");
        let max_length: MaxMessageLength = MaxMessageLengthConfig::from_env().into();
        assert_eq!(max_length, MaxMessageLength::Unlimited);

        guard.remove("BAML_LOG_MAX_MESSAGE_LENGTH");
        let max_length: MaxMessageLength = MaxMessageLengthConfig::from_env().into();
        assert_eq!(max_length, defaults::MAX_MESSAGE_LENGTH.into());
    }

    #[test]
    fn test_lsp_config_from_env() {
        let mut guard = EnvGuard::new();

        guard.set("BAML_LOG_LSP", "true");
        let lsp_mode: bool = LspConfig::from_env().into();
        assert!(lsp_mode);

        guard.set("BAML_LOG_LSP", "1");
        let lsp_mode: bool = LspConfig::from_env().into();
        assert!(lsp_mode);

        guard.set("BAML_LOG_LSP", "false");
        let lsp_mode: bool = LspConfig::from_env().into();
        assert!(!lsp_mode);

        guard.remove("BAML_LOG_LSP");
        let lsp_mode: bool = LspConfig::from_env().into();
        assert_eq!(lsp_mode, defaults::RUNNING_IN_LSP);
    }

    // Tests for set_from_env - Testing potential bug
    #[test]
    fn test_set_from_env_updates_config() {
        // Save current state and ensure clean state
        let _ = init();

        // Create test environment variables
        let mut env_vars = HashMap::new();
        env_vars.insert("BAML_LOG".to_string(), "trace".to_string());
        env_vars.insert("BAML_LOG_JSON".to_string(), "true".to_string());
        env_vars.insert("BAML_LOG_COLOR_MODE".to_string(), "never".to_string());
        env_vars.insert("BAML_LOG_MAX_MESSAGE_LENGTH".to_string(), "200".to_string());

        // Apply the environment variables
        assert!(set_from_env(&env_vars).is_ok());

        // Verify the changes took effect
        assert_eq!(get_log_level(), Level::Trace);
        assert!(JsonMode::get());
        assert_eq!(LogColorMode::get(), ColorMode::Never);
        assert_eq!(
            LogMaxMessageLength::get(),
            MaxMessageLength::Limited { max_length: 200 }
        );
    }

    #[test]
    fn test_set_from_env_partial_update() {
        let _ = init();

        // Set known initial state
        let _ = set_log_level(Level::Debug);
        let _ = set_json_mode(true);
        let _ = set_color_mode(ColorMode::Always);

        // Update only log level
        let mut env_vars = HashMap::new();
        env_vars.insert("BAML_LOG".to_string(), "error".to_string());

        assert!(set_from_env(&env_vars).is_ok());

        // Check that only the specified value changed
        assert_eq!(get_log_level(), Level::Error);
        // Other values should remain unchanged
        assert!(JsonMode::get());
        assert_eq!(LogColorMode::get(), ColorMode::Always);
    }

    #[test]
    fn test_set_from_env_invalid_values() {
        let _ = init();

        // Test with invalid values
        let mut env_vars = HashMap::new();
        env_vars.insert("BAML_LOG".to_string(), "invalid_level".to_string());
        env_vars.insert(
            "BAML_LOG_MAX_MESSAGE_LENGTH".to_string(),
            "not_a_number".to_string(),
        );

        // Should not panic, should use defaults for invalid values
        assert!(set_from_env(&env_vars).is_ok());

        // Invalid log level should default to INFO
        assert_eq!(get_log_level(), defaults::LOG_LEVEL);
    }

    #[test]
    fn test_set_from_env_empty_map() {
        let _ = init();

        let original_level = get_log_level();
        let env_vars = HashMap::new();

        assert!(set_from_env(&env_vars).is_ok());

        // Should not change anything
        assert_eq!(get_log_level(), original_level);
    }

    // Tests for configuration update functions
    #[test]
    fn test_set_log_level() {
        let _ = init();

        // Test setting to Debug
        assert!(set_log_level(Level::Debug).is_ok());
        assert_eq!(get_log_level(), Level::Debug);

        // Test setting to Error
        assert!(set_log_level(Level::Error).is_ok());
        assert_eq!(get_log_level(), Level::Error);

        // Test setting to Info
        assert!(set_log_level(Level::Info).is_ok());
        assert_eq!(get_log_level(), Level::Info);
    }

    #[test]
    fn test_set_json_mode() {
        let _ = init();

        assert!(set_json_mode(true).is_ok());
        assert!(JsonMode::get());

        assert!(set_json_mode(false).is_ok());
        assert!(!JsonMode::get());
    }

    #[test]
    fn test_set_color_mode() {
        let _ = init();

        assert!(set_color_mode(ColorMode::Always).is_ok());
        assert_eq!(LogColorMode::get(), ColorMode::Always);

        assert!(set_color_mode(ColorMode::Never).is_ok());
        assert_eq!(LogColorMode::get(), ColorMode::Never);
    }

    #[test]
    fn test_set_max_message_length() {
        let _ = init();

        assert!(set_max_message_length(1000).is_ok());
        assert_eq!(
            LogMaxMessageLength::get(),
            MaxMessageLength::Limited { max_length: 1000 }
        );

        assert!(set_max_message_length(0).is_ok());
        assert_eq!(LogMaxMessageLength::get(), MaxMessageLength::Unlimited);
    }

    #[test]
    fn test_set_running_in_lsp() {
        let _ = init();

        assert!(set_running_in_lsp(true).is_ok());
        // Can't directly test the internal state, but we can verify it doesn't panic

        assert!(set_running_in_lsp(false).is_ok());
    }

    #[test]
    fn test_reload_from_env() {
        let _ = init();
        let mut env_guard = EnvGuard::new();

        // Set environment variables
        env_guard.set("BAML_LOG", "warn");
        env_guard.set("BAML_LOG_JSON", "true");

        // Reload from environment
        assert!(reload_from_env().is_ok());

        // Verify the changes
        assert_eq!(get_log_level(), Level::Warn);
        assert!(JsonMode::get());
    }

    // Test thread safety of configuration
    #[test]
    fn test_concurrent_config_access() {
        use std::thread;

        let _ = init();

        let handles: Vec<_> = (0..10)
            .map(|i| {
                thread::spawn(move || {
                    for _ in 0..100 {
                        let level = if i % 2 == 0 {
                            Level::Debug
                        } else {
                            Level::Info
                        };
                        let _ = set_log_level(level);
                        let _ = get_log_level();
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }

    // Test log_internal_once functionality
    #[test]
    fn test_log_internal_once() {
        // Clear logged lines for test
        {
            let mut logged_lines = LOGGED_LINES.write().unwrap();
            logged_lines.clear();
        }

        // First call should log
        log_internal_once(
            Level::Info,
            "Test message",
            Some("test_module"),
            Some("test_file.rs"),
            Some(42),
        );

        // Check that the line was logged
        {
            let logged_lines = LOGGED_LINES.read().unwrap();
            assert_eq!(logged_lines.len(), 1);
            assert!(logged_lines.contains(&(
                Some("test_module".to_string()),
                Some("test_file.rs".to_string()),
                Some(42)
            )));
        }

        // Second call with same parameters should not log again
        let logged_lines_count = LOGGED_LINES.read().unwrap().len();
        log_internal_once(
            Level::Info,
            "Test message",
            Some("test_module"),
            Some("test_file.rs"),
            Some(42),
        );

        // Should still have same count
        assert_eq!(LOGGED_LINES.read().unwrap().len(), logged_lines_count);

        // Different line number should log
        log_internal_once(
            Level::Info,
            "Test message",
            Some("test_module"),
            Some("test_file.rs"),
            Some(43),
        );

        assert_eq!(LOGGED_LINES.read().unwrap().len(), logged_lines_count + 1);
    }

    // Test LogConfig functionality
    #[test]
    fn test_log_config_from_env() {
        // Save current environment state
        let saved_log = env::var("BAML_LOG").ok();
        let saved_json = env::var("BAML_LOG_JSON").ok();
        let saved_color = env::var("BAML_LOG_COLOR_MODE").ok();
        let saved_length = env::var("BAML_LOG_MAX_MESSAGE_LENGTH").ok();

        // Set test values
        env::set_var("BAML_LOG", "DEBUG");
        env::set_var("BAML_LOG_JSON", "true");
        env::set_var("BAML_LOG_COLOR_MODE", "always");
        env::set_var("BAML_LOG_MAX_MESSAGE_LENGTH", "500");

        // Create config from environment
        let config = LogConfig::from_env();

        // Restore original environment
        match saved_log {
            Some(v) => env::set_var("BAML_LOG", v),
            None => env::remove_var("BAML_LOG"),
        }
        match saved_json {
            Some(v) => env::set_var("BAML_LOG_JSON", v),
            None => env::remove_var("BAML_LOG_JSON"),
        }
        match saved_color {
            Some(v) => env::set_var("BAML_LOG_COLOR_MODE", v),
            None => env::remove_var("BAML_LOG_COLOR_MODE"),
        }
        match saved_length {
            Some(v) => env::set_var("BAML_LOG_MAX_MESSAGE_LENGTH", v),
            None => env::remove_var("BAML_LOG_MAX_MESSAGE_LENGTH"),
        }

        // Assert the values
        assert_eq!(
            config,
            LogConfig {
                level: Level::Debug,
                use_json: true,
                color_mode: ColorMode::Always,
                max_message_length: MaxMessageLength::Limited { max_length: 500 },
                initialized: false,
                running_in_lsp: false,
            }
        );
    }

    #[test]
    fn test_log_config_update_from_map() {
        let mut config = LogConfig::from_env();

        let mut vars = HashMap::new();
        vars.insert("BAML_LOG".to_string(), "ERROR".to_string());
        vars.insert("BAML_LOG_JSON".to_string(), "false".to_string());

        config.update_from_map(&vars);

        assert_eq!(config.level, Level::Error);
        assert!(!config.use_json);
    }
}
