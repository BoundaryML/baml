//! Typed query errors (design 04-query-system, 06-studio-experience).
//!
//! **E_BACKEND_CAPABILITY** and **E_QUERY_BUDGET_EXCEEDED** are
//! decision-frozen codes; the remaining spellings freeze with the v1
//! error schema. Errors carry a stable code, a human message, and
//! retryability — never secrets, presigned URLs, raw customer bodies, or
//! private physical SQL.

use std::fmt;

/// Stable machine-readable error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryErrorCode {
    /// Statement is not part of the supported grammar (DDL/DML, CREATE
    /// FUNCTION, plugins, raw-dialect passthrough, multiple statements).
    InvalidSql,
    /// A function/operator requires a backend the bound session does not
    /// have. Decision-frozen spelling: `E_BACKEND_CAPABILITY` (D4).
    BackendCapability,
    /// A query-global budget was exhausted. Decision-frozen spelling:
    /// `E_QUERY_BUDGET_EXCEEDED`. Rows already streamed are explicitly
    /// incomplete.
    BudgetExceeded,
    /// The query was cancelled.
    Cancelled,
    /// The bound scope denies this query outright (missing value-read
    /// permission fails before execution).
    AuthorizationDenied,
    /// A provider/store dependency failed in a retryable way.
    DependencyUnavailable,
    /// Evidence exists but cannot be decoded (corrupt/unsupported).
    ArtifactCorrupt,
    /// Internal invariant failure.
    Internal,
}

impl QueryErrorCode {
    /// The stable wire spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            QueryErrorCode::InvalidSql => "invalid_sql",
            QueryErrorCode::BackendCapability => "E_BACKEND_CAPABILITY",
            QueryErrorCode::BudgetExceeded => "E_QUERY_BUDGET_EXCEEDED",
            QueryErrorCode::Cancelled => "cancelled",
            QueryErrorCode::AuthorizationDenied => "authorization_denied",
            QueryErrorCode::DependencyUnavailable => "dependency_unavailable",
            QueryErrorCode::ArtifactCorrupt => "artifact_corrupt",
            QueryErrorCode::Internal => "internal",
        }
    }

    /// May a caller retry the same query unchanged?
    #[must_use]
    pub fn retryable(self) -> bool {
        matches!(self, QueryErrorCode::DependencyUnavailable)
    }
}

/// One typed query failure.
#[derive(Debug, Clone)]
pub struct QueryError {
    pub code: QueryErrorCode,
    pub message: String,
    /// Optional actionable hint ("use the parameter name: args['customer']").
    pub remedy: Option<String>,
}

impl QueryError {
    #[must_use]
    pub fn new(code: QueryErrorCode, message: impl Into<String>) -> QueryError {
        QueryError {
            code,
            message: message.into(),
            remedy: None,
        }
    }

    #[must_use]
    pub fn with_remedy(mut self, remedy: impl Into<String>) -> QueryError {
        self.remedy = Some(remedy.into());
        self
    }

    #[must_use]
    pub fn invalid_sql(message: impl Into<String>) -> QueryError {
        QueryError::new(QueryErrorCode::InvalidSql, message)
    }

    /// Planning-time backend rejection (D4): named function, required
    /// backend, current backend — before any data read.
    #[must_use]
    pub fn backend_capability(function: &str, required: &str, current: &str) -> QueryError {
        QueryError::new(
            QueryErrorCode::BackendCapability,
            format!(
                "function: {function}\nrequired_backend: {required}\ncurrent_backend: {current}"
            ),
        )
    }
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)?;
        if let Some(remedy) = &self.remedy {
            write!(f, " (remedy: {remedy})")?;
        }
        Ok(())
    }
}

impl std::error::Error for QueryError {}
