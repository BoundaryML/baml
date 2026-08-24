//! Program source-content identity.
//!
//! Lives in the pure data layer beside [`crate::Program`]'s
//! `source_content_hash` field: the compiler stamps it at compile time
//! and hosts restamp it when materializing a program from stored bytes
//! (the field is `borsh(skip)` — in-memory metadata, not compiled
//! content).

/// The conservative source-content program hash (profiling streams spec
/// §2.3):
///
/// ```text
/// SHA-256( "baml-program-v1"
///        ‖ baml_version::CANONICAL_VERSION (utf8)
///        ‖ for each file, sorted by path bytes ascending:
///            path (utf8) ‖ 0x00 ‖ file bytes ‖ 0x00 )
/// ```
///
/// Any byte change in any compiled file — comments and whitespace included —
/// or a compiler version change yields a new hash. Two byte-identical builds
/// in different processes produce identical hashes, so profiling
/// `ContextKey`s are comparable across executions of one build. A later
/// semantic hash must use a NEW domain string, never `"baml-program-v1"`.
#[must_use]
pub fn program_content_hash<'a>(files: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut sorted: Vec<(&str, &[u8])> = files.into_iter().collect();
    sorted.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let mut hasher = Sha256::new();
    hasher.update(b"baml-program-v1");
    hasher.update(baml_version::CANONICAL_VERSION.as_bytes());
    for (path, bytes) in sorted {
        hasher.update(path.as_bytes());
        hasher.update([0u8]);
        hasher.update(bytes);
        hasher.update([0u8]);
    }
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_content_hash_is_order_independent_and_byte_sensitive() {
        let base = program_content_hash([
            ("b.baml", b"beta".as_slice()),
            ("a.baml", b"alpha".as_slice()),
        ]);
        let reordered = program_content_hash([
            ("a.baml", b"alpha".as_slice()),
            ("b.baml", b"beta".as_slice()),
        ]);
        assert_eq!(base, reordered, "input order must not matter");
        let comment_flip = program_content_hash([
            ("a.baml", b"alpha ".as_slice()),
            ("b.baml", b"beta".as_slice()),
        ]);
        assert_ne!(base, comment_flip, "any byte change splits the identity");
        let path_flip = program_content_hash([
            ("a2.baml", b"alpha".as_slice()),
            ("b.baml", b"beta".as_slice()),
        ]);
        assert_ne!(base, path_flip, "path bytes are part of the identity");
        // Cross-platform golden for the formula (pinned to CANONICAL_VERSION
        // "0.17.0"; regenerate deliberately on a version bump).
        if baml_version::CANONICAL_VERSION == "0.17.0" {
            assert_eq!(
                hex::encode(base),
                "60a583a5e76288bf796db866499c4d494c7f1a211490250d98b37b6ec5c5aebd"
            );
        }
    }
}
