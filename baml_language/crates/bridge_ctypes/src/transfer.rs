//! Ownership of an encoded aggregate, independent of whether its bytes decode.
//!
//! A receipt identifies staged ownership, not an object. Completed transfers
//! are removed; a long-lived session does not retain historical receivers.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::{CffiHandleTable, CffiHandleTableEntry};

pub(crate) struct OutboundOwnership<'a> {
    pub(crate) table: &'a CffiHandleTable,
    keys: Vec<OwnedHandle<'a>>,
}

struct OwnedHandle<'a> {
    table: &'a CffiHandleTable,
    key: u64,
    owned: bool,
}

impl Drop for OwnedHandle<'_> {
    fn drop(&mut self) {
        if self.owned {
            self.table.release(self.key);
        }
    }
}

impl<'a> OutboundOwnership<'a> {
    pub(crate) fn new(table: &'a CffiHandleTable) -> Self {
        Self {
            table,
            keys: Vec::new(),
        }
    }

    pub(crate) fn insert(&mut self, value: CffiHandleTableEntry) -> u64 {
        self.keys.reserve(1);
        let key = self.table.insert(value);
        self.keys.push(OwnedHandle {
            table: self.table,
            key,
            owned: true,
        });
        key
    }

    pub(crate) fn append(&mut self, other: &mut Self) {
        assert!(std::ptr::eq(self.table, other.table));
        self.keys.reserve(other.keys.len());
        self.keys.append(&mut other.keys);
    }

    fn commit_keys(&mut self) {
        for handle in &mut self.keys {
            handle.owned = false;
        }
        self.keys.clear();
    }
}

/// Encoded payload and all ownership created by encoding it. Dropping this
/// before staging rolls back the whole aggregate, including host retention.
#[must_use]
pub struct EncodedTransfer<'a, T> {
    pub(crate) payload: T,
    pub(crate) ownership: OutboundOwnership<'a>,
}

impl<'a, T> EncodedTransfer<'a, T> {
    pub fn map_payload<U>(self, map: impl FnOnce(T) -> U) -> EncodedTransfer<'a, U> {
        EncodedTransfer {
            payload: map(self.payload),
            ownership: self.ownership,
        }
    }

    /// Temporary entry for adapters not yet ported to receipts. It transfers
    /// every key immediately and cannot protect against failed delivery or
    /// partial decoding. Remove at ABI cutover.
    pub fn into_unreceipted(self) -> T {
        let Self {
            payload,
            mut ownership,
        } = self;
        ownership.commit_keys();
        payload
    }
}

/// Passed beside the payload bytes, so even malformed bytes can be discarded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferReceipt {
    pub session_id: u64,
    pub transfer_id: u64,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum TransferError {
    #[error("transfer belongs to another session")]
    WrongSession,
    #[error("encoded ownership belongs to another handle table")]
    WrongTable,
    #[error("transfer session is closed")]
    Closed,
    #[error("transfer is absent or already completed")]
    NotPending,
    #[error("claimed handle ownership is not present in this transfer")]
    InvalidClaims,
    #[error("transfer identifier space exhausted")]
    Exhausted,
}

struct SessionState<'a> {
    closed: bool,
    next_transfer: u64,
    staged: HashMap<u64, OutboundOwnership<'a>>,
}

struct SessionInner<'a> {
    id: u64,
    table: &'a CffiHandleTable,
    state: Mutex<SessionState<'a>>,
}

/// One transport session's staged output. Its owner must close it during
/// runtime/session teardown, including when the runtime is a global singleton.
/// Cloning this value refers to the same session, not a new ownership domain.
#[derive(Clone)]
pub struct TransferSession<'a> {
    inner: Arc<SessionInner<'a>>,
}

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

