use std::{fmt, io, path::PathBuf};

use baml_lib::Diagnostics;
use colored::Colorize;

#[derive(Debug)]
pub enum CliError {
    GlobError(glob::GlobError),
    PatternError(glob::PatternError),
    StringError(String),
    BAMLError(Diagnostics),
}

impl From<glob::GlobError> for CliError {
    fn from(error: glob::GlobError) -> Self {
        CliError::GlobError(error)
    }
}

impl From<glob::PatternError> for CliError {
    fn from(error: glob::PatternError) -> Self {
        CliError::PatternError(error)
    }
}

impl From<String> for CliError {
    fn from(error: String) -> Self {
        CliError::StringError(error)
    }
}

impl From<&'static str> for CliError {
    fn from(error: &'static str) -> Self {
        CliError::StringError(String::from(error))
    }
}

impl From<Diagnostics> for CliError {
    fn from(error: Diagnostics) -> Self {
        CliError::BAMLError(error)
    }
}

impl From<(&PathBuf, std::io::Error)> for CliError {
    fn from(error: (&PathBuf, std::io::Error)) -> Self {
        CliError::StringError(format!("{}: {}", error.0.to_string_lossy().bold(), error.1))
    }
}

impl From<(&'static str, std::io::Error)> for CliError {
    fn from(error: (&'static str, std::io::Error)) -> Self {
        CliError::StringError(format!("{}: {}", error.0.bold(), error.1))
    }
}

impl From<reqwest::Error> for CliError {
    fn from(error: reqwest::Error) -> Self {
        CliError::StringError(error.to_string())
    }
}

impl From<base64::DecodeError> for CliError {
    fn from(error: base64::DecodeError) -> Self {
        CliError::StringError(error.to_string())
    }
}

impl From<serde_json::Error> for CliError {
    fn from(error: serde_json::Error) -> Self {
        CliError::StringError(error.to_string())
    }
}

impl From<io::Error> for CliError {
    fn from(error: io::Error) -> Self {
        CliError::StringError(error.to_string())
    }
}

impl From<std::string::FromUtf8Error> for CliError {
    fn from(error: std::string::FromUtf8Error) -> Self {
        CliError::StringError(error.to_string())
    }
}

impl From<dialoguer::Error> for CliError {
    fn from(error: dialoguer::Error) -> Self {
        CliError::StringError(error.to_string())
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CliError::BAMLError(error) => {
                write!(
                    f,
                    "{0}\n{1}\n{2}{0}",
                    "Compiler failed".bold(),
                    error.warnings_to_pretty_string(),
                    error.to_pretty_string(),
                )
            }
            CliError::StringError(err) => write!(f, "{}", err),
            CliError::GlobError(err) => write!(f, "{}", err),
            CliError::PatternError(err) => write!(f, "{}", err),
        }
    }
}

impl std::error::Error for CliError {}
