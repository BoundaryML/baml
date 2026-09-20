//! Scoped pointer translation for heap snapshots.
//!
//! A [`HeapPtr`] is a process-local address, so its Borsh impls refuse to run
//! (see `heap_ptr.rs`). A heap snapshot still has to write and read object
//! graphs, and every data-bearing `Object` variant already has a Borsh impl
//! that bottoms out in `HeapPtr`. This module lets the snapshot code reuse
//! those impls: while a translation context is installed on the current OS
//! thread, `HeapPtr` serializes as a [`SnapRef`] and deserializes from one.
//!
//! With no context installed the `HeapPtr` impls keep their original behavior
//! and return an error, so a malformed packed `Program` still fails.
//!
//! Two more things translate through the context. An `Object::RustData` that
//! the snapshot writer registered as a *resource* (a cancel token, a task
//! group) is written as its resource id and read back as the rebuilt value, so
//! that every handle of one resource restores to one resource. A `Future` is
//! written with its id, and the loader adds [`LoaderTable::future_id_base`] to
//! it, so that restored futures cannot collide with futures the target engine
//! has already issued.
//!
//! The context is a thread-local, installed by [`with_writer`] or
//! [`with_loader`] for the duration of one closure and removed afterwards,
//! also when the closure panics. Contexts do not nest.

use std::{any::Any, cell::RefCell, collections::HashMap, sync::Arc};

use borsh::{BorshDeserialize, BorshSerialize};

use crate::{HeapPtr, Object};

/// Position-independent form of a [`HeapPtr`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum SnapRef {
    /// The null sentinel (`HeapPtr::null()`), used by owner back-edges of
    /// compile-time declarations and similar optional pointers.
    Null,
    /// Index into the snapshot's object table.
    Runtime(u32),
    /// Index into the compile-time region of the heap. The region is built
    /// from the program, so the index means the same object in every process
    /// that loaded the identical program.
    CompileTime(u32),
}

/// The contiguous compile-time region of a heap, described without naming
/// `BexHeap` (which lives downstream of this crate).
#[derive(Clone, Copy, Debug, Default)]
pub struct CompileTimeRegion {
    /// Pointer to the compile-time object at index 0. Null when `len == 0`.
    pub base: HeapPtr,
    /// Number of compile-time objects.
    pub len: usize,
}

impl CompileTimeRegion {
    /// Index of `ptr` in the region, or `None` when it points elsewhere.
    #[must_use]
    pub fn index_of(&self, ptr: HeapPtr) -> Option<u32> {
        if self.len == 0 {
            return None;
        }
        let base = self.base.as_ptr() as usize;
        let addr = ptr.as_ptr() as usize;
        let size = std::mem::size_of::<Object>();
        let offset = addr.checked_sub(base)?;
        if offset % size != 0 {
            return None;
        }
        let index = offset / size;
        (index < self.len)
            .then(|| u32::try_from(index).ok())
            .flatten()
    }

    /// Pointer to the compile-time object at `index`, or `None` when the index
    /// is out of bounds.
    #[must_use]
    pub fn ptr_at(&self, index: u32) -> Option<HeapPtr> {
        let index = index as usize;
        if index >= self.len {
            return None;
        }
        // SAFETY: `base` points at element 0 of a contiguous allocation of
        // `len` objects (the caller's contract for `CompileTimeRegion`), and
        // `index < len`, so the offset stays inside that allocation.
        #[allow(unsafe_code)]
        let raw = unsafe { self.base.as_ptr().add(index) };
        // SAFETY: `raw` points at a live compile-time object, which is never
        // moved or collected.
        #[allow(unsafe_code)]
        #[cfg(not(feature = "heap_debug"))]
        let ptr = unsafe { HeapPtr::from_ptr(raw) };
        #[allow(unsafe_code)]
        #[cfg(feature = "heap_debug")]
        let ptr = unsafe { HeapPtr::from_ptr(raw, self.base.epoch()) };
        Some(ptr)
    }
}

/// Payload of an `Object::RustData`.
pub type RustDataRef = Arc<dyn Any + Send + Sync>;

/// Identity of a `RustData` payload: the address of its allocation. Every
/// clone of the `Arc` (a GC copy of the heap object, a handle the engine
/// holds) has the same key.
#[must_use]
pub fn resource_key(data: &RustDataRef) -> usize {
    Arc::as_ptr(data).cast::<()>() as usize
}

/// [`resource_key`] for a caller that holds the concrete `Arc`.
#[must_use]
pub fn resource_key_of<T>(data: &Arc<T>) -> usize {
    Arc::as_ptr(data).cast::<()>() as usize
}

