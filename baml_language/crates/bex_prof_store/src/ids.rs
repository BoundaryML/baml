//! BEX runtime identity: the quad-scoped local IDs and their reversible,
//! versioned string encodings.
//!
//! # Scoping model
//!
//! Events carry *local* IDs only; global identity is the quad
//! `(process_euid, engine_id, thread_id, call_id)`:
//!
//! - [`BexThreadId`] is allocated per engine (restarts at 1 for each
//!   [`EngineId`]); [`BexCallId`] is allocated per thread (restarts at 1 for
//!   each thread, the root call is always 1).
//! - The process/engine half lives once per artifact in the file/batch
//!   header (`pb::EventFileHeaderV1`), never on events.
//!
//! # Encodings
//!
//! [`ThreadRef`] / [`CallRef`] / [`BoundaryId`] / [`RuntimeId`] encode to prefixed,
//! versioned, base64url strings (`baml_thread_1_…`, `baml_call_1_…`,
//! `baml_id_1_…`) and decode losslessly back; decoding validates prefix,
//! base64, payload length, and version, and never panics on malformed
//! input. The typed decoders reject each other's prefixes.

use std::{fmt, sync::OnceLock};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

const CALL_REF_PREFIX: &str = "baml_call_1_";
const THREAD_REF_PREFIX: &str = "baml_thread_1_";
const BOUNDARY_ID_PREFIX: &str = "baml_id_1_";
const PAYLOAD_VERSION: u8 = 1;
const CALL_REF_LEN: usize = 1 + 16 + 8 + 8 + 8;
const THREAD_REF_LEN: usize = 1 + 16 + 8 + 8;
const BOUNDARY_ID_LEN: usize = 16;

static PROCESS_EUID: OnceLock<ProcessEuid> = OnceLock::new();

/// Effectively-unique process identifier, minted once per process
/// ([`ProcessEuid::current`]) — the outermost scope of the identity quad.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProcessEuid(pub [u8; 16]);

impl ProcessEuid {
    #[must_use]
    pub fn new_random() -> Self {
        Self(*uuid::Uuid::new_v4().as_bytes())
    }

    #[must_use]
    pub fn current() -> Self {
        *PROCESS_EUID.get_or_init(ProcessEuid::new_random)
    }

    /// Returns the process identity only when some identity-bearing operation
    /// has already materialized it.
    ///
    /// Engine construction deliberately does not call [`Self::current`]. The
    /// process-global [`OnceLock`] serializes concurrent first calls, so every
    /// engine and request observes the same cryptographically generated value.
    #[doc(hidden)]
    #[must_use]
    pub fn current_if_initialized() -> Option<Self> {
        PROCESS_EUID.get().copied()
    }
}

/// Process-local engine counter (starts at 1). Two engines in one process
/// always get distinct ids; the id is only meaningful together with the
/// [`ProcessEuid`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EngineId(pub u64);

/// Identity of a compiled program. Currently random per engine construction
/// — good enough for event-file-local joins, NOT a durable content identity
/// (identical programs in two engines get different ids).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProgramId(pub [u8; 16]);

impl ProgramId {
    #[must_use]
    pub fn new_random() -> Self {
        Self(*uuid::Uuid::new_v4().as_bytes())
    }
}

/// Identity of the source snapshot a program was compiled from. Modeled for
/// the metadata join path but not yet populated by the runtime — consumers
/// must tolerate `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceSnapshotId(pub [u8; 32]);

/// Engine-local BEX thread id (starts at 1 per engine; the root call's
/// thread first, spawned children after).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BexThreadId(pub u64);

/// Index into the engine's function metadata table
/// (`bex_events::FunctionMetadataTable`) — 1-based sequential in object-pool
/// walk order (0 = unassigned). NOT stable across recompiles: joins are
/// valid only against the same artifact's header (cross-run joins use the
/// FQN). Synthetic rows (spawn-closure, unknown-function) sit just past
/// the real functions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FunctionId(pub u32);

/// Thread-local call counter (starts at 1 per thread; the thread's root
/// call is always 1). Minted for every bytecode call whether or not an
/// event sink is configured — `$id` depends on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BexCallId(pub u64);

