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

/// Upload targets may use plain HTTP only when the prepare base does, so an
/// HTTPS base never sends payloads in the clear.
pub(crate) fn checked_url(value: &str, base: &Url) -> Result<Url, DeliveryError> {
    let url = Url::parse(value).map_err(|_| DeliveryError::InvalidPlan)?;
    validate_url(&url, is_loopback_http(base))?;
    Ok(url)
}

/// Plain HTTP is accepted only for loopback hosts, so local development can
/// reach a local data plane without TLS.
pub(crate) fn is_loopback_http(url: &Url) -> bool {
    url.scheme() == "http" && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
}

pub(crate) fn validate_url(url: &Url, loopback_http: bool) -> Result<(), DeliveryError> {
    if (url.scheme() != "https" && !(loopback_http && is_loopback_http(url)))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(DeliveryError::InvalidPlan);
    }
    Ok(())
}

pub(crate) fn validate_response(
    request: &PrepareUploadsRequest,
    response: &PrepareUploadsResponse,
    base: &Url,
) -> Result<(), DeliveryError> {
    if response.plan_id.is_empty() {
        return Err(DeliveryError::InvalidPlan);
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
        checked_url(&upload.presigned_put_url, base)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_http_is_limited_to_loopback_hosts_and_loopback_bases() {
        let http_base = Url::parse("http://127.0.0.1:9000/publisher").unwrap();
        let https_base = Url::parse("https://bcs.example/publisher").unwrap();
        for value in [
            "http://localhost:8080/put",
            "http://127.0.0.1:1/put",
            "http://[::1]:8080/put",
        ] {
            let url = Url::parse(value).unwrap();
            assert_eq!(validate_url(&url, true), Ok(()), "{value}");
            assert_eq!(checked_url(value, &http_base), Ok(url), "{value}");
            assert_eq!(
                checked_url(value, &https_base),
                Err(DeliveryError::InvalidPlan),
                "{value}"
            );
        }
        for value in [
            "http://example.com/",
            "http://localhost.example/",
            "http://127.0.0.2/",
        ] {
            let url = Url::parse(value).unwrap();
            assert_eq!(validate_url(&url, true), Err(DeliveryError::InvalidPlan));
            assert_eq!(
                checked_url(value, &http_base),
                Err(DeliveryError::InvalidPlan)
            );
        }
        assert!(checked_url("https://uploads.example/put", &http_base).is_ok());
    }
}
