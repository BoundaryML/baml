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
        env::var(Self::env_name())
            .ok()
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
pub struct LogLevelConfig;
impl ConfigValue for LogLevelConfig {
    fn env_name() -> &'static str {
        "BAML_LOG"
    }

    fn parse_value(_: &str) -> Option<Self> {
        Some(LogLevelConfig)
    }

    fn default_value() -> Self {
        LogLevelConfig
    }
}

impl From<LogLevelConfig> for Level {
    fn from(_: LogLevelConfig) -> Self {
        env::var(LogLevelConfig::env_name())
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults::LOG_LEVEL)
    }
}

pub struct JsonModeConfig;
impl ConfigValue for JsonModeConfig {
    fn env_name() -> &'static str {
        "BAML_LOG_JSON"
    }

    fn parse_value(_: &str) -> Option<Self> {
        Some(JsonModeConfig)
    }

    fn default_value() -> Self {
        JsonModeConfig
    }
}

impl From<JsonModeConfig> for bool {
    fn from(_: JsonModeConfig) -> Self {
        env::var(JsonModeConfig::env_name())
            .map(|val| val.trim().eq_ignore_ascii_case("true") || val.trim() == "1")
            .unwrap_or(defaults::USE_JSON)
    }
}

pub struct ColorModeConfig;
impl ConfigValue for ColorModeConfig {
    fn env_name() -> &'static str {
        "BAML_LOG_COLOR_MODE"
    }

    fn parse_value(_: &str) -> Option<Self> {
        Some(ColorModeConfig)
    }

    fn default_value() -> Self {
        ColorModeConfig
    }
}

impl From<ColorModeConfig> for ColorMode {
    fn from(_: ColorModeConfig) -> Self {
        env::var(ColorModeConfig::env_name())
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults::COLOR_MODE)
    }
}

pub struct LspConfig;
impl ConfigValue for LspConfig {
    fn env_name() -> &'static str {
        "BAML_LOG_LSP"
    }

    fn parse_value(_: &str) -> Option<Self> {
        Some(LspConfig)
    }

    fn default_value() -> Self {
        LspConfig
    }
}

impl From<LspConfig> for bool {
    fn from(_: LspConfig) -> Self {
        env::var(LspConfig::env_name())
            .map(|val| val.trim().eq_ignore_ascii_case("true") || val.trim() == "1")
            .unwrap_or(defaults::RUNNING_IN_LSP)
    }
}
pub struct MaxMessageLengthConfig;
impl ConfigValue for MaxMessageLengthConfig {
    fn env_name() -> &'static str {
        "BAML_LOG_MAX_MESSAGE_LENGTH"
    }

    fn parse_value(_: &str) -> Option<Self> {
        Some(MaxMessageLengthConfig)
    }

    fn default_value() -> Self {
        MaxMessageLengthConfig
    }
}

impl From<MaxMessageLengthConfig> for MaxMessageLength {
    fn from(_: MaxMessageLengthConfig) -> Self {
        env::var(MaxMessageLengthConfig::env_name())
            .ok()
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
#[derive(Debug, Clone)]
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
