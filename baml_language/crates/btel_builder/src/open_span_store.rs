use std::collections::{HashMap, hash_map::Entry};

use crate::event::{CallKey, ClosedSpan, OpenSpan};

/// Only unmatched halves live here. Capacity grows with concurrent unmatched
/// calls, not total completed calls. No timeout, thread-end flush, or tombstones.
#[derive(Default)]
pub struct OpenSpanStore {
    spans: HashMap<CallKey, OpenSpan>,
}

impl OpenSpanStore {
    pub fn len(&self) -> usize {
        self.spans.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&CallKey, &OpenSpan)> {
        self.spans.iter()
    }

    pub(crate) fn drain(&mut self) -> impl Iterator<Item = (CallKey, OpenSpan)> + '_ {
        self.spans.drain()
    }

    /// Same-side duplicates fail without overwriting the retained fact.
    /// Completed IDs aren't retained: replay deduplication is not this store's job.
    pub(crate) fn insert(
        &mut self,
        key: CallKey,
        half: OpenSpan,
    ) -> Result<Option<ClosedSpan>, CallKey> {
        match self.spans.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert(half);
                Ok(None)
            }
            Entry::Occupied(entry) => {
                let ((OpenSpan::Start(start), OpenSpan::End(end))
                | (OpenSpan::End(end), OpenSpan::Start(start))) = (*entry.get(), half)
                else {
                    return Err(key);
                };
                entry.remove();
                Ok(Some(ClosedSpan { key, start, end }))
            }
        }
    }
}
