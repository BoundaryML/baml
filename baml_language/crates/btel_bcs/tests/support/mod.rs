use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

use btel_bcs::wire::{
    CasDisposition, Disposition, PrepareUploadsRequest, PrepareUploadsResponse, UploadKind,
    UploadTarget,
};
use wiremock::Request;

pub(crate) fn response(request: &Request, base: &str, skips: &[u32]) -> PrepareUploadsResponse {
    let request: PrepareUploadsRequest = serde_json::from_slice(&request.body).unwrap();
    let expiry = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
        + 3_600_000;
    let mut uploads = Vec::new();
    let mut cas = Vec::new();
    for target in request.proposed_uploads {
        let upload_id = format!(
            "{}-{}",
            request.recording.recording_file_sequence, target.client_target_id
        );
        let mut members = Vec::new();
        for index in target.candidate_indices {
            let disposition = if skips.contains(&index) {
                Disposition::AlreadyAvailable {}
            } else {
                members.push(index);
                match target.kind {
                    UploadKind::Recording => Disposition::InlineWithRecording {
                        upload_id: upload_id.clone(),
                    },
                    UploadKind::CasBatch => Disposition::MemberOfBatch {
                        upload_id: upload_id.clone(),
                    },
                    UploadKind::CasObject => Disposition::SeparateObject {
                        upload_id: upload_id.clone(),
                    },
                }
            };
            cas.push(CasDisposition {
                candidate_index: index,
                disposition,
            });
        }
        if target.kind == UploadKind::Recording || !members.is_empty() {
            uploads.push(UploadTarget {
                object_key: format!("key-{upload_id}"),
                presigned_put_url: format!("{base}/put/{upload_id}?capability=secret"),
                upload_id,
                client_target_id: target.client_target_id,
                expires_at_unix_ms: expiry,
                required_headers: BTreeMap::from([("x-required".into(), "signed-value".into())]),
                kind: target.kind,
                candidate_indices: members,
            });
        }
    }
    PrepareUploadsResponse {
        plan_id: format!("plan-{}", request.recording.recording_file_sequence),
        expires_at_unix_ms: expiry,
        uploads,
        cas,
        heartbeat: None,
    }
}
