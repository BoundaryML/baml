//! Stored model usage (`call.usage`, `call.total_usage`, and the same on
//! `thread`): a JSON list with one entry per provider and model, sorted by
//! both. A count no call reported is null, not zero. How usage is read and
//! summed is `btel-core`'s (`btel_core::reader::usage`), as it is for BCS.

pub(crate) use btel_core::reader::usage::{ModelUsage, from_wire, merge};

pub(crate) fn parse(text: &str) -> serde_json::Result<Vec<ModelUsage>> {
    serde_json::from_str(text)
}

/// The stored column value; `None` without usage.
pub(crate) fn to_json(entries: &[ModelUsage]) -> Option<String> {
    (!entries.is_empty()).then(|| serde_json::to_string(entries).expect("usage serializes"))
}
