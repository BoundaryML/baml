// Keep these protocol constants in sync with the VS Code extension:
// - typescript2/app-vscode-ext/src/compat.ts mirrors the BAML_* protocol ranges.
// - typescript2/app-vscode-ext/src/extension.ts mirrors CAPABILITIES in initializationOptions.
// TODO: generate both sides from one source if these start changing often.
pub(super) const BAML_LSP_PROTOCOL_VERSION: u32 = 1;
pub(super) const BAML_PLAYGROUND_PROTOCOL_VERSION: u32 = 1;
pub(super) const MIN_SUPPORTED_VSCODE_LSP_PROTOCOL: u32 = 1;
pub(super) const MIN_SUPPORTED_PLAYGROUND_PROTOCOL: u32 = 1;

pub(super) const CAPABILITIES: &[&str] = &[
    "openPlayground.v1",
    "listProjects.v1",
    "playgroundWebSocket.v1",
];