/// Pointer-to-reference table used while writing a snapshot.
#[derive(Debug, Default)]
pub struct WriterTable {
    /// Dense ids of the runtime objects in the snapshot.
    pub runtime: HashMap<HeapPtr, u32>,
    /// Compile-time region of the source heap.
    pub compile_time: CompileTimeRegion,
    /// Ids of the `RustData` payloads the snapshot can rebuild, keyed by
    /// [`resource_key`].
    pub resources: HashMap<usize, u32>,
}

impl WriterTable {
    /// The reference that stands for `ptr`, or `None` when `ptr` is neither
    /// null, a compile-time object, nor a registered runtime object.
    #[must_use]
    pub fn encode(&self, ptr: HeapPtr) -> Option<SnapRef> {
        if ptr.is_null() {
            return Some(SnapRef::Null);
        }
        if let Some(index) = self.compile_time.index_of(ptr) {
            return Some(SnapRef::CompileTime(index));
        }
        self.runtime.get(&ptr).copied().map(SnapRef::Runtime)
    }
}

/// Reference-to-pointer table used while loading a snapshot.
#[derive(Default)]
pub struct LoaderTable {
    /// Address of each snapshot object in the target heap, indexed by id.
    pub runtime: Vec<HeapPtr>,
    /// Compile-time region of the target heap.
    pub compile_time: CompileTimeRegion,
    /// The rebuilt `RustData` payloads, indexed by resource id.
    pub resources: Vec<RustDataRef>,
    /// Added to the id of every restored future.
    pub future_id_base: usize,
}

impl std::fmt::Debug for LoaderTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoaderTable")
            .field("runtime", &self.runtime.len())
            .field("compile_time", &self.compile_time)
            .field("resources", &self.resources.len())
            .field("future_id_base", &self.future_id_base)
            .finish()
    }
}

impl LoaderTable {
    /// The pointer that `reference` stands for, or `None` when it is out of
    /// bounds.
    #[must_use]
    pub fn decode(&self, reference: SnapRef) -> Option<HeapPtr> {
        match reference {
            SnapRef::Null => Some(HeapPtr::null()),
            SnapRef::Runtime(id) => self.runtime.get(id as usize).copied(),
            SnapRef::CompileTime(index) => self.compile_time.ptr_at(index),
        }
    }
}

enum Context {
    Writer(WriterTable),
    Loader(LoaderTable),
}

thread_local! {
    static CONTEXT: RefCell<Option<Context>> = const { RefCell::new(None) };
}

/// Removes the context when dropped, so a panic inside the scoped closure
/// cannot leave translation enabled on the thread.
struct Uninstall;

impl Drop for Uninstall {
    fn drop(&mut self) {
        CONTEXT.with(|slot| slot.borrow_mut().take());
    }
}

fn install(context: Context) -> Uninstall {
    CONTEXT.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(
            slot.is_none(),
            "snapshot translation contexts do not nest on one thread"
        );
        *slot = Some(context);
    });
    Uninstall
}

/// Run `f` with `table` installed as the writer context of this thread.
/// Returns the closure result and hands the table back.
///
/// # Panics
///
/// Panics if a context is already installed on this thread.
pub fn with_writer<R>(table: WriterTable, f: impl FnOnce() -> R) -> (R, WriterTable) {
    let guard = install(Context::Writer(table));
    let result = f();
    let table = CONTEXT.with(|slot| slot.borrow_mut().take());
    drop(guard);
    match table {
        Some(Context::Writer(table)) => (result, table),
        _ => unreachable!("the writer context is owned by this scope"),
    }
}

/// Run `f` with `table` installed as the loader context of this thread.
///
/// # Panics
///
/// Panics if a context is already installed on this thread.
pub fn with_loader<R>(table: LoaderTable, f: impl FnOnce() -> R) -> R {
    let _guard = install(Context::Loader(table));
    f()
}

/// Whether a translation context is installed on this thread.
#[must_use]
pub fn is_active() -> bool {
    CONTEXT.with(|slot| slot.borrow().is_some())
}

fn invalid(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}

/// `HeapPtr::serialize` hook. `None` means no writer context is installed and
/// the caller keeps its default behavior.
pub(crate) fn encode_ptr(ptr: HeapPtr) -> Option<std::io::Result<SnapRef>> {
    CONTEXT.with(|slot| match &*slot.borrow() {
        Some(Context::Writer(table)) => Some(table.encode(ptr).ok_or_else(|| {
            invalid(format!(
                "snapshot writer reached a pointer outside its object table: {ptr:?}"
            ))
        })),
        _ => None,
    })
}

/// Whether a writer context is installed on this thread.
pub(crate) fn writer_active() -> bool {
    CONTEXT.with(|slot| matches!(&*slot.borrow(), Some(Context::Writer(_))))
}

