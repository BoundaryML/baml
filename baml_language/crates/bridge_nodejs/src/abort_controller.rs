//! Node.js `AbortController` class for cancelling in-flight BAML function calls.
//! Mirrors bridge_python/src/abort_controller.rs.

use bex_project::CancellationToken;
use napi_derive::napi;

/// An abort controller for cancelling BAML function calls.
#[napi]
pub struct AbortController {
    token: CancellationToken,
}

#[napi]
impl AbortController {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
        }
    }

    #[napi]
    pub fn abort(&self) {
        self.token.cancel();
    }

    #[napi(getter)]
    pub fn aborted(&self) -> bool {
        self.token.is_cancelled()
    }
}

impl AbortController {
    pub(crate) fn token(&self) -> CancellationToken {
        self.token.clone()
    }
}
