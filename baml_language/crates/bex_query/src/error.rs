use std::{fmt, io};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BqlDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
    pub source_line: String,
    pub correction: Option<String>,
    pub valid: Vec<String>,
}

#[derive(Debug)]
pub enum QueryError {
    Io(io::Error),
    InvalidData(String),
    InvalidRequest(String),
    NotFound(String),
    CapabilityUnavailable(String),
    BudgetExceeded { required: usize, max_bytes: usize },
    Bql(BqlDiagnostic),
}

impl QueryError {
    pub(crate) fn invalid_data(message: impl Into<String>) -> Self {
        Self::InvalidData(message.into())
    }

    pub(crate) fn invalid_request(message: impl Into<String>) -> Self {
        Self::InvalidRequest(message.into())
    }

    pub(crate) fn bql(
        code: &'static str,
        source: &str,
        start: usize,
        end: usize,
        message: impl Into<String>,
    ) -> Self {
        let bounded_start = start.min(source.len());
        let bounded_end = end.max(bounded_start.saturating_add(1)).min(source.len());
        let line_start = source[..bounded_start]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let line_end = source[bounded_start..]
            .find('\n')
            .map_or(source.len(), |index| bounded_start + index);
        let line = source[..bounded_start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let column = source[line_start..bounded_start].chars().count() + 1;
        Self::Bql(BqlDiagnostic {
            code,
            message: message.into(),
            start: bounded_start,
            end: bounded_end,
            line,
            column,
            source_line: source[line_start..line_end].to_owned(),
            correction: None,
            valid: Vec::new(),
        })
    }

    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "E_IO",
            Self::InvalidData(_) => "E_INVALID_DATA",
            Self::InvalidRequest(_) => "E_INVALID_REQUEST",
            Self::NotFound(_) => "E_NOT_FOUND",
            Self::CapabilityUnavailable(_) => "E_CAPABILITY",
            Self::BudgetExceeded { .. } => "E_BUDGET",
            Self::Bql(diagnostic) => diagnostic.code,
        }
    }

    #[must_use]
    pub fn diagnostic(&self) -> Option<&BqlDiagnostic> {
        match self {
            Self::Bql(diagnostic) => Some(diagnostic),
            _ => None,
        }
    }
}

impl fmt::Display for QueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::InvalidData(message) => {
                write!(formatter, "invalid observability data: {message}")
            }
            Self::InvalidRequest(message) => write!(formatter, "invalid query request: {message}"),
            Self::NotFound(message) => write!(formatter, "observability data not found: {message}"),
            Self::CapabilityUnavailable(message) => {
                write!(formatter, "observability capability unavailable: {message}")
            }
            Self::BudgetExceeded {
                required,
                max_bytes,
            } => write!(
                formatter,
                "response requires {required} bytes, exceeding max_bytes={max_bytes}"
            ),
            Self::Bql(diagnostic) => {
                let width = diagnostic.end.saturating_sub(diagnostic.start).max(1);
                writeln!(
                    formatter,
                    "{} at {}:{}: {}",
                    diagnostic.code, diagnostic.line, diagnostic.column, diagnostic.message
                )?;
                writeln!(formatter, "{}", diagnostic.source_line)?;
                writeln!(
                    formatter,
                    "{}{}",
                    " ".repeat(diagnostic.column.saturating_sub(1)),
                    "^".repeat(width.min(diagnostic.source_line.len().max(1)))
                )?;
                if let Some(correction) = &diagnostic.correction {
                    writeln!(formatter, "try: {correction}")?;
                }
                if !diagnostic.valid.is_empty() {
                    write!(formatter, "valid: {}", diagnostic.valid.join(", "))?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for QueryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidData(_)
            | Self::InvalidRequest(_)
            | Self::NotFound(_)
            | Self::CapabilityUnavailable(_)
            | Self::BudgetExceeded { .. }
            | Self::Bql(_) => None,
        }
    }
}

impl From<io::Error> for QueryError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
