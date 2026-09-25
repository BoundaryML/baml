//! Proposed JSON prepare protocol. Digests and recording IDs are lowercase hex.
//! Expirations are milliseconds since the Unix epoch.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PrepareUploadsRequest {
    pub recording: RecordingDescriptor,
    pub candidates: Vec<CasCandidate>,
    pub proposed_uploads: Vec<ProposedUploadTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer_session_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liveness_sequence: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<ProducerState>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecordingDescriptor {
    pub recording_id: String,
    pub recording_file_sequence: u64,
    pub encoded_length: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CasCandidate {
    pub snapshot_id: String,
    pub snapshot_format_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_class: Option<CasSizeClass>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CasSizeClass {
    Large,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UploadKind {
    Recording,
    CasBatch,
    CasObject,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProposedUploadTarget {
    pub client_target_id: u32,
    pub kind: UploadKind,
    pub candidate_indices: Vec<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PrepareUploadsResponse {
    pub plan_id: String,
    pub expires_at_unix_ms: u64,
    pub uploads: Vec<UploadTarget>,
    pub cas: Vec<CasDisposition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat: Option<HeartbeatPolicy>,
}

/// Server policy. Both durations are milliseconds, not Unix timestamps.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HeartbeatPolicy {
    pub interval_ms: u64,
    pub staleness_threshold_ms: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProducerState {
    Running,
    Draining,
    Disabled,
}

/// Minimal JSON heartbeat body, also flattened into every prepare HTTP attempt.
/// These fields are not part of prepare idempotency or immutable upload plans.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Liveness {
    pub producer_session_id: Uuid,
    pub liveness_sequence: u64,
    pub state: ProducerState,
}

// Deliberately no Debug: a presigned URL is a bearer capability.
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UploadTarget {
    pub upload_id: String,
    pub client_target_id: u32,
    pub object_key: String,
    pub presigned_put_url: String,
    pub expires_at_unix_ms: u64,
    pub required_headers: BTreeMap<String, String>,
    pub kind: UploadKind,
    pub candidate_indices: Vec<u32>,
}

impl std::fmt::Debug for UploadTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UploadTarget")
            .field("upload_id", &self.upload_id)
            .field("client_target_id", &self.client_target_id)
            .field("kind", &self.kind)
            .field("candidate_indices", &self.candidate_indices)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CasDisposition {
    pub candidate_index: u32,
    pub disposition: Disposition,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Disposition {
    InlineWithRecording { upload_id: String },
    MemberOfBatch { upload_id: String },
    SeparateObject { upload_id: String },
    AlreadyAvailable {},
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disposition_wire_shape_is_stable() {
        let value = CasDisposition {
            candidate_index: 2,
            disposition: Disposition::MemberOfBatch {
                upload_id: "batch-1".into(),
            },
        };
        let json = serde_json::json!({
            "candidate_index": 2,
            "disposition": {"kind": "member_of_batch", "upload_id": "batch-1"},
        });
        assert_eq!(serde_json::to_value(&value).unwrap(), json);
        assert_eq!(
            serde_json::from_value::<CasDisposition>(json).unwrap(),
            value
        );
    }

    #[test]
    fn unknown_dispositions_and_fields_are_rejected() {
        for json in [
            serde_json::json!({"kind": "unknown"}),
            serde_json::json!({"kind": "already_available", "upload_id": "extra"}),
        ] {
            assert!(serde_json::from_value::<Disposition>(json).is_err());
        }
    }

    #[test]
    fn target_debug_does_not_expose_capabilities() {
        let target = UploadTarget {
            upload_id: "upload-1".into(),
            client_target_id: 0,
            object_key: "private-key".into(),
            presigned_put_url: "https://example.invalid/?secret-token".into(),
            expires_at_unix_ms: 1,
            required_headers: BTreeMap::from([("x-secret".into(), "secret-value".into())]),
            kind: UploadKind::Recording,
            candidate_indices: vec![],
        };
        let debug = format!("{target:?}");
        for secret in ["private-key", "secret-token", "secret-value"] {
            assert!(!debug.contains(secret));
        }
    }
}
