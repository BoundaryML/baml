//! The handle table: where the objects a host holds are, for the
//! collector to keep current.
//!
//! A handle is the number of a slot. The slots are values in chunks of
//! [`CHUNK`] that are made as the table grows and never move or go away,
//! so the collector rewrites a slot through its address when it moves the
//! object. Slot zero is never handed out: zero is what every function
//! returns when it refuses.
//!
//! The collector reaches the table through [`each_slot`], from its own
//! root walk (`heap.rs`, `copy_into`), not through the worker's list of
//! roots: a build function may grow the table between two allocations.
//!
//! The table is its own static, apart from the host's state, and no
//! reference to it is held across an allocation: a collection reads it.

use std::sync::atomic::{AtomicPtr, Ordering};

use crate::{heap::Value, layout::is_pointer};

/// How many slots a chunk has.
const CHUNK: usize = 4096;

/// Set on a handle to an array or a map the host made with `array_new` or
/// `map_new` and has not passed anywhere yet. Only such a container is
/// accepted as an input, once: so no container lands in two typed places,
/// where BAML could write into one a value the other's type refuses.
pub(super) const FRESH: u8 = 1;

struct Table {
    /// The chunks, each [`CHUNK`] values.
    chunks: Vec<*mut Value>,
    /// The free slots, the next one handed out last.
    free: Vec<u32>,
    /// Per slot, [`FRESH`] or nothing.
    flags: Vec<u8>,
}

static TABLE: AtomicPtr<Table> = AtomicPtr::new(std::ptr::null_mut());

/// Make the table, with one chunk. Called once, at init.
pub(super) fn init() {
    let table = Box::into_raw(Box::new(Table {
        chunks: Vec::new(),
        free: Vec::new(),
        flags: Vec::new(),
    }));
    TABLE.store(table, Ordering::Release);
    grow();
}

/// The table, for one statement: no reference to it outlives a call that
/// can allocate.
fn table<'a>() -> &'a mut Table {
    let table = TABLE.load(Ordering::Acquire);
    assert!(!table.is_null(), "baml_lib_init has not run");
    // SAFETY: made at init and never freed; one thread uses it at a time,
    // and no reference to it is held across an allocation.
    unsafe { &mut *table }
}

/// Add a chunk of free slots.
fn grow() {
    let table = table();
    let first = table.chunks.len() * CHUNK;
    let chunk = Box::into_raw(vec![Value::NULL; CHUNK].into_boxed_slice()).cast::<Value>();
    table.chunks.push(chunk);
    table.flags.resize(first + CHUNK, 0);
    let first = u32::try_from(first).expect("fewer than four billion handles");
    // Slot zero is never a handle.
    let from = first.max(1);
    let to = first + u32::try_from(CHUNK).expect("a small chunk");
    table.free.extend((from..to).rev());
}

/// The address of slot `handle`, which exists.
fn slot(table: &Table, handle: usize) -> *mut Value {
    // SAFETY: the chunk exists and the offset is inside it.
    unsafe { table.chunks[handle / CHUNK].add(handle % CHUNK) }
}

/// A new handle holding `value`, an object, with `flags`. Never zero.
pub(super) fn hold(value: Value, flags: u8) -> usize {
    debug_assert!(is_pointer(value.tag));
    if table().free.is_empty() {
        grow();
    }
    let table = table();
    let handle = table.free.pop().expect("a chunk was just added") as usize;
    // SAFETY: a slot of the table.
    unsafe { slot(table, handle).write(value) };
    table.flags[handle] = flags;
    handle
}

/// The object `handle` holds, if it is a live handle.
pub(super) fn get(handle: u64) -> Option<Value> {
    let table = table();
    let handle = usize::try_from(handle).ok()?;
    if handle == 0 || handle >= table.chunks.len() * CHUNK {
        return None;
    }
    // SAFETY: a slot of the table.
    let value = unsafe { slot(table, handle).read() };
    is_pointer(value.tag).then_some(value)
}

/// The flags of `handle`, a live handle.
pub(super) fn flags(handle: u64) -> u8 {
    table().flags[index(handle)]
}

pub(super) fn set_flags(handle: u64, flags: u8) {
    table().flags[index(handle)] = flags;
}

/// The slot of `handle`, which [`get`] has found live.
fn index(handle: u64) -> usize {
    usize::try_from(handle).expect("a live handle is a slot")
}

/// Let go of `handle`. A handle that is not live is ignored.
pub(super) fn free(handle: u64) {
    if get(handle).is_none() {
        return;
    }
    let table = table();
    let handle = index(handle);
    // SAFETY: a slot of the table.
    unsafe { slot(table, handle).write(Value::NULL) };
    table.flags[handle] = 0;
    table
        .free
        .push(u32::try_from(handle).expect("checked against the table"));
}

/// How many handles are live: for tests.
pub(super) fn live() -> usize {
    let table = table();
    table.chunks.len() * CHUNK - 1 - table.free.len()
}

/// Hand `each` the address of every slot, for the collector to forward
/// what it holds. A free slot holds `null`, which it leaves alone.
pub(crate) fn each_slot(each: &mut dyn FnMut(usize)) {
    let table = TABLE.load(Ordering::Acquire);
    if table.is_null() {
        return;
    }
    // SAFETY: made at init and never freed; the collector runs with no
    // other reference to the table live.
    let table = unsafe { &*table };
    for chunk in &table.chunks {
        for index in 0..CHUNK {
            // SAFETY: inside the chunk.
            each(unsafe { chunk.add(index) } as usize);
        }
    }
}