/// Fully-scoped thread identity (the quad minus `call_id`); encodes to a
/// reversible `baml_thread_1_…` string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ThreadRef {
    pub process_euid: ProcessEuid,
    pub engine_id: EngineId,
    pub thread_id: BexThreadId,
}

/// An execution's identity: its root (parentless) logical thread. Wire form
/// is the `ThreadRef` wire form (`baml_thread_1_…`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExecutionId(pub ThreadRef);

impl ExecutionId {
    #[must_use]
    pub fn encode(&self) -> String {
        self.0.encode()
    }

    pub fn decode(value: &str) -> Result<Self, DecodeError> {
        ThreadRef::decode(value).map(Self)
    }
}

/// Fully-scoped call identity (the complete quad); encodes to a reversible
/// `baml_call_1_…` string — the default value of `$id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CallRef {
    pub process_euid: ProcessEuid,
    pub engine_id: EngineId,
    pub thread_id: BexThreadId,
    pub call_id: BexCallId,
}

/// Public execution boundary identity, allocated by the host before BEX
/// execution starts. It intentionally keeps the existing `baml_id_1_` raw
/// 16-byte encoding so runtime-id overrides and boundary history share one
/// typed payload shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BoundaryId([u8; 16]);

/// The value of `$id`: either the call's default [`CallRef`], or the active
/// execution [`BoundaryId`] / explicit child boundary (`baml_id_1_…`). A
/// boundary override applies to exactly one call; without a runtime-ID
/// annotation, `$id` is the [`CallRef`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RuntimeId {
    DefaultCall(CallRef),
    Boundary(BoundaryId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    InvalidPrefix,
    InvalidBase64,
    InvalidLength { expected: usize, actual: usize },
    UnsupportedVersion(u8),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPrefix => write!(f, "invalid BAML runtime ID prefix"),
            Self::InvalidBase64 => write!(f, "invalid BAML runtime ID base64 payload"),
            Self::InvalidLength { expected, actual } => {
                write!(
                    f,
                    "invalid BAML runtime ID payload length: expected {expected}, got {actual}"
                )
            }
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported BAML runtime ID version {version}")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

impl ThreadRef {
    #[must_use]
    pub fn encode(&self) -> String {
        let mut payload = Vec::with_capacity(THREAD_REF_LEN);
        payload.push(PAYLOAD_VERSION);
        payload.extend_from_slice(&self.process_euid.0);
        payload.extend_from_slice(&self.engine_id.0.to_be_bytes());
        payload.extend_from_slice(&self.thread_id.0.to_be_bytes());
        format!("{THREAD_REF_PREFIX}{}", URL_SAFE_NO_PAD.encode(payload))
    }

    pub fn decode(s: &str) -> Result<Self, DecodeError> {
        let payload = decode_payload(s, THREAD_REF_PREFIX, THREAD_REF_LEN)?;
        Ok(Self {
            process_euid: ProcessEuid(payload[1..17].try_into().expect("fixed-width slice")),
            engine_id: EngineId(u64::from_be_bytes(
                payload[17..25].try_into().expect("fixed-width slice"),
            )),
            thread_id: BexThreadId(u64::from_be_bytes(
                payload[25..33].try_into().expect("fixed-width slice"),
            )),
        })
    }
}

impl CallRef {
    #[must_use]
    pub fn encode(&self) -> String {
        let mut payload = Vec::with_capacity(CALL_REF_LEN);
        payload.push(PAYLOAD_VERSION);
        payload.extend_from_slice(&self.process_euid.0);
        payload.extend_from_slice(&self.engine_id.0.to_be_bytes());
        payload.extend_from_slice(&self.thread_id.0.to_be_bytes());
        payload.extend_from_slice(&self.call_id.0.to_be_bytes());
        format!("{CALL_REF_PREFIX}{}", URL_SAFE_NO_PAD.encode(payload))
    }

    pub fn decode(s: &str) -> Result<Self, DecodeError> {
        let payload = decode_payload(s, CALL_REF_PREFIX, CALL_REF_LEN)?;
        Ok(Self {
            process_euid: ProcessEuid(payload[1..17].try_into().expect("fixed-width slice")),
            engine_id: EngineId(u64::from_be_bytes(
                payload[17..25].try_into().expect("fixed-width slice"),
            )),
            thread_id: BexThreadId(u64::from_be_bytes(
                payload[25..33].try_into().expect("fixed-width slice"),
            )),
            call_id: BexCallId(u64::from_be_bytes(
                payload[33..41].try_into().expect("fixed-width slice"),
            )),
        })
    }
}

impl BoundaryId {
    #[must_use]
    pub fn new_random() -> Self {
        Self(*uuid::Uuid::new_v4().as_bytes())
    }

    #[must_use]
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn as_bytes(self) -> [u8; 16] {
        self.0
    }

    #[must_use]
    pub fn to_wire_string(self) -> String {
        self.encode()
    }

    #[must_use]
    pub fn from_wire_str(value: &str) -> Option<Self> {
        Self::decode(value).ok()
    }

    #[must_use]
    pub fn encode(self) -> String {
        format!("{BOUNDARY_ID_PREFIX}{}", URL_SAFE_NO_PAD.encode(self.0))
    }

    pub fn decode(s: &str) -> Result<Self, DecodeError> {
        let encoded = s
            .strip_prefix(BOUNDARY_ID_PREFIX)
            .ok_or(DecodeError::InvalidPrefix)?;
        let payload = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| DecodeError::InvalidBase64)?;
        if payload.len() != BOUNDARY_ID_LEN {
            return Err(DecodeError::InvalidLength {
                expected: BOUNDARY_ID_LEN,
                actual: payload.len(),
            });
        }
        Ok(Self(
            payload.as_slice().try_into().expect("fixed-width slice"),
        ))
    }
}

