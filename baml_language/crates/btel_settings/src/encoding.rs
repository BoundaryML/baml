//! Bounds for the current protobuf encoder, not independently tunable tags.
//! Field numbers and enum values remain defined by the recording .proto schema.
/// **Fixed bound.** Must cover containers, selector and the largest scalar event. Shrinking
/// this does not shrink records; validate worst-case encodings first.
pub const MAX_EVENT_BYTES: usize = 128;
/// **Correctness bound.** The specialized writer relies on this reserved region. Update only
/// with encoder changes and worst-case/equivalence tests.
pub const MAX_COMPLETION_BYTES: usize = 70;
/// **Format limit.** Length backpatching uses u32; changing the cap requires reviewing that
/// representation and allocation checks.
pub const MAX_BUFFER_BYTES: usize = u32::MAX as usize;
/// **Format contract.** Reserved u32 varint length width. Changing it alters backpatch
/// offsets and requires encoder/decoder validation.
pub const LENGTH_BYTES: usize = 5;
/// **Wire compatibility.** Coordinate with readers/schema evolution; never a performance
/// knob.
pub const FORMAT_MAJOR: u32 = 1;
/// **Wire compatibility.** Coordinate with readers/schema evolution; never a performance
/// knob.
pub const FORMAT_MINOR: u32 = 0;
