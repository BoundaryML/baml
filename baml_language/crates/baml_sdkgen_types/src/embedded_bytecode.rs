//! The bytecode payload that generated SDKs embed in source.

/// Encode compiled BAML bytecode for embedding as a single-line string
/// literal (see [`baml_artifact::encode_embedded`]). Generated SDKs hand the
/// string to their bridge's embedded-bytecode initializer unchanged; decoding
/// happens natively, never in the host language.
///
/// Empty bytecode stays empty so the bridge still reports a missing payload
/// rather than a corrupt one.
pub fn embedded_bytecode_base64(bytecode: &[u8]) -> String {
    if bytecode.is_empty() {
        return String::new();
    }
    baml_artifact::encode_embedded(bytecode)
}
