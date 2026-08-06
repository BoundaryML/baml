//! WASM host for the same protobuf tooling.v1 endpoint as napi (API stub).

use baml_tooling_api::ToolingProtocol;
use wasm_bindgen::prelude::*;

/// Per-instance tooling protocol host, mirroring the napi
/// `BamlToolingBridge`. Each constructed bridge owns its sessions, so two
/// clients serving the same project root (client + SSR environments, two
/// projects in one test process) never observe each other's overlays — the
/// same isolation the native backend provides.
#[wasm_bindgen]
pub struct BamlToolingBridge {
    #[allow(dead_code)]
    protocol: ToolingProtocol,
}

#[wasm_bindgen]
impl BamlToolingBridge {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            protocol: ToolingProtocol::default(),
        }
    }

    /// Dispatch an encoded `baml.tooling.v1.ToolingRequest`.
    ///
    /// Browser callers provide the virtual filesystem by including complete
    /// file snapshots in `ProjectOpen` and `ProjectUpdate`; the compiler
    /// never reads or parses source in TypeScript.
    #[wasm_bindgen(js_name = dispatch)]
    pub fn dispatch(&mut self, request: &[u8]) -> Vec<u8> {
        let _ = request;
        todo!("implemented in the bridge-sessions commit")
    }
}

impl Default for BamlToolingBridge {
    fn default() -> Self {
        Self::new()
    }
}
