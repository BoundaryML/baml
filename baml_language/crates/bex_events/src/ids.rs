use std::{fmt, sync::OnceLock};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

const CALL_REF_PREFIX: &str = "baml_call_1_";
const THREAD_REF_PREFIX: &str = "baml_thread_1_";
const OVERRIDE_ID_PREFIX: &str = "baml_id_1_";
const PAYLOAD_VERSION: u8 = 1;
const CALL_REF_LEN: usize = 1 + 16 + 8 + 8 + 8;
const THREAD_REF_LEN: usize = 1 + 16 + 8 + 8;
const OVERRIDE_ID_LEN: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProcessEuid(pub [u8; 16]);

impl ProcessEuid {
    #[must_use]
    pub fn new_random() -> Self {
        Self(*uuid::Uuid::new_v4().as_bytes())
    }

    #[must_use]
    pub fn current() -> Self {
        static PROCESS_EUID: OnceLock<ProcessEuid> = OnceLock::new();
        *PROCESS_EUID.get_or_init(ProcessEuid::new_random)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EngineId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProgramId(pub [u8; 16]);

impl ProgramId {
    #[must_use]
    pub fn new_random() -> Self {
        Self(*uuid::Uuid::new_v4().as_bytes())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceSnapshotId(pub [u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BexThreadId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FunctionId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CallId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ThreadRef {
    pub process_euid: ProcessEuid,
    pub engine_id: EngineId,
    pub thread_id: BexThreadId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CallRef {
    pub process_euid: ProcessEuid,
    pub engine_id: EngineId,
    pub thread_id: BexThreadId,
    pub call_id: CallId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RuntimeId {
    DefaultCall(CallRef),
    OverrideUuid([u8; 16]),
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
            call_id: CallId(u64::from_be_bytes(
                payload[33..41].try_into().expect("fixed-width slice"),
            )),
        })
    }
}

impl RuntimeId {
    #[must_use]
    pub fn encode(&self) -> String {
        match self {
            Self::DefaultCall(call_ref) => call_ref.encode(),
            Self::OverrideUuid(id) => {
                format!("{OVERRIDE_ID_PREFIX}{}", URL_SAFE_NO_PAD.encode(id))
            }
        }
    }

    pub fn decode(s: &str) -> Result<Self, DecodeError> {
        if s.starts_with(CALL_REF_PREFIX) {
            return CallRef::decode(s).map(Self::DefaultCall);
        }

        let encoded = s
            .strip_prefix(OVERRIDE_ID_PREFIX)
            .ok_or(DecodeError::InvalidPrefix)?;
        let payload = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| DecodeError::InvalidBase64)?;
        if payload.len() != OVERRIDE_ID_LEN {
            return Err(DecodeError::InvalidLength {
                expected: OVERRIDE_ID_LEN,
                actual: payload.len(),
            });
        }

        Ok(Self::OverrideUuid(
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

    fn sample_call_ref() -> CallRef {
        CallRef {
            process_euid: ProcessEuid([7; 16]),
            engine_id: EngineId(2),
            thread_id: BexThreadId(3),
            call_id: CallId(4),
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
    fn runtime_ids_round_trip_default_and_override_forms() {
        let default = RuntimeId::DefaultCall(sample_call_ref());
        assert_eq!(RuntimeId::decode(&default.encode()).unwrap(), default);

        let override_id = RuntimeId::OverrideUuid([11; 16]);
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
                call_id: CallId(5),
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
}