impl RuntimeId {
    #[must_use]
    pub fn encode(&self) -> String {
        match self {
            Self::DefaultCall(call_ref) => call_ref.encode(),
            Self::Boundary(boundary_id) => boundary_id.encode(),
        }
    }

    pub fn decode(s: &str) -> Result<Self, DecodeError> {
        if s.starts_with(CALL_REF_PREFIX) {
            return CallRef::decode(s).map(Self::DefaultCall);
        }
        BoundaryId::decode(s).map(Self::Boundary)
    }
}

fn decode_payload(s: &str, prefix: &str, expected_len: usize) -> Result<Vec<u8>, DecodeError> {
    let encoded = s.strip_prefix(prefix).ok_or(DecodeError::InvalidPrefix)?;
    let payload = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| DecodeError::InvalidBase64)?;
    if payload.len() != expected_len {
        return Err(DecodeError::InvalidLength {
            expected: expected_len,
            actual: payload.len(),
        });
    }
    if payload[0] != PAYLOAD_VERSION {
        return Err(DecodeError::UnsupportedVersion(payload[0]));
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_call_ref() -> CallRef {
        CallRef {
            process_euid: ProcessEuid([7; 16]),
            engine_id: EngineId(2),
            thread_id: BexThreadId(3),
            call_id: BexCallId(4),
        }
    }

    #[test]
    fn call_ref_round_trips() {
        let call_ref = sample_call_ref();
        assert_eq!(CallRef::decode(&call_ref.encode()).unwrap(), call_ref);
    }

    #[test]
    fn thread_ref_round_trips() {
        let thread_ref = ThreadRef {
            process_euid: ProcessEuid([9; 16]),
            engine_id: EngineId(10),
            thread_id: BexThreadId(11),
        };
        assert_eq!(ThreadRef::decode(&thread_ref.encode()).unwrap(), thread_ref);
    }

    #[test]
    fn boundary_id_round_trips_with_existing_wire_shape() {
        let boundary_id = BoundaryId::from_bytes([11; 16]);
        assert_eq!(
            boundary_id.to_wire_string(),
            "baml_id_1_CwsLCwsLCwsLCwsLCwsLCw"
        );
        assert_eq!(
            BoundaryId::decode(&boundary_id.encode()).unwrap(),
            boundary_id
        );
        assert_eq!(
            BoundaryId::from_wire_str(&boundary_id.to_wire_string()),
            Some(boundary_id)
        );
    }

    #[test]
    fn runtime_ids_round_trip_default_and_boundary_forms() {
        let default = RuntimeId::DefaultCall(sample_call_ref());
        assert_eq!(RuntimeId::decode(&default.encode()).unwrap(), default);

        let override_id = RuntimeId::Boundary(BoundaryId::from_bytes([11; 16]));
        assert_eq!(
            RuntimeId::decode(&override_id.encode()).unwrap(),
            override_id
        );
    }

    #[test]
    fn each_call_ref_component_affects_encoding() {
        let base = sample_call_ref();
        let encoded = base.encode();
        assert_ne!(
            CallRef {
                process_euid: ProcessEuid([8; 16]),
                ..base
            }
            .encode(),
            encoded
        );
        assert_ne!(
            CallRef {
                engine_id: EngineId(5),
                ..base
            }
            .encode(),
            encoded
        );
        assert_ne!(
            CallRef {
                thread_id: BexThreadId(5),
                ..base
            }
            .encode(),
            encoded
        );
        assert_ne!(
            CallRef {
                call_id: BexCallId(5),
                ..base
            }
            .encode(),
            encoded
        );
    }

    #[test]
    fn malformed_ids_fail_cleanly() {
        assert_eq!(CallRef::decode("bad"), Err(DecodeError::InvalidPrefix));
        assert!(matches!(
            CallRef::decode("baml_call_1_!"),
            Err(DecodeError::InvalidBase64)
        ));

        let mut payload = vec![PAYLOAD_VERSION; CALL_REF_LEN];
        payload[0] = 99;
        let encoded = format!("{CALL_REF_PREFIX}{}", URL_SAFE_NO_PAD.encode(payload));
        assert_eq!(
            CallRef::decode(&encoded),
            Err(DecodeError::UnsupportedVersion(99))
        );
    }

    #[test]
    fn malformed_thread_refs_fail_cleanly() {
        assert_eq!(ThreadRef::decode("bad"), Err(DecodeError::InvalidPrefix));
        assert!(matches!(
            ThreadRef::decode("baml_thread_1_!"),
            Err(DecodeError::InvalidBase64)
        ));

        let mut payload = vec![PAYLOAD_VERSION; THREAD_REF_LEN];
        payload[0] = 99;
        let encoded = format!("{THREAD_REF_PREFIX}{}", URL_SAFE_NO_PAD.encode(payload));
        assert_eq!(
            ThreadRef::decode(&encoded),
            Err(DecodeError::UnsupportedVersion(99))
        );
    }

    #[test]
    fn truncated_payloads_fail_with_invalid_length() {
        let truncated = vec![PAYLOAD_VERSION; CALL_REF_LEN - 1];
        let encoded = format!("{CALL_REF_PREFIX}{}", URL_SAFE_NO_PAD.encode(truncated));
        assert_eq!(
            CallRef::decode(&encoded),
            Err(DecodeError::InvalidLength {
                expected: CALL_REF_LEN,
                actual: CALL_REF_LEN - 1,
            })
        );

        let truncated = vec![PAYLOAD_VERSION; THREAD_REF_LEN - 1];
        let encoded = format!("{THREAD_REF_PREFIX}{}", URL_SAFE_NO_PAD.encode(truncated));
        assert_eq!(
            ThreadRef::decode(&encoded),
            Err(DecodeError::InvalidLength {
                expected: THREAD_REF_LEN,
                actual: THREAD_REF_LEN - 1,
            })
        );

        // Boundary payloads are raw 16 bytes (no version byte).
        let short = vec![0u8; BOUNDARY_ID_LEN - 1];
        let encoded = format!("{BOUNDARY_ID_PREFIX}{}", URL_SAFE_NO_PAD.encode(short));
        assert_eq!(
            RuntimeId::decode(&encoded),
            Err(DecodeError::InvalidLength {
                expected: BOUNDARY_ID_LEN,
                actual: BOUNDARY_ID_LEN - 1,
            })
        );
    }

    #[test]
    fn cross_type_prefixes_are_rejected() {
        let thread_ref = ThreadRef {
            process_euid: ProcessEuid([9; 16]),
            engine_id: EngineId(10),
            thread_id: BexThreadId(11),
        };
        // A thread ref is not a runtime id (only call refs and boundary ids are).
        assert_eq!(
            RuntimeId::decode(&thread_ref.encode()),
            Err(DecodeError::InvalidPrefix)
        );
        // And the typed decoders reject each other's prefixes.
        assert_eq!(
            CallRef::decode(&thread_ref.encode()),
            Err(DecodeError::InvalidPrefix)
        );
        assert_eq!(
            ThreadRef::decode(&sample_call_ref().encode()),
            Err(DecodeError::InvalidPrefix)
        );
    }
}
