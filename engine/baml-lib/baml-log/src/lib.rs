//! A custom logging library for BAML that supports JSON and standard formats.
//!
//! This crate provides a simple logging interface similar to the standard `log` crate
//! but with separate configuration controlled by BAML-specific environment variables.
//!
//! # Features
//!
//! - Controlled by BAML-specific environment variables
//! - Support for both text and JSON log formats
//! - Dynamic configuration changes at runtime
//! - Log level filtering
//! - File, line, and module path information
//!
//! # Environment Variables
//!
//! - `BAML_LOG`: Sets the log level (error, warn, info, debug, trace)
//! - `BAML_LOG_JSON`: Enables JSON formatting when set to "true" or "1"
//! - `BAML_LOG_STYLE`: Controls color output ("auto", "always", "never")
//!
//! # Example
//!
//! ```
//! use baml_log::{binfo, bwarn, berror, bdebug, btrace};
//! use baml_log::{Level, set_log_level};
//!
//! // Initialize the logger (optional)
//! baml_log::init();
//!
//! // Log messages at different levels
//! binfo!("This is an info message");
//! bwarn!("This is a warning: {}", "something went wrong");
//!
//! // Dynamically change the log level
//! set_log_level(Level::Debug);
//! bdebug!("This debug message is now visible");
//! ```

// Export the macros
#[macro_use]
mod macros;

#[macro_use]
mod event;

mod logger;

// Re-export the core types and functions
pub use logger::{
    get_log_level, init, log_event_internal, log_internal, log_internal_once, reload_from_env,
    set_color_mode, set_from_env, set_json_mode, set_log_level, set_max_message_length,
    set_running_in_lsp, Level, LogError, Loggable, MaxMessageLength,
};

pub use crate::{
    bdebug as debug, berror as error, bfatal_once as fatal_once, binfo as info, blog as log,
    btrace as trace, bwarn as warn,
};

// Provide a prelude for easy imports
pub mod prelude {
    pub use crate::{init, set_color_mode, set_json_mode, set_log_level, Level, LogError};
}
