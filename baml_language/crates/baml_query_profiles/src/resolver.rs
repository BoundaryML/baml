//! The store-backed value resolver and the provider's handle encoding
//! (TASK/baml-query-scope.md §5.5).
//!
//! Handle wire form (provider-private; SQL never sees it):
//! `0x01 ‖ u16-BE codec ‖ cid[32]` for an available value,
//! `0x00 ‖ reason-byte` for a typed unavailability.

use std::{path::PathBuf, sync::Arc};

use baml_query::{
    outcome::UnavailableReason,
    value::{
        model::Value,
        resolver::{DecodeCaps, Resolved, ValueResolver},
    },
};
use bex_prof_store::prof::backend::{ValueCid, ValueState, decode_cas_object};

use crate::decode::decode_codec1;

const HANDLE_AVAILABLE: u8 = 0x01;
const HANDLE_UNAVAILABLE: u8 = 0x00;

/// Encode a value handle from a fold's `ValueState`.
#[must_use]
pub(crate) fn encode_handle(state: ValueState) -> Vec<u8> {
    match state {
        ValueState::Available { cid, codec, .. } => {
            let mut handle = Vec::with_capacity(35);
            handle.push(HANDLE_AVAILABLE);
            handle.extend_from_slice(&codec.0.to_be_bytes());
            handle.extend_from_slice(&cid.0);
            handle
        }
        ValueState::Lost(reason) => vec![HANDLE_UNAVAILABLE, loss_byte(reason)],
    }
}

/// Encode the handle for a role that was selected but never produced a
/// value fact (evidence gap, not "not applicable").
#[must_use]
pub(crate) fn encode_missing_handle() -> Vec<u8> {
    vec![HANDLE_UNAVAILABLE, LOSS_NOT_CAPTURED]
}

const LOSS_NOT_CAPTURED: u8 = 0xFE;

/// The one loss table: (reason, wire byte, SQL-facing unavailability).
/// `loss_byte` and `loss_unavailability` both read it, so the numbering
/// cannot drift between encode and decode; a round-trip test walks it.
const LOSS_TABLE: [(
    bex_prof_store::prof::backend::ValueLossReason,
    u8,
    UnavailableReason,
); 11] = {
    use bex_prof_store::prof::backend::ValueLossReason as R;
    [
        (R::ValueMemoryExceeded, 0, UnavailableReason::Lost),
        (R::ValueAttemptTransportExceeded, 1, UnavailableReason::Lost),
        (
            R::ErrorCaptureAttemptTransportExceeded,
            2,
            UnavailableReason::Lost,
        ),
        (R::ValueTooLarge, 3, UnavailableReason::Truncated),
        (R::CopyFailed, 4, UnavailableReason::Lost),
        (R::EncodeFailed, 5, UnavailableReason::Lost),
        (R::CasWriteFailed, 6, UnavailableReason::Lost),
        (R::CasConflict, 7, UnavailableReason::Lost),
        (R::DiskGuardExceeded, 8, UnavailableReason::Lost),
        (R::EvidenceSegmentPublishFailed, 9, UnavailableReason::Lost),
        (R::StoreUnavailable, 10, UnavailableReason::StoreUnavailable),
    ]
};

fn loss_byte(reason: bex_prof_store::prof::backend::ValueLossReason) -> u8 {
    LOSS_TABLE
        .iter()
        .find(|(r, _, _)| *r == reason)
        .map_or(u8::MAX, |(_, byte, _)| *byte)
}

fn loss_unavailability(byte: u8) -> UnavailableReason {
    if byte == LOSS_NOT_CAPTURED {
        return UnavailableReason::NotCaptured;
    }
    LOSS_TABLE
        .iter()
        .find(|(_, b, _)| *b == byte)
        .map_or(UnavailableReason::Lost, |(_, _, reason)| *reason)
}

/// Resolves provider handles from the bound store's CAS.
pub struct ProfilesResolver {
    store_root: PathBuf,
}

impl ProfilesResolver {
    #[must_use]
    pub fn new(store_root: PathBuf) -> Arc<ProfilesResolver> {
        Arc::new(ProfilesResolver { store_root })
    }

    fn resolve_one(&self, handle: &[u8], caps: DecodeCaps) -> Resolved {
        match handle.split_first() {
            Some((&HANDLE_AVAILABLE, rest)) if rest.len() == 34 => {
                let codec = u16::from_be_bytes([rest[0], rest[1]]);
                if codec != 1 {
                    return Resolved::Unavailable(UnavailableReason::Unsupported);
                }
                let mut cid = [0u8; 32];
                cid.copy_from_slice(&rest[2..]);
                self.resolve_cid(&cid, caps)
            }
            Some((&HANDLE_UNAVAILABLE, rest)) if rest.len() == 1 => {
                Resolved::Unavailable(loss_unavailability(rest[0]))
            }
            _ => Resolved::Unavailable(UnavailableReason::Corrupt),
        }
    }

    fn read_cas(&self, cid: &[u8; 32], caps: DecodeCaps) -> Result<Value, UnavailableReason> {
        let digest = hex::encode(cid);
        let path = self
            .store_root
            .join("cas/sha256")
            .join(&digest[..2])
            .join(format!("{digest}.bamlvalue"));
        let bytes = std::fs::read(&path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                UnavailableReason::Lost
            } else {
                UnavailableReason::StoreUnavailable
            }
        })?;
        if bytes.len() as u64 > caps.max_bytes {
            return Err(UnavailableReason::Truncated);
        }
        let object = decode_cas_object(&bytes).map_err(|_| UnavailableReason::Corrupt)?;
        if object.cid != ValueCid(*cid) {
            return Err(UnavailableReason::Corrupt);
        }
        if object.codec.0 != 1 {
            return Err(UnavailableReason::Unsupported);
        }
        decode_codec1(&object.body, caps.max_depth)
    }
}

impl ValueResolver for ProfilesResolver {
    fn resolve_many(&self, handles: &[&[u8]], caps: DecodeCaps) -> Vec<Resolved> {
        handles
            .iter()
            .map(|handle| self.resolve_one(handle, caps))
            .collect()
    }

    fn resolve_cid(&self, cid: &[u8; 32], caps: DecodeCaps) -> Resolved {
        match self.read_cas(cid, caps) {
            Ok(value) => Resolved::Value(Arc::new(value)),
            Err(reason) => Resolved::Unavailable(reason),
        }
    }

    /// CAS cids ARE the canonical identity of the encoded body: the
    /// shortcut is exact for codec-1 handles.
    fn canonical_cid(&self, handle: &[u8]) -> Option<[u8; 32]> {
        match handle.split_first() {
            Some((&HANDLE_AVAILABLE, rest)) if rest.len() == 34 => {
                let mut cid = [0u8; 32];
                cid.copy_from_slice(&rest[2..]);
                Some(cid)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod loss_table_tests {
    use super::*;

    #[test]
    fn every_loss_reason_round_trips_through_the_wire_byte() {
        for (reason, byte, unavailability) in LOSS_TABLE {
            let handle = encode_handle(bex_prof_store::prof::backend::ValueState::Lost(reason));
            assert_eq!(handle, vec![HANDLE_UNAVAILABLE, byte], "{reason:?}");
            assert_eq!(loss_unavailability(byte), unavailability, "{reason:?}");
        }
        assert_eq!(
            loss_unavailability(LOSS_NOT_CAPTURED),
            UnavailableReason::NotCaptured
        );
    }
}
