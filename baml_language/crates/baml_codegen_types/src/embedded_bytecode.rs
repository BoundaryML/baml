//! The bytecode payload that generated SDKs embed in source.

use base64::{Engine as _, engine::general_purpose::STANDARD};

/// Encode compiled BAML bytecode for embedding as a single-line string
/// literal: an LZ4 frame, then standard padded base64. The generated SDK
/// base64-decodes it with its language's standard library and hands the
/// compressed bytes to the bridge, which decompresses them at startup.
///
/// Empty bytecode stays empty so the bridge still reports a missing payload
/// rather than a corrupt one.
pub fn embedded_bytecode_base64(bytecode: &[u8]) -> String {
    if bytecode.is_empty() {
        return String::new();
    }
    STANDARD.encode(baml_artifact::compress_for_embedding(bytecode))
}
