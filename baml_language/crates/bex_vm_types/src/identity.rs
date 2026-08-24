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
///        ‖ framed(baml_version::CANONICAL_VERSION (utf8))
///        ‖ for each file, sorted by path bytes ascending:
///            framed(path (utf8)) ‖ framed(file bytes) )
///
/// where framed(x) = len(x) as u64 big-endian ‖ x
/// ```
///
/// Every variable-length field carries its own length, so no two distinct
/// inputs share a byte stream: a delimiter alone would not do, since path
/// and file bytes may contain any byte value.
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
    let mut framed = |bytes: &[u8]| {
        hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_be_bytes());
        hasher.update(bytes);
    };
    framed(baml_version::CANONICAL_VERSION.as_bytes());
    for (path, bytes) in sorted {
        framed(path.as_bytes());
        framed(bytes);
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
                "e98260b82b3b024bcc7d3b56fee3632a840cc8b05abf9ebcbd28882b16ef3049"
            );
        }
    }

    #[test]
    fn version_and_first_path_cannot_be_confused() {
        use sha2::{Digest, Sha256};

        // Recomputes the documented formula for one file, with the version
        // as a parameter (`CANONICAL_VERSION` is a constant, so the real
        // entry point cannot vary it).
        fn formula(version: &str, path: &str, bytes: &[u8]) -> [u8; 32] {
            let mut hasher = Sha256::new();
            hasher.update(b"baml-program-v1");
            for field in [version.as_bytes(), path.as_bytes(), bytes] {
                hasher.update((field.len() as u64).to_be_bytes());
                hasher.update(field);
            }
            hasher.finalize().into()
        }

        // Ties the formula above to the shipping one: if `program_content_hash`
        // stops framing the version, this equality breaks.
        assert_eq!(
            program_content_hash([("a.baml", b"alpha".as_slice())]),
            formula(baml_version::CANONICAL_VERSION, "a.baml", b"alpha"),
            "the entry point must hash the documented framing"
        );

        // Unframed, "0.17.1" + "0a.baml" and "0.17.10" + "a.baml" concatenate
        // to identical bytes.
        assert_ne!(
            formula("0.17.1", "0a.baml", b"same"),
            formula("0.17.10", "a.baml", b"same"),
            "a version boundary must not be absorbed into the first path"
        );
    }

    #[test]
    fn path_and_content_boundaries_cannot_be_confused() {
        // Length framing (unlike a NUL delimiter) also survives paths and
        // file bytes that themselves contain the delimiter byte.
        assert_ne!(
            program_content_hash([("a\0b.baml", b"c".as_slice())]),
            program_content_hash([("a.baml", b"\0bc".as_slice())]),
            "field boundaries must not depend on the content bytes"
        );
    }
}
