//! Protobuf dispatcher for the versioned tooling protocol (API stub).

use crate::ToolingWorkspace;

/// Session-owning `baml.tooling.v1` endpoint shared by the native and WASM
/// hosts. Each instance owns its sessions, so same-root projects served by
/// two clients never observe each other's overlays.
#[derive(Default)]
pub struct ToolingProtocol {
    #[allow(dead_code)]
    workspace: ToolingWorkspace,
}

impl ToolingProtocol {
    /// Dispatch one encoded `baml.tooling.v1.ToolingRequest` and return the
    /// encoded `ToolingResponse`.
    pub fn dispatch(&mut self, request_bytes: &[u8]) -> Vec<u8> {
        let _ = request_bytes;
        todo!("implemented in the bridge-sessions commit")
    }
}
