//! Bounds a writer's buffers take from the recording format. The format itself, its field
//! numbers, its minors and the size of each event, is `btel-core`'s (`btel_core::recording`).

/// **Format limit.** A container's length is a padded `u32` varint
/// (`btel_core::recording::frame::LENGTH_BYTES`), so a buffer it frames holds at most this many
/// bytes; changing the cap requires reviewing that representation and allocation checks.
pub const MAX_BUFFER_BYTES: usize = u32::MAX as usize;
