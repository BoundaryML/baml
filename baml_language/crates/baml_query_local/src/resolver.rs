//! The local canonical-CAS ValueResolver.
//!
//! Handle codec (provider-private; never a public surface, never an
//! equality surface):
//!
//! ```text
//! 0x00 <reason>          typed unavailability recorded at row build
//! 0x01 <32-byte CID>     canonical DAG root in the project CAS
//! 0x02 <json ref>        legacy inline/blob body: {"run":dir,"value":id}
//! ```
//!
//! CID handles admit the D7 identity shortcut (`canonical_cid`); legacy
//! handles never do — they hydrate and compare semantically.

use std::sync::{Arc, Mutex};

use baml_query::outcome::UnavailableReason;
use baml_query::value::resolver::{DecodeCaps, Resolved, ValueResolver};
use bex_events::store::{Store, canon};

use crate::universe::LocalUniverse;

pub const TAG_UNAVAILABLE: u8 = 0x00;
pub const TAG_CID: u8 = 0x01;
pub const TAG_LEGACY: u8 = 0x02;

/// Encode a typed-unavailable handle.
#[must_use]
pub fn unavailable_handle(reason: UnavailableReason) -> Vec<u8> {
    vec![TAG_UNAVAILABLE, reason_byte(reason)]
}

/// Encode a canonical-CID handle.
#[must_use]
pub fn cid_handle(cid: &[u8; 32]) -> Vec<u8> {
    let mut handle = Vec::with_capacity(33);
    handle.push(TAG_CID);
    handle.extend_from_slice(cid);
    handle
}

/// Encode a legacy body reference.
#[must_use]
pub fn legacy_handle(run_dir_name: &str, value_id: &str) -> Vec<u8> {
    let mut handle = vec![TAG_LEGACY];
    handle.extend_from_slice(
        serde_json::json!({ "run": run_dir_name, "value": value_id })
            .to_string()
            .as_bytes(),
    );
    handle
}

fn reason_byte(reason: UnavailableReason) -> u8 {
    match reason {
        UnavailableReason::Pending => 1,
        UnavailableReason::NotCaptured => 2,
        UnavailableReason::Omitted => 3,
        UnavailableReason::Redacted => 4,
        UnavailableReason::Lost => 5,
        UnavailableReason::Truncated => 6,
        UnavailableReason::Corrupt => 7,
        UnavailableReason::Unsupported => 8,
        UnavailableReason::StoreUnavailable => 9,
        UnavailableReason::QueryBudgetExhausted => 10,
    }
}

fn byte_reason(byte: u8) -> UnavailableReason {
    match byte {
        1 => UnavailableReason::Pending,
        2 => UnavailableReason::NotCaptured,
        3 => UnavailableReason::Omitted,
        4 => UnavailableReason::Redacted,
        5 => UnavailableReason::Lost,
        6 => UnavailableReason::Truncated,
        8 => UnavailableReason::Unsupported,
        9 => UnavailableReason::StoreUnavailable,
        10 => UnavailableReason::QueryBudgetExhausted,
        _ => UnavailableReason::Corrupt,
    }
}

/// CAS-backed local resolver bound to one universe.
pub struct LocalValueResolver {
    universe: Arc<LocalUniverse>,
    store: Mutex<StoreState>,
}

enum StoreState {
    Unopened,
    Open(Box<Store>),
    Failed,
}

impl LocalValueResolver {
    #[must_use]
    pub fn new(universe: Arc<LocalUniverse>) -> LocalValueResolver {
        LocalValueResolver {
            universe,
            store: Mutex::new(StoreState::Unopened),
        }
    }

    fn with_store<R>(&self, f: impl FnOnce(&Store) -> R) -> Option<R> {
        let mut state = self
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(*state, StoreState::Unopened) {
            let dir = self.universe.baml_dir().join("store");
            *state = match Store::open(&dir, [0u8; 16]) {
                Ok(store) => StoreState::Open(Box::new(store)),
                Err(_) => StoreState::Failed,
            };
        }
        match &*state {
            StoreState::Open(store) => Some(f(store)),
            _ => None,
        }
    }

