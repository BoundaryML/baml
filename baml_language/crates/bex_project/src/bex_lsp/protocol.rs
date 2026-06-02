pub(super) const BAML_LSP_PROTOCOL_VERSION: u32 = 1;
pub(super) const BAML_PLAYGROUND_PROTOCOL_VERSION: u32 = 1;
pub(super) const MIN_SUPPORTED_VSCODE_LSP_PROTOCOL: u32 = 1;
pub(super) const MIN_SUPPORTED_PLAYGROUND_PROTOCOL: u32 = 1;

pub(super) const CAPABILITIES: &[&str] = &[
    "openPlayground.v1",
    "listProjects.v1",
    "playgroundWebSocket.v1",
];