impl<'a> TransferSession<'a> {
    pub fn new(table: &'a CffiHandleTable) -> Self {
        let id = NEXT_SESSION
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("transfer session identifier space exhausted");
        Self {
            inner: Arc::new(SessionInner {
                id,
                table,
                state: Mutex::new(SessionState {
                    closed: false,
                    next_transfer: 1,
                    staged: HashMap::new(),
                }),
            }),
        }
    }

    pub fn stage<T>(
        &self,
        encoded: EncodedTransfer<'a, T>,
    ) -> Result<PendingDelivery<'a, T>, TransferError> {
        if !std::ptr::eq(self.inner.table, encoded.ownership.table) {
            return Err(TransferError::WrongTable);
        }
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed {
            return Err(TransferError::Closed);
        }
        let id = state.next_transfer;
        state.next_transfer = id.checked_add(1).ok_or(TransferError::Exhausted)?;
        state.staged.insert(id, encoded.ownership);
        Ok(PendingDelivery {
            session: self.clone(),
            receipt: TransferReceipt {
                session_id: self.inner.id,
                transfer_id: id,
            },
            payload: Some(encoded.payload),
        })
    }

    /// Atomically adopt only the leases used by successfully decoded wrappers.
    /// Repeated keys count as repeated ownership, not a set of identities.
    /// The SDK must root provisional wrappers and rehydrate host objects before
    /// adoption, then activate only those wrappers after this succeeds.
    pub fn adopt(&self, receipt: TransferReceipt, claims: &[u64]) -> Result<(), TransferError> {
        self.check_session(receipt)?;
        let mut counts = HashMap::<u64, usize>::new();
        for key in claims {
            *counts.entry(*key).or_default() += 1;
        }
        let mut ownership = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.closed {
                return Err(TransferError::Closed);
            }
            let ownership = state
                .staged
                .get(&receipt.transfer_id)
                .ok_or(TransferError::NotPending)?;
            let mut remaining = counts.clone();
            for handle in &ownership.keys {
                if let Some(count) = remaining.get_mut(&handle.key) {
                    *count = count.saturating_sub(1);
                }
            }
            if remaining.values().any(|count| *count != 0) {
                return Err(TransferError::InvalidClaims);
            }
            state
                .staged
                .remove(&receipt.transfer_id)
                .expect("validated while holding session lock")
        };
        // No table release or host destructor may run under the session lock.
        // Keep claimed leases owned until cleanup finishes, too: a panicking
        // unclaimed resource must roll them back before SDK wrappers activate.
        let mut adopted = OutboundOwnership::new(self.inner.table);
        adopted.keys.reserve(claims.len());
        ownership
            .keys
            .retain_mut(|handle| match counts.get_mut(&handle.key) {
                Some(count) if *count > 0 => {
                    *count -= 1;
                    adopted.keys.push(OwnedHandle {
                        table: handle.table,
                        key: handle.key,
                        owned: true,
                    });
                    handle.owned = false;
                    false
                }
                _ => true,
            });
        drop(ownership);
        adopted.commit_keys();
        Ok(())
    }

    /// Idempotent cleanup, including after adoption or session close. An
    /// already-adopted receipt never releases the SDK's subsequently owned keys.
    pub fn discard(&self, receipt: TransferReceipt) -> Result<(), TransferError> {
        self.check_session(receipt)?;
        let ownership = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .staged
            .remove(&receipt.transfer_id);
        drop(ownership);
        Ok(())
    }

    /// Stop staging/adoption and discard pending outputs. This is transfer
    /// teardown, not cancellation or draining of still-running host work.
    pub fn close(&self) {
        let staged = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.closed = true;
            std::mem::take(&mut state.staged)
        };
        drop(staged);
    }

    pub fn pending_count(&self) -> usize {
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .staged
            .len()
    }

    /// A closed issuer cannot admit new calls through its retained SDK refs.
    /// This is an admission check, not a barrier against concurrent teardown;
    /// `stage` still rejects results if close wins after admission.
    pub fn is_closed(&self) -> bool {
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .closed
    }

    fn check_session(&self, receipt: TransferReceipt) -> Result<(), TransferError> {
        if receipt.session_id != self.inner.id {
            Err(TransferError::WrongSession)
        } else {
            Ok(())
        }
    }
}

