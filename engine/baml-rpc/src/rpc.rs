use serde::{de::DeserializeOwned, Serialize};

/// Trait for GET endpoints (no request body).
pub trait GetEndpoint {
    type Response: DeserializeOwned;
    const PATH: &'static str;

    /// Returns the endpoint path (e.g., "/users/42").
    fn path(&self) -> String {
        debug_assert!(Self::PATH.starts_with('/'));
        Self::PATH.to_string()
    }
}

/// Trait for POST endpoints that have an associated request body and response.
pub trait ApiEndpoint {
    type Request<'a>: Serialize;
    type Response<'a>: DeserializeOwned;

    const PATH: &'static str;

    /// Returns the endpoint path (e.g., "/users/42").
    fn path() -> &'static str {
        debug_assert!(Self::PATH.starts_with('/'));
        Self::PATH
    }
}