    fn resolve_dag(&self, cid: &[u8; 32], caps: DecodeCaps) -> Resolved {
        let resolved = self.with_store(|store| {
            let Ok(Some(root)) = store.get(cid) else {
                return Resolved::Unavailable(UnavailableReason::Lost);
            };
            struct Src<'a>(&'a Store);
            impl canon::DagSource for Src<'_> {
                fn node(&mut self, cid: &[u8; 32]) -> Option<Vec<u8>> {
                    self.0.get(cid).ok().flatten()
                }
                fn chunk(&mut self, cid: &[u8; 32]) -> Option<Vec<u8>> {
                    self.0.get(cid).ok().flatten()
                }
            }
            let mut budget = canon::DecodeBudget::bounded(
                usize::try_from(caps.max_bytes).unwrap_or(usize::MAX),
                caps.max_depth,
            );
            budget.spent = root.len();
            match canon::decode_budgeted(&root, &mut Src(store), &mut budget) {
                Ok(value) => Resolved::Value(Arc::new(value)),
                Err(canon::DecodeError::MissingNode(_) | canon::DecodeError::MissingChunk(_)) => {
                    Resolved::Unavailable(UnavailableReason::Lost)
                }
                Err(canon::DecodeError::Malformed(_)) => {
                    Resolved::Unavailable(UnavailableReason::Corrupt)
                }
            }
        });
        resolved.unwrap_or(Resolved::Unavailable(UnavailableReason::StoreUnavailable))
    }

    fn resolve_legacy(&self, payload: &[u8], caps: DecodeCaps) -> Resolved {
        let Ok(reference) = serde_json::from_slice::<serde_json::Value>(payload) else {
            return Resolved::Unavailable(UnavailableReason::Corrupt);
        };
        let (Some(run_name), Some(value_id)) = (
            reference.get("run").and_then(|v| v.as_str()),
            reference.get("value").and_then(|v| v.as_str()),
        ) else {
            return Resolved::Unavailable(UnavailableReason::Corrupt);
        };
        let Some(run) = self
            .universe
            .runs
            .iter()
            .find(|r| r.row.dir.file_name().is_some_and(|n| n == run_name))
        else {
            return Resolved::Unavailable(UnavailableReason::Lost);
        };
        // Cold path: scan the run's bound value segments for the record.
        for file in &run.value_files {
            let Ok(bytes) = file.read() else { continue };
            let Ok(contents) = bex_events::value::read_bamlvalue_from_bytes(&bytes) else {
                continue;
            };
            for record in contents.records {
                let bex_events::value::ValueFileRecord::CapturedValue(record) = record else {
                    continue;
                };
                if record.value_ref.id != value_id {
                    continue;
                }
                // Prefer canonical DAG even on the legacy path.
                if let Some(dag) = &record.dag_ref {
                    return self.resolve_dag(&dag.root_cid, caps);
                }
                if record.body.is_empty() {
                    return Resolved::Unavailable(UnavailableReason::Lost);
                }
                // Legacy inline body: schema-erased JSON interpretation.
                return match bex_query::values::decode_legacy_body_json(&record.body) {
                    Some(json) => Resolved::Value(Arc::new(
                        baml_query::value::semantics::json_to_canon(&json),
                    )),
                    None => Resolved::Unavailable(UnavailableReason::Corrupt),
                };
            }
        }
        Resolved::Unavailable(UnavailableReason::Lost)
    }
}

impl ValueResolver for LocalValueResolver {
    fn resolve(&self, handle: &[u8], caps: DecodeCaps) -> Resolved {
        match handle.split_first() {
            Some((&TAG_UNAVAILABLE, rest)) => {
                Resolved::Unavailable(byte_reason(rest.first().copied().unwrap_or(0)))
            }
            Some((&TAG_CID, rest)) => match <[u8; 32]>::try_from(rest) {
                Ok(cid) => self.resolve_dag(&cid, caps),
                Err(_) => Resolved::Unavailable(UnavailableReason::Corrupt),
            },
            Some((&TAG_LEGACY, rest)) => self.resolve_legacy(rest, caps),
            _ => Resolved::Unavailable(UnavailableReason::Corrupt),
        }
    }

    fn resolve_cid(&self, cid: &[u8; 32], caps: DecodeCaps) -> Resolved {
        self.resolve_dag(cid, caps)
    }

    fn canonical_cid(&self, handle: &[u8]) -> Option<[u8; 32]> {
        match handle.split_first() {
            Some((&TAG_CID, rest)) => <[u8; 32]>::try_from(rest).ok(),
            _ => None,
        }
    }
}
