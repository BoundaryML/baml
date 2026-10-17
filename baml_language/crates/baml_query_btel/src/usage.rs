//! Stored model usage (`call.usage`, `call.total_usage`, and the same on
//! `thread`): a JSON list with one entry per provider and model, sorted by
//! both. A count no call reported is null, not zero.

use btel_recorder::proto;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ModelUsage {
    pub(crate) provider: String,
    pub(crate) model: Option<String>,
    pub(crate) calls: i64,
    pub(crate) input_tokens: Option<i64>,
    pub(crate) output_tokens: Option<i64>,
    pub(crate) cache_read_tokens: Option<i64>,
    pub(crate) cache_write_tokens: Option<i64>,
    pub(crate) reasoning_tokens: Option<i64>,
}

impl ModelUsage {
    fn key(&self) -> (&str, Option<&str>) {
        (&self.provider, self.model.as_deref())
    }

    fn add(&mut self, other: &Self) {
        let sum = |a: Option<i64>, b: Option<i64>| match (a, b) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0).saturating_add(b.unwrap_or(0))),
        };
        self.calls = self.calls.saturating_add(other.calls);
        self.input_tokens = sum(self.input_tokens, other.input_tokens);
        self.output_tokens = sum(self.output_tokens, other.output_tokens);
        self.cache_read_tokens = sum(self.cache_read_tokens, other.cache_read_tokens);
        self.cache_write_tokens = sum(self.cache_write_tokens, other.cache_write_tokens);
        self.reasoning_tokens = sum(self.reasoning_tokens, other.reasoning_tokens);
    }
}

/// A completion's usage as stored; empty without usage.
pub(crate) fn from_proto(usage: &[proto::ModelUsage]) -> Vec<ModelUsage> {
    let count = |value: u64| i64::try_from(value).unwrap_or(i64::MAX);
    let mut entries = Vec::new();
    merge(
        &mut entries,
        &usage
            .iter()
            .map(|entry| ModelUsage {
                provider: entry.provider.clone(),
                model: entry.model.clone(),
                calls: count(entry.calls),
                input_tokens: entry.input_tokens.map(count),
                output_tokens: entry.output_tokens.map(count),
                cache_read_tokens: entry.cache_read_tokens.map(count),
                cache_write_tokens: entry.cache_write_tokens.map(count),
                reasoning_tokens: entry.reasoning_tokens.map(count),
            })
            .collect::<Vec<_>>(),
    );
    entries
}

/// Add `added` into `into`, summing entries for one provider's model and
/// keeping `into` sorted.
pub(crate) fn merge(into: &mut Vec<ModelUsage>, added: &[ModelUsage]) {
    for entry in added {
        match into.binary_search_by(|known| known.key().cmp(&entry.key())) {
            Ok(index) => into[index].add(entry),
            Err(index) => into.insert(index, entry.clone()),
        }
    }
}

pub(crate) fn parse(text: &str) -> serde_json::Result<Vec<ModelUsage>> {
    serde_json::from_str(text)
}

/// The stored column value; `None` without usage.
pub(crate) fn to_json(entries: &[ModelUsage]) -> Option<String> {
    (!entries.is_empty()).then(|| serde_json::to_string(entries).expect("usage serializes"))
}
