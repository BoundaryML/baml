//! In-process napi-rs host for the versioned BAML tooling protocol.

use std::sync::Mutex;

use baml_tooling_api::ToolingProtocol;
use napi::bindgen_prelude::Buffer;
use napi_derive::napi;

#[napi]
pub struct BamlToolingBridge {
    protocol: Mutex<ToolingProtocol>,
}

#[napi]
impl BamlToolingBridge {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            protocol: Mutex::new(ToolingProtocol::default()),
        }
    }

    /// Dispatch one encoded `baml.tooling.v1.ToolingRequest` and return the
    /// encoded `ToolingResponse`. Hot-path reads are synchronous by design.
    #[napi]
    #[allow(clippy::needless_pass_by_value)] // napi-rs owns JS Buffer arguments.
    pub fn dispatch(&self, request: Buffer) -> napi::Result<Buffer> {
        let mut protocol = self
            .protocol
            .lock()
            .map_err(|_| napi::Error::from_reason("BAML tooling protocol lock was poisoned"))?;
        Ok(protocol.dispatch(request.as_ref()).into())
    }
}

impl Default for BamlToolingBridge {
    fn default() -> Self {
        Self::new()
    }
}
