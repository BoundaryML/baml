use std::collections::HashSet;

use reqwest::{
    Url,
    header::{HeaderName, HeaderValue},
};

use crate::{
    delivery::DeliveryError,
    wire::{Disposition, PrepareUploadsRequest, PrepareUploadsResponse, UploadKind},
};

pub(crate) fn validate_proposal(request: &PrepareUploadsRequest) -> Result<(), DeliveryError> {
    let mut ids = HashSet::new();
    if request
        .candidates
        .iter()
        .any(|c| !ids.insert(&c.snapshot_id))
    {
        return Err(DeliveryError::InvalidPlan);
    }
    let mut targets = HashSet::new();
    let mut assigned = vec![false; request.candidates.len()];
    let mut recordings = 0;
    for target in &request.proposed_uploads {
        if !targets.insert(target.client_target_id) {
            return Err(DeliveryError::InvalidPlan);
        }
        match target.kind {
            UploadKind::Recording => recordings += 1,
            UploadKind::CasObject if target.candidate_indices.len() != 1 => {
                return Err(DeliveryError::InvalidPlan);
            }
            UploadKind::CasBatch if target.candidate_indices.is_empty() => {
                return Err(DeliveryError::InvalidPlan);
            }
            _ => {}
        }
        for &index in &target.candidate_indices {
            let slot = assigned
                .get_mut(index as usize)
                .ok_or(DeliveryError::InvalidPlan)?;
            if std::mem::replace(slot, true) {
                return Err(DeliveryError::InvalidPlan);
            }
            if request.candidates[index as usize].size_class.is_some()
                && target.kind != UploadKind::CasObject
            {
                return Err(DeliveryError::InvalidPlan);
            }
        }
    }
    if recordings != 1 || assigned.contains(&false) {
        return Err(DeliveryError::InvalidPlan);
    }
    Ok(())
}

pub(crate) fn checked_url(value: &str, allow_http: bool) -> Result<Url, DeliveryError> {
    let url = Url::parse(value).map_err(|_| DeliveryError::InvalidPlan)?;
    if (url.scheme() != "https" && !(allow_http && url.scheme() == "http"))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(DeliveryError::InvalidPlan);
    }
    Ok(url)
}

pub(crate) fn validate_response(
    request: &PrepareUploadsRequest,
    response: &PrepareUploadsResponse,
    required_expiry_ms: u64,
    allow_http: bool,
) -> Result<(), DeliveryError> {
    if response.plan_id.is_empty() {
        return Err(DeliveryError::InvalidPlan);
    }
    if response.expires_at_unix_ms <= required_expiry_ms {
        return Err(DeliveryError::Expired);
    }
    if response.cas.len() != request.candidates.len() {
        return Err(DeliveryError::InvalidPlan);
    }
    let mut dispositions = vec![None; request.candidates.len()];
    for disposition in &response.cas {
        let slot = dispositions
            .get_mut(disposition.candidate_index as usize)
            .ok_or(DeliveryError::InvalidPlan)?;
        if slot.replace(&disposition.disposition).is_some() {
            return Err(DeliveryError::InvalidPlan);
        }
    }
    let mut upload_ids = HashSet::new();
    let mut target_ids = HashSet::new();
    let mut keys = HashSet::new();
    let mut urls = HashSet::new();
    for upload in &response.uploads {
        if upload.upload_id.is_empty()
            || upload.object_key.is_empty()
            || !upload_ids.insert(&upload.upload_id)
            || !keys.insert(&upload.object_key)
            || !urls.insert(&upload.presigned_put_url)
            || !target_ids.insert(upload.client_target_id)
        {
            return Err(DeliveryError::InvalidPlan);
        }
        if upload.expires_at_unix_ms <= required_expiry_ms {
            return Err(DeliveryError::Expired);
        }
        checked_url(&upload.presigned_put_url, allow_http)?;
        let mut headers = HashSet::new();
        for (name, value) in &upload.required_headers {
            let name =
                HeaderName::from_bytes(name.as_bytes()).map_err(|_| DeliveryError::InvalidPlan)?;
            HeaderValue::from_str(value).map_err(|_| DeliveryError::InvalidPlan)?;
            if !headers.insert(name.clone())
                || matches!(
                    name.as_str(),
                    "authorization"
                        | "proxy-authorization"
                        | "cookie"
                        | "host"
                        | "content-length"
                        | "transfer-encoding"
                        | "connection"
                        | "content-md5"
                        | "x-amz-sdk-checksum-algorithm"
                )
                || name.as_str().starts_with("x-amz-checksum-")
                || (name.as_str() == "x-amz-content-sha256" && value != "UNSIGNED-PAYLOAD")
            {
                return Err(DeliveryError::InvalidPlan);
            }
        }
        let proposed = request
            .proposed_uploads
            .iter()
            .find(|p| p.client_target_id == upload.client_target_id)
            .ok_or(DeliveryError::InvalidPlan)?;
        if proposed.kind != upload.kind {
            return Err(DeliveryError::InvalidPlan);
        }
        let expected: Vec<_> = proposed
            .candidate_indices
            .iter()
            .copied()
            .filter(|&i| {
                !matches!(
                    dispositions[i as usize],
                    Some(Disposition::AlreadyAvailable {})
                )
            })
            .collect();
        if upload.candidate_indices != expected
            || (expected.is_empty() && upload.kind != UploadKind::Recording)
        {
            return Err(DeliveryError::InvalidPlan);
        }
        for index in expected {
            let ((
                UploadKind::Recording,
                Some(Disposition::InlineWithRecording {
                    upload_id: referenced,
                }),
            )
            | (
                UploadKind::CasBatch,
                Some(Disposition::MemberOfBatch {
                    upload_id: referenced,
                }),
            )
            | (
                UploadKind::CasObject,
                Some(Disposition::SeparateObject {
                    upload_id: referenced,
                }),
            )) = (upload.kind, dispositions[index as usize])
            else {
                return Err(DeliveryError::InvalidPlan);
            };
            if referenced != &upload.upload_id {
                return Err(DeliveryError::InvalidPlan);
            }
        }
    }
    for proposed in &request.proposed_uploads {
        let required = proposed.kind == UploadKind::Recording
            || proposed.candidate_indices.iter().any(|&i| {
                !matches!(
                    dispositions[i as usize],
                    Some(Disposition::AlreadyAvailable {})
                )
            });
        if required != target_ids.contains(&proposed.client_target_id) {
            return Err(DeliveryError::InvalidPlan);
        }
    }
    Ok(())
}
