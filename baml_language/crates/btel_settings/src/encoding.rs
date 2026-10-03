//! Bounds for the current protobuf encoder, not independently tunable tags.
//! Field numbers and enum values remain defined by the recording .proto schema.
/// **Fixed bound.** Must cover containers, selector and the largest scalar event. Shrinking
/// this does not shrink records; validate worst-case encodings first.
/// Variable-length log names require additional reservation beyond this bound.
/// Worst case: two six-byte containers, an eleven-byte thread selector,
/// a twenty-byte context reference, and a ninety-two-byte completion.
pub const MAX_EVENT_BYTES: usize = 135;
/// **Correctness bound.** The specialized writer relies on this reserved region. Update only
/// with encoder changes and worst-case/equivalence tests.
pub const MAX_COMPLETION_BYTES: usize = 92;
/// **Format limit.** Length backpatching uses u32; changing the cap requires reviewing that
/// representation and allocation checks.
pub const MAX_BUFFER_BYTES: usize = u32::MAX as usize;
/// **Format contract.** Reserved u32 varint length width. Changing it alters backpatch
/// offsets and requires encoder/decoder validation.
pub const LENGTH_BYTES: usize = 5;
/// **Wire compatibility.** Coordinate with readers/schema evolution; never a performance
/// knob.
pub const FORMAT_MAJOR: u32 = 2;
/// **Wire compatibility.** Coordinate with readers/schema evolution; never a performance
/// knob. 1 adds `FunctionMetadata.argument_layout`; 2 adds
/// `FunctionMetadata.source_map` and `RecordingFile.errors`; 3 adds process identity, panic
/// flags, future names, sysop time, model usage and `baml.errors.Context` error values.
/// Readers accept any minor.
pub const FORMAT_MINOR: u32 = 3;
/// Files that use execution-context sections opt into this additive version.
/// Recordings without context retain their existing bytes and minor version.
pub const CONTEXT_FORMAT_MINOR: u32 = 4;
/// Structured logs share span sections and the existing CAS encoding.
pub const LOG_FORMAT_MINOR: u32 = 5;
/// Spawned futures record when their body began running (`ThreadRunning`),
/// apart from when they were scheduled.
pub const THREAD_RUNNING_FORMAT_MINOR: u32 = 6;
/// HTTP requests recorded as network spans: `NetworkAnnouncement`,
/// `NetworkEvent` and `NetworkCompletion`.
pub const NETWORK_FORMAT_MINOR: u32 = 7;
/// A generic function's span announcement names the type arguments it was
/// called with (`FunctionAnnouncement.type_args_cas_id`).
pub const TYPE_ARGS_FORMAT_MINOR: u32 = 8;
/// Process headers capture immutable launch context independently of spans.
pub const PROCESS_CONTEXT_FORMAT_MINOR: u32 = 9;