/// `Object::RustData` serialize hook. `None` means no writer context is
/// installed.
pub(crate) fn encode_resource(data: &RustDataRef) -> Option<std::io::Result<u32>> {
    CONTEXT.with(|slot| match &*slot.borrow() {
        Some(Context::Writer(table)) => Some(
            table
                .resources
                .get(&resource_key(data))
                .copied()
                .ok_or_else(|| invalid("snapshot writer reached an unregistered Rust value")),
        ),
        _ => None,
    })
}

/// `Object::RustData` deserialize hook.
pub(crate) fn decode_resource(id: u32) -> std::io::Result<RustDataRef> {
    CONTEXT.with(|slot| match &*slot.borrow() {
        Some(Context::Loader(table)) => table
            .resources
            .get(id as usize)
            .cloned()
            .ok_or_else(|| invalid(format!("snapshot resource {id} out of bounds"))),
        _ => Err(invalid("no snapshot loader context is installed")),
    })
}

/// The offset a loader adds to restored future ids, or `None` without a
/// loader context.
pub(crate) fn loader_future_id_base() -> Option<usize> {
    CONTEXT.with(|slot| match &*slot.borrow() {
        Some(Context::Loader(table)) => Some(table.future_id_base),
        _ => None,
    })
}

/// Whether a loader context is installed on this thread. `HeapPtr`'s
/// deserializer checks this before it consumes any input, so that the default
/// error path reads nothing.
pub(crate) fn loader_active() -> bool {
    CONTEXT.with(|slot| matches!(&*slot.borrow(), Some(Context::Loader(_))))
}

/// `HeapPtr::deserialize` hook: map `reference` through the loader context.
pub(crate) fn decode_ptr(reference: SnapRef) -> std::io::Result<HeapPtr> {
    CONTEXT.with(|slot| match &*slot.borrow() {
        Some(Context::Loader(table)) => table
            .decode(reference)
            .ok_or_else(|| invalid(format!("snapshot reference out of bounds: {reference:?}"))),
        _ => Err(invalid("no snapshot loader context is installed")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region_over(objects: &mut [Object]) -> CompileTimeRegion {
        let raw = objects.as_mut_ptr();
        // SAFETY: the slice outlives the region in every test below.
        #[allow(unsafe_code)]
        #[cfg(not(feature = "heap_debug"))]
        let base = unsafe { HeapPtr::from_ptr(raw) };
        #[allow(unsafe_code)]
        #[cfg(feature = "heap_debug")]
        let base = unsafe { HeapPtr::from_ptr(raw, 0) };
        CompileTimeRegion {
            base,
            len: objects.len(),
        }
    }

    #[test]
    fn compile_time_pointers_round_trip_by_index() {
        let mut objects = vec![Object::Float(1.0), Object::Float(2.0), Object::Float(3.0)];
        let region = region_over(&mut objects);
        let second = region.ptr_at(1).expect("in bounds");

        let writer = WriterTable {
            compile_time: region,
            ..WriterTable::default()
        };
        let (bytes, _) = with_writer(writer, || borsh::to_vec(&second).expect("serializes"));
        assert_eq!(
            SnapRef::try_from_slice(&bytes).expect("a SnapRef"),
            SnapRef::CompileTime(1)
        );

        let loader = LoaderTable {
            compile_time: region,
            ..LoaderTable::default()
        };
        let decoded = with_loader(loader, || HeapPtr::try_from_slice(&bytes)).expect("decodes");
        assert_eq!(decoded, second);
        assert!(!is_active(), "the context is removed after the scope");
    }

    #[test]
    fn out_of_bounds_references_are_errors() {
        let mut objects = vec![Object::Float(1.0)];
        let region = region_over(&mut objects);
        for reference in [SnapRef::CompileTime(1), SnapRef::Runtime(0)] {
            let bytes = borsh::to_vec(&reference).expect("serializes");
            let loader = LoaderTable {
                compile_time: region,
                ..LoaderTable::default()
            };
            let err = with_loader(loader, || HeapPtr::try_from_slice(&bytes)).unwrap_err();
            assert!(err.to_string().contains("out of bounds"), "got: {err}");
        }
    }

    #[test]
    fn unknown_pointers_fail_to_serialize() {
        let mut objects = vec![Object::Float(1.0)];
        let region = region_over(&mut objects);
        let mut elsewhere = Object::Float(9.0);
        let stray = region_over(std::slice::from_mut(&mut elsewhere)).base;
        let writer = WriterTable {
            compile_time: region,
            ..WriterTable::default()
        };
        let (result, _) = with_writer(writer, || borsh::to_vec(&stray));
        assert!(result.is_err());
    }
}
