//! Rust values held on the BAML heap.
//!
//! A `$rust_type` field holds an [`Object::RustData`](crate::Object::RustData):
//! an `Arc` to a Rust value the VM cannot look inside. The collector treats it
//! as a leaf, so the only thing it needs from the value is how much memory it
//! keeps alive, and every payload type says so by implementing
//! [`BexRustData`]. There is no default: a type that cannot be sized cannot be
//! stored.

use std::{
    any::{Any, TypeId},
    fmt,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use crate::Meter;

/// A Rust value that can be stored on the heap.
///
/// # Measuring
///
/// [`measure`](Self::measure) reports the memory the value keeps alive beyond
/// its own allocation, which the heap reports itself: buffers it owns go to
/// [`Meter::bytes`], strings to [`Meter::string`], and allocations other values
/// may also hold to [`Meter::shared`]. It runs on the allocating VM's thread
/// when the value is stored, and on the collector's thread during a census,
/// while every VM is parked but detached tasks still run. So it must:
///
/// - never block: no lock, no await, no channel;
/// - never `try_lock` a lock that asynchronous code shares, because callers
///   read meaning into a failed `try_lock` (a value that grows off the VM
///   thread keeps a [`RetainedBytes`] its mutators maintain instead);
/// - never drop a last reference, since a `Drop` may take a lock;
/// - never follow a heap reference, allocate on the heap, or call back into
///   the VM, the engine, or the host;
/// - never panic.
///
/// Memory the heap, the engine, or the host owns is not reported here: a heap
/// handle, a type head, the engine itself, or a host object.
pub trait BexRustData: Any + Send + Sync {
    fn measure(&self, meter: &mut Meter);

    /// The name of the concrete type, for diagnostics.
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

impl dyn BexRustData {
    /// The value as `Any`, for the downcasts below.
    pub fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    /// The `TypeId` of the concrete value. Named so it cannot be confused
    /// with `Any::type_id` called on the `Arc` holding the value, which
    /// reports the `Arc`'s type.
    pub fn payload_type_id(&self) -> TypeId {
        self.as_any().type_id()
    }

    pub fn is<T: BexRustData>(&self) -> bool {
        self.as_any().is::<T>()
    }

    pub fn downcast_ref<T: BexRustData>(&self) -> Option<&T> {
        self.as_any().downcast_ref::<T>()
    }
}

impl fmt::Debug for dyn BexRustData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<rust data: {}>", self.type_name())
    }
}

/// Downcasting a shared payload to its concrete type.
pub trait RustDataArc: Sized {
    /// The payload as `Arc<T>`, sharing the allocation, or the original back.
    fn downcast_payload<T: BexRustData>(self) -> Result<Arc<T>, Self>;

    /// The `TypeId` of the concrete value held.
    fn payload_type_id(&self) -> TypeId;
}

impl RustDataArc for Arc<dyn BexRustData> {
    fn downcast_payload<T: BexRustData>(self) -> Result<Arc<T>, Self> {
        if !self.is::<T>() {
            return Err(self);
        }
        let any: Arc<dyn Any + Send + Sync> = self;
        any.downcast::<T>()
            .map_err(|_| unreachable!("the type was checked just above"))
    }

    fn payload_type_id(&self) -> TypeId {
        (**self).payload_type_id()
    }
}

/// Bytes a payload keeps alive that change after it is stored: whoever
/// mutates the payload keeps this current, and [`BexRustData::measure`] reads
/// it without touching the payload's own synchronization.
#[derive(Debug, Default)]
pub struct RetainedBytes(AtomicUsize);

impl RetainedBytes {
    pub const fn new(bytes: usize) -> Self {
        Self(AtomicUsize::new(bytes))
    }

    pub fn get(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }

    /// Publish the current size. Returns how much it grew, or zero.
    pub fn set(&self, bytes: usize) -> usize {
        let before = self.0.swap(bytes, Ordering::Relaxed);
        bytes.saturating_sub(before)
    }

    pub fn add(&self, bytes: usize) {
        self.0.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn sub(&self, bytes: usize) {
        self.0.fetch_sub(bytes, Ordering::Relaxed);
    }
}

/// A payload with nothing to measure, for tests that only need some
/// `RustData` object.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestRustData(pub u64);

impl BexRustData for TestRustData {
    fn measure(&self, _: &mut Meter) {}
}

// The resource crate sits below this one, so its payloads are sized here.

impl BexRustData for bex_resource_types::HostValueArc {
    fn measure(&self, _: &mut Meter) {
        // The value itself lives in the host.
    }
}

impl BexRustData for bex_resource_types::ResourceHandle {
    fn measure(&self, meter: &mut Meter) {
        // The resource lives in a registry the handle only names.
        meter.bytes(self.display_name().len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Payload {
        bytes: Vec<u8>,
    }

    impl BexRustData for Payload {
        fn measure(&self, meter: &mut Meter) {
            meter.bytes(self.bytes.capacity());
        }
    }

    #[test]
    fn type_ids_name_the_payload_not_the_arc() {
        let payload: Arc<dyn BexRustData> = Arc::new(Payload { bytes: vec![0; 10] });
        assert_eq!(payload.payload_type_id(), TypeId::of::<Payload>());
        assert_ne!(
            payload.payload_type_id(),
            TypeId::of::<Arc<dyn BexRustData>>()
        );
        assert!(payload.is::<Payload>());
        assert!(!payload.is::<TestRustData>());
        assert_eq!(payload.downcast_ref::<Payload>().unwrap().bytes.len(), 10);
        assert_eq!(
            format!("{payload:?}"),
            "<rust data: bex_vm_types::rust_data::tests::Payload>"
        );
    }

    #[test]
    fn downcasting_shares_the_allocation() {
        let payload: Arc<dyn BexRustData> = Arc::new(Payload { bytes: Vec::new() });
        let address = Arc::as_ptr(&payload).cast::<()>();
        let other = Arc::clone(&payload)
            .downcast_payload::<TestRustData>()
            .expect_err("wrong type comes back");
        assert_eq!(Arc::as_ptr(&other).cast::<()>(), address);
        let typed = other.downcast_payload::<Payload>().unwrap();
        assert_eq!(Arc::as_ptr(&typed).cast::<()>(), address);
        assert_eq!(Arc::strong_count(&typed), 2);
    }

    #[test]
    fn retained_bytes_reports_growth() {
        let retained = RetainedBytes::new(100);
        assert_eq!(retained.set(150), 50);
        assert_eq!(retained.set(120), 0);
        retained.add(5);
        retained.sub(25);
        assert_eq!(retained.get(), 100);
    }
}
