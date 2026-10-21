//! Execution context is section-local. No state or lookup table is required
//! to associate events with a context, including when earlier files are missing.

use btel_recorder::proto::{ThreadSection, thread_section::Context};
use btel_snapshot::CasId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextReference {
    Unavailable,
    Empty,
    Snapshot(CasId),
    /// A selected empty marker must contain true, not false.
    Invalid,
}

/// Snapshot references can be resolved using the ordinary `CasStore::load`.
/// A reference is not evidence that its blob has been delivered.
pub fn reference(section: &ThreadSection) -> ContextReference {
    match section.context {
        None => ContextReference::Unavailable,
        Some(Context::EmptyContext(true)) => ContextReference::Empty,
        Some(Context::EmptyContext(false)) => ContextReference::Invalid,
        Some(Context::ContextCasId(id)) => ContextReference::Snapshot(id.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_empty_invalid_and_zero_hash_remain_distinct() {
        for (context, expected) in [
            (None, ContextReference::Unavailable),
            (Some(Context::EmptyContext(true)), ContextReference::Empty),
            (
                Some(Context::EmptyContext(false)),
                ContextReference::Invalid,
            ),
            (
                Some(Context::ContextCasId(btel_recorder::proto::CasId::default())),
                ContextReference::Snapshot(CasId::from_bytes([0; 16])),
            ),
        ] {
            assert_eq!(
                reference(&ThreadSection {
                    context,
                    ..Default::default()
                }),
                expected
            );
        }
    }
}