/// The sender retains this guard until the transport accepts delivery. A
/// missing callback, dropped future, or transport error discards on drop.
#[must_use]
pub struct PendingDelivery<'a, T> {
    session: TransferSession<'a>,
    receipt: TransferReceipt,
    payload: Option<T>,
}

impl<T> PendingDelivery<'_, T> {
    pub fn payload(&self) -> &T {
        self.payload.as_ref().expect("payload exists until handoff")
    }

    pub fn receipt(&self) -> TransferReceipt {
        self.receipt
    }

    /// After transport acceptance, its receiver owes adoption or discard.
    /// Keep the session alive until receipt processing or explicit close.
    pub fn handoff(mut self) -> (TransferReceipt, T) {
        (
            self.receipt,
            self.payload.take().expect("handoff consumes the guard"),
        )
    }
}

impl<T> Drop for PendingDelivery<'_, T> {
    fn drop(&mut self) {
        if self.payload.is_some() {
            let _ = self.session.discard(self.receipt);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Barrier, Weak};

    use bex_project::{BexExternalValue, HostValueArc, HostValueKind, RuntimeTy};
    use indexmap::IndexMap;

    use super::*;
    use crate::baml_bridge::cffi::{BamlOutboundValue, baml_outbound_value::Value};
    use crate::{CffiHandleTableOptions, OutboundEncoder, encode_to_host_call};

    fn options(table: &CffiHandleTable) -> CffiHandleTableOptions<'_> {
        CffiHandleTableOptions {
            table,
            serialize_media: true,
            serialize_prompt_ast: true,
        }
    }

    fn encode<'a>(
        table: &'a CffiHandleTable,
        value: &BexExternalValue,
    ) -> EncodedTransfer<'a, BamlOutboundValue> {
        let mut encoder = OutboundEncoder::new(options(table));
        let value = encoder.encode(value).unwrap();
        encoder.finish(value)
    }

    fn key(value: &BamlOutboundValue) -> u64 {
        let Some(Value::HandleValue(handle)) = &value.value else {
            panic!("expected a handle")
        };
        handle.key
    }

    #[test]
    fn sender_drop_and_payload_panic_release_ownership() {
        let table = CffiHandleTable::new();
        let session = TransferSession::new(&table);
        let value = BexExternalValue::RustData(Arc::new(()));
        let encoded = encode(&table, &value);
        assert_eq!(table.len(), 1);
        drop(encoded);
        assert!(table.is_empty());

        let delivery = session.stage(encode(&table, &value)).unwrap();
        let receipt = delivery.receipt();
        assert_eq!(session.pending_count(), 1);
        drop(delivery); // e.g. callback absent, future abandoned, failed send
        assert!(table.is_empty());
        assert_eq!(session.pending_count(), 0);
        assert_eq!(session.adopt(receipt, &[]), Err(TransferError::NotPending));
        session.discard(receipt).unwrap();

        let encoded = encode(&table, &value);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                encoded.map_payload::<()>(|_| panic!("serialization failed"))
            }))
            .is_err()
        );
        assert!(table.is_empty());
    }

    #[test]
    fn adoption_removes_history_without_revoking_adopted_keys() {
        let table = CffiHandleTable::new();
        let session = TransferSession::new(&table);
        let resource = Arc::new(());
        let value = BexExternalValue::RustData(resource.clone());
        for _ in 0..128 {
            let delivery = session.stage(encode(&table, &value)).unwrap();
            let key = key(delivery.payload());
            let (receipt, _) = delivery.handoff();
            session.adopt(receipt, &[key]).unwrap();
            assert_eq!(session.pending_count(), 0);
            assert_eq!(
                session.adopt(receipt, &[key]),
                Err(TransferError::NotPending)
            );
            session.discard(receipt).unwrap();
            assert!(table.resolve(key).is_some());
            assert!(table.release(key));
            assert!(table.is_empty());
            assert_eq!(Arc::strong_count(&resource), 2);
        }
    }

    struct StubHeap;
    impl bex_project::WeakHeapRef for StubHeap {
        fn release_handle(&self, _: usize) {}
        fn resolve_handle_ptr(&self, _: usize) -> Option<bex_project::HeapPtr> {
            None
        }
    }

    #[test]
    fn claims_count_leases_and_release_unclaimed_cache_hits() {
        let table = CffiHandleTable::new();
        let session = TransferSession::new(&table);
        let value = BexExternalValue::Handle(bex_project::Handle::new(7, Arc::new(StubHeap)));
        let input = BexExternalValue::Array {
            element_type: RuntimeTy::unknown(),
            items: vec![value.clone(), value.clone()],
        };
        let delivery = session.stage(encode(&table, &input)).unwrap();
        let Some(Value::ListValue(values)) = &delivery.payload().value else {
            panic!("expected list")
        };
        let handle = key(&values.items[0]);
        assert_eq!(handle, key(&values.items[1]));
        let (receipt, _) = delivery.handoff();
        assert_eq!(
            session.adopt(receipt, &[handle, handle, handle]),
            Err(TransferError::InvalidClaims)
        );
        assert_eq!(session.pending_count(), 1);
        // Two decoded occurrences can use one cached wrapper and one lease.
        session.adopt(receipt, &[handle]).unwrap();
        assert!(table.release(handle));
        assert!(
            table.is_empty(),
            "unclaimed duplicate must already be released"
        );

        let delivery = session.stage(encode(&table, &input)).unwrap();
        let Some(Value::ListValue(values)) = &delivery.payload().value else {
            panic!("expected list")
        };
        let handle = key(&values.items[0]);
        let (receipt, _) = delivery.handoff();
        session.adopt(receipt, &[handle, handle]).unwrap();
        assert!(table.release(handle));
        assert!(table.resolve(handle).is_some());
        assert!(table.release(handle));
        assert!(table.is_empty());
    }

    #[test]
    fn wrong_session_table_and_claims_cannot_consume_pending_ownership() {
        let table = CffiHandleTable::new();
        let other_table = CffiHandleTable::new();
        let session = TransferSession::new(&table);
        let other_session = TransferSession::new(&table);
        let value = BexExternalValue::RustData(Arc::new(()));
        assert!(matches!(
            session.stage(encode(&other_table, &value)),
            Err(TransferError::WrongTable)
        ));
        assert!(other_table.is_empty());
        let (receipt, _) = session.stage(encode(&table, &value)).unwrap().handoff();
        assert_eq!(
            other_session.discard(receipt),
            Err(TransferError::WrongSession)
        );
        assert_eq!(
            other_session.adopt(receipt, &[]),
            Err(TransferError::WrongSession)
        );
        assert_eq!(
            session.adopt(receipt, &[u64::MAX]),
            Err(TransferError::InvalidClaims)
        );
        assert_eq!(session.pending_count(), 1);
        session.discard(receipt).unwrap();
        assert!(table.is_empty());
    }

    #[test]
    fn host_reference_adoption_retains_only_claimed_registrations() {
        let table = CffiHandleTable::new();
        let session = TransferSession::new(&table);
        let first = HostValueArc::new(9001, HostValueKind::Opaque);
        let second = HostValueArc::new(9002, HostValueKind::Callable);
        let first_weak = Arc::downgrade(&first);
        let second_weak = Arc::downgrade(&second);
        let mut encoder = OutboundEncoder::new(options(&table));
        let first_value = encoder.encode(&BexExternalValue::HostValue(first)).unwrap();
        encoder
            .encode(&BexExternalValue::HostValue(second))
            .unwrap();
        let (receipt, _) = session.stage(encoder.finish(())).unwrap().handoff();
        assert_eq!(table.len(), 2);
        assert_eq!(
            session.adopt(receipt, &[9001]),
            Err(TransferError::InvalidClaims)
        );
        session.adopt(receipt, &[key(&first_value)]).unwrap();
        assert!(first_weak.upgrade().is_some());
        assert!(second_weak.upgrade().is_none());
        assert_eq!(session.pending_count(), 0);
        // Closing the issuing session cannot revoke already adopted ownership.
        session.close();
        assert!(first_weak.upgrade().is_some());
        assert!(table.release(key(&first_value)));
        assert!(first_weak.upgrade().is_none());
        assert!(table.is_empty());
    }

    #[test]
    fn failed_attempt_rolls_back_before_encoding_fallback() {
        let table = CffiHandleTable::new();
        let session = TransferSession::new(&table);
        let mut encoder = OutboundEncoder::new(options(&table));
        let invalid = BexExternalValue::Array {
            element_type: RuntimeTy::unknown(),
            items: vec![
                BexExternalValue::RustData(Arc::new(())),
                BexExternalValue::union(
                    BexExternalValue::Bool(true),
                    [RuntimeTy::int()],
                    RuntimeTy::bool(),
                ),
            ],
        };
        assert!(encoder.encode(&invalid).is_err());
        assert!(table.is_empty());
        let fallback = encoder
            .encode(&BexExternalValue::String("encoding failed".into()))
            .unwrap();
        let (receipt, _) = session.stage(encoder.finish(fallback)).unwrap().handoff();
        session.adopt(receipt, &[]).unwrap();
        assert!(table.is_empty());
    }

    #[test]
    fn callback_aggregate_keeps_both_registries_until_discard() {
        let table = CffiHandleTable::new();
        let session = TransferSession::new(&table);
        let host = HostValueArc::new(9002, HostValueKind::Callable);
        let weak = Arc::downgrade(&host);
        let encoded = encode_to_host_call(
            &[BexExternalValue::RustData(Arc::new(()))],
            &IndexMap::from([("callback".into(), BexExternalValue::HostValue(host))]),
            options(&table),
        )
        .unwrap();
        let (receipt, _) = session.stage(encoded).unwrap().handoff();
        assert_eq!(table.len(), 2);
        assert!(weak.upgrade().is_some());
        session.discard(receipt).unwrap();
        assert!(table.is_empty());
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn close_racing_staging_cannot_leave_pending_owners() {
        let table = CffiHandleTable::new();
        for _ in 0..64 {
            let session = TransferSession::new(&table);
            let barrier = Barrier::new(2);
            std::thread::scope(|threads| {
                threads.spawn(|| {
                    let value = BexExternalValue::RustData(Arc::new(()));
                    let encoded = encode(&table, &value);
                    barrier.wait();
                    match session.stage(encoded) {
                        Ok(delivery) => {
                            delivery.handoff();
                        }
                        Err(error) => assert_eq!(error, TransferError::Closed),
                    }
                });
                barrier.wait();
                session.close();
            });
            assert_eq!(session.pending_count(), 0);
            assert!(table.is_empty());
        }
    }

    #[test]
    fn adoption_racing_discard_has_one_owner() {
        let table = CffiHandleTable::new();
        for _ in 0..64 {
            let session = TransferSession::new(&table);
            let delivery = session
                .stage(encode(&table, &BexExternalValue::RustData(Arc::new(()))))
                .unwrap();
            let key = key(delivery.payload());
            let (receipt, _) = delivery.handoff();
            let barrier = Barrier::new(2);
            let adopted = std::thread::scope(|threads| {
                let adoption = threads.spawn(|| {
                    barrier.wait();
                    session.adopt(receipt, &[key])
                });
                barrier.wait();
                session.discard(receipt).unwrap();
                adoption.join().unwrap()
            });
            match adopted {
                Ok(()) => assert!(table.release(key)),
                Err(error) => assert_eq!(error, TransferError::NotPending),
            }
            assert!(table.is_empty());
            assert_eq!(session.pending_count(), 0);
        }
    }

    #[test]
    fn close_invalidates_adoption_and_last_session_drop_discards() {
        let table = CffiHandleTable::new();
        let value = BexExternalValue::RustData(Arc::new(()));
        let session = TransferSession::new(&table);
        let (receipt, _) = session.stage(encode(&table, &value)).unwrap().handoff();
        session.close();
        session.close();
        assert_eq!(session.adopt(receipt, &[]), Err(TransferError::Closed));
        session.discard(receipt).unwrap();
        assert!(matches!(
            session.stage(encode(&table, &value)),
            Err(TransferError::Closed)
        ));
        assert!(table.is_empty());
        let session = TransferSession::new(&table);
        session.stage(encode(&table, &value)).unwrap().handoff();
        drop(session);
        assert!(table.is_empty());
    }

    #[test]
    fn discard_runs_resource_destructors_outside_session_lock() {
        struct Resource(
            Weak<SessionInner<'static>>,
            Arc<std::sync::atomic::AtomicBool>,
        );
        impl Drop for Resource {
            fn drop(&mut self) {
                let session = self.0.upgrade().unwrap();
                assert!(
                    session.state.try_lock().is_ok(),
                    "destructor must be able to reenter session"
                );
                self.1.store(true, Ordering::SeqCst);
            }
        }
        let session = TransferSession::new(&crate::HANDLE_TABLE);
        let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let value = BexExternalValue::RustData(Arc::new(Resource(
            Arc::downgrade(&session.inner),
            dropped.clone(),
        )));
        let delivery = session.stage(encode(&crate::HANDLE_TABLE, &value)).unwrap();
        drop(value);
        drop(delivery);
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[test]
    fn panicking_resource_destructor_does_not_orphan_sibling_leases() {
        struct PanicsOnDrop;
        impl Drop for PanicsOnDrop {
            fn drop(&mut self) {
                panic!("resource destructor failed");
            }
        }
        let table = CffiHandleTable::new();
        let session = TransferSession::new(&table);
        let sibling = Arc::new(());
        let weak = Arc::downgrade(&sibling);
        let value = BexExternalValue::Array {
            element_type: RuntimeTy::unknown(),
            items: vec![
                BexExternalValue::RustData(Arc::new(PanicsOnDrop)),
                BexExternalValue::RustData(sibling),
            ],
        };
        let delivery = session.stage(encode(&table, &value)).unwrap();
        drop(value);
        let (receipt, _) = delivery.handoff();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| session.discard(receipt)))
                .is_err()
        );
        assert_eq!(session.pending_count(), 0);
        assert!(table.is_empty());
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn adoption_cleanup_panic_rolls_back_provisional_claims() {
        struct PanicsOnDrop;
        impl Drop for PanicsOnDrop {
            fn drop(&mut self) {
                panic!("unclaimed resource destructor failed");
            }
        }
        let table = CffiHandleTable::new();
        let session = TransferSession::new(&table);
        let value = BexExternalValue::Array {
            element_type: RuntimeTy::unknown(),
            items: vec![
                BexExternalValue::RustData(Arc::new(())),
                BexExternalValue::RustData(Arc::new(PanicsOnDrop)),
            ],
        };
        let delivery = session.stage(encode(&table, &value)).unwrap();
        drop(value);
        let Some(Value::ListValue(values)) = &delivery.payload().value else {
            panic!("expected list")
        };
        let handle = key(&values.items[0]);
        let (receipt, _) = delivery.handoff();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || session.adopt(receipt, &[handle])
            ))
            .is_err()
        );
        assert_eq!(session.pending_count(), 0);
        assert!(
            table.is_empty(),
            "failed adoption cannot leave an unactivated SDK lease"
        );
    }
}
