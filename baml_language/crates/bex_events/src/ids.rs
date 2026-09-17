//! Host run, logical thread, and compiled-program identities.

#[cfg(target_arch = "wasm32")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::{fmt, sync::OnceLock};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

const THREAD_REF_PREFIX: &str = "baml_thread_1_";
const BOUNDARY_ID_PREFIX: &str = "baml_id_1_";
const PAYLOAD_VERSION: u8 = 1;
const THREAD_REF_LEN: usize = 1 + 16 + 8 + 8;
const BOUNDARY_ID_LEN: usize = 16;

#[cfg(target_arch = "wasm32")]
// Workerd forbids access to crypto/random while a Worker isolate is starting,
// and it may reuse that isolate for an arbitrary number of requests. The
// workerd-only loader sets this flag before WASM startup so every identity
// minted by that isolate consistently uses the workerd-safe UUID source.
static WORKERD_RUNTIME: AtomicBool = AtomicBool::new(false);

#[cfg(target_arch = "wasm32")]
pub fn configure_workerd_runtime() {
    WORKERD_RUNTIME.store(true, Ordering::Relaxed);
}

#[cfg(any(target_arch = "wasm32", test))]
fn zero_random_uuid_v7(millis: u64) -> uuid::Uuid {
    let mut builder = uuid::Builder::from_unix_timestamp_millis(millis, &[0; 10]);
    builder
        .set_variant(uuid::Variant::RFC4122)
        .set_version(uuid::Version::SortRand);
    builder.into_uuid()
}

fn new_uuid_v7() -> uuid::Uuid {
    let timestamp = uuid::Timestamp::now(uuid::NoContext);

    #[cfg(target_arch = "wasm32")]
    if WORKERD_RUNTIME.load(Ordering::Relaxed) {
        let (seconds, nanos) = timestamp.to_unix();
        let millis = seconds
            .saturating_mul(1_000)
            .saturating_add(u64::from(nanos) / 1_000_000);
        // UUIDs minted in the same millisecond collide because their suffix is
        // zero. This is a known, tolerated temporary compromise; distinctness
        // is intentionally deferred while this path avoids workerd's
        // startup-time restriction on crypto/random access.
        return zero_random_uuid_v7(millis);
    }

    uuid::Uuid::new_v7(timestamp)
}

/// Effectively unique process identifier used as the outermost scope of the
/// thread identity. Workerd uses a temporary zero-random `UUIDv7` implementation
/// that may collide with another process initialized in the same millisecond.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProcessEuid(pub [u8; 16]);

impl ProcessEuid {
    #[must_use]
    pub fn new_random() -> Self {
        Self(*new_uuid_v7().as_bytes())
    }

    #[must_use]
    pub fn current() -> Self {
        static PROCESS_EUID: OnceLock<ProcessEuid> = OnceLock::new();
        *PROCESS_EUID.get_or_init(Self::new_random)
    }
}

/// Process-local engine counter (starts at 1). Two engines in one process
/// always get distinct ids; the id is only meaningful together with the
/// [`ProcessEuid`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EngineId(pub u64);

/// Identity of a compiled program. Workerd uses a temporary zero-random
/// `UUIDv7` implementation that may collide with another program initialized in
/// the same millisecond.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProgramId(pub [u8; 16]);

impl ProgramId {
    #[must_use]
    pub fn new_random() -> Self {
        Self(*new_uuid_v7().as_bytes())
    }
}

/// Source-content hash supplied by the compiler for program metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceSnapshotId(pub [u8; 32]);

/// Engine-local BEX thread id (starts at 1 per engine; the root call's
/// thread first, spawned children after).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BexThreadId(pub u64);

pub use btel_types::FunctionId;

/// A logical thread scoped by its process and engine. Encodes to a
/// reversible `baml_thread_1_…` string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ThreadRef {
    pub process_euid: ProcessEuid,
    pub engine_id: EngineId,
    pub thread_id: BexThreadId,
}

/// Host-created run identity shared by structured logs and run history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BoundaryId([u8; 16]);

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

    const ZERO_RANDOM_UUID_V7_SUFFIX: [u8; 10] = [0x70, 0, 0x80, 0, 0, 0, 0, 0, 0, 0];

    fn assert_uuid_v7(bytes: [u8; 16]) {
        let uuid = uuid::Uuid::from_bytes(bytes);
        assert_eq!(uuid.get_variant(), uuid::Variant::RFC4122);
        assert_eq!(uuid.get_version(), Some(uuid::Version::SortRand));
    }

    #[test]
    fn zero_random_uuid_v7_encodes_the_supplied_timestamp() {
        let uuid = zero_random_uuid_v7(0x0123_4567_89ab);
        assert_eq!(
            uuid.as_bytes(),
            &[
                1, 0x23, 0x45, 0x67, 0x89, 0xab, 0x70, 0, 0x80, 0, 0, 0, 0, 0, 0, 0
            ]
        );
        assert_eq!(uuid.get_variant(), uuid::Variant::RFC4122);
        assert_eq!(uuid.get_version(), Some(uuid::Version::SortRand));
    }

    #[test]
    fn current_process_euid_is_stable() {
        assert_eq!(ProcessEuid::current(), ProcessEuid::current());
    }

    #[test]
    fn process_euid_and_program_id_use_distinct_uuid_v7_values() {
        let process_a = ProcessEuid::new_random();
        let process_b = ProcessEuid::new_random();
        let program_a = ProgramId::new_random();
        let program_b = ProgramId::new_random();

        for bytes in [process_a.0, process_b.0, program_a.0, program_b.0] {
            assert_uuid_v7(bytes);
            assert_ne!(bytes[6..], ZERO_RANDOM_UUID_V7_SUFFIX);
        }
        assert_ne!(process_a, process_b);
        assert_ne!(program_a, program_b);
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
}
