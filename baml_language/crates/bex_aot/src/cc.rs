//! Tracked handles and the cycle collector: Bacon and Rajan's synchronous
//! trial deletion, as Nim's ORC uses it.
//!
//! The backend decides per type whether a value can be part of a cycle
//! (a type that can reach itself through fields, elements, map values,
//! captured cells or a function value's environment). Those types are
//! [`Tracked`]: their handles ([`crate::handle::Cc`], [`crate::map::CcMap`],
//! [`crate::cell::CcCell`], [`crate::func::CcFunc`]) point to a box
//! counted here. Every other type stays a plain `Rc` ([`Untracked`]), which
//! this module never sees.
//!
//! # Counting
//!
//! A tracked object's header is 32 bits of state (the count, a color and
//! a buffered bit), the object's 32-bit index in the root buffer and a
//! pointer to its type's operations table: 16 bytes on a 64-bit target,
//! an `Rc`'s two counts. A copy increments. A drop decrements, and then:
//!
//! - at zero the object is freed: its value is dropped, it leaves the root
//!   buffer if it is in it (swapping the last root into its slot, by the
//!   index in its header), and its box is deallocated. Frees are
//!   iterative: a free that drops more handles to zero queues them, so a
//!   long chain never recurses;
//! - above zero it may have just become the last link into a garbage cycle,
//!   so it is buffered as a possible root (once: the buffered bit stays set
//!   until a collection takes it out).
//!
//! # Collecting
//!
//! [`collect`] runs when the buffer reaches [`threshold`] entries (which
//! adapts: it doubles while collections mostly retrace live data), when
//! called explicitly ([`crate::mem::collect`]), and under
//! `BAML_MEM_TORTURE=1` (with `mem-stats`) at every allocation and every
//! buffered decrement. From every buffered root it subtracts the counts the
//! objects reachable from it give each other (`mark_gray`); whatever keeps
//! a count is referenced from outside, and it and everything it reaches is
//! restored (`scan`, `scan_black`); what is left (white) is garbage only
//! itself references, and is freed. Every walk keeps its own stack.
//!
//! The collector only reads values, through [`Trace`] under a shared
//! borrow. An object that is mutably borrowed while a collection runs (a
//! decrement inside a store can trigger one) is read as having no edges,
//! which can only keep more objects alive. Untracked handles are not
//! followed: an untracked type is never on a cycle, so the edges through
//! one only make what they reach look referenced from outside. Collection
//! never runs while values are being freed or inside another collection.
//!
//! # Invariants
//!
//! What the `unsafe` below relies on:
//!
//! - **`Trace` reports each owned handle exactly once** (see
//!   [`crate::mem`]). `mark_gray` takes one count away per reported edge,
//!   so an extra edge would whiten a live object and free it; a missing one
//!   only keeps a cycle alive.
//! - **Every object in the root buffer has a nonzero count, and its
//!   header holds its index there.** A free takes a buffered object out of
//!   the buffer before deallocating its box, so the buffer never dangles.
//!   A collection takes the whole buffer and clears every root's buffered
//!   bit before it drops any value, so a free during a collection only
//!   ever looks up the new buffer.
//! - **Values are dropped only outside a collection's walks.** `freeing`
//!   queues frees that a drop triggers and blocks collections; `collecting`
//!   blocks nested ones. Dropping a value (which drops the handles it
//!   holds) therefore never frees a box a walk is about to visit.
//! - **One thread.** Handles are `!Send`, the state lives in a
//!   `thread_local!`, and counts are plain `Cell`s.
//!
//! # Why this design
//!
//! The backend emits code that clones and drops handles freely, and a BAML
//! value's lifetime is mostly lexical, so reference counting frees almost
//! everything at the earliest point with no pause and no roots to find:
//! temporaries, in-flight errors and array snapshots all hold counts. What
//! counting alone cannot do is free a cycle, and only a few types can form
//! one (about 1% of the corpus's heap types), so only those pay for the
//! buffer and the trial deletion: a program with no cycle-capable type runs
//! exactly as it did under plain `Rc`. Trial deletion is local to the
//! buffered roots, so its work is proportional to what they reach, not to
//! the whole heap. Frees are iterative because `Rc`'s recursive drop
//! overflowed the stack on long lists.
//!
//! Settings, read once per thread: `BAML_CC_THRESHOLD=n` (the buffer
//! length that triggers a collection; the floor of the adaptive
//! threshold), `BAML_CC_ADAPTIVE=0` (keep it fixed), `BAML_CC_STATS=1`
//! (print [`report`] to stderr when the thread exits).
//!
//! [`Untracked`]: crate::handle::Untracked

// The tracked box is a raw allocation with a hand-rolled count: the one
// place in this crate that needs `unsafe` (besides `mem-stats`' read of a
// box through `inspect`). Every block states what it relies on.
#![allow(unsafe_code)]

use std::{
    alloc::Layout,
    cell::{Cell, RefCell},
    marker::PhantomData,
    mem::ManuallyDrop,
    ptr::NonNull,
};

use crate::{
    func::Callable,
    handle::Kind,
    mem::{HeapRef, Trace, Visitor},
};

/// The kind of a handle the cycle collector examines. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tracked {}

/// The state word: the count in the low bits, then the color, the buffered
/// bit and the acyclic bit in the top four bits, so a count stays below
/// 2^28 (a copy past that aborts, as `Rc`'s count overflow does). 32 bits,
/// so that with the root index beside it the header is two words.
type State = u32;
const COLOR_SHIFT: u32 = State::BITS - 4;
const RC_MASK: State = (1 << COLOR_SHIFT) - 1;
const COLOR_MASK: State = 0b11 << COLOR_SHIFT;
/// In use, or found live by the last collection.
const BLACK: State = 0;
/// Reached by `mark_gray`; its count has had the internal references taken
/// away.
const GRAY: State = 1 << COLOR_SHIFT;
/// Garbage, once `scan` is done.
const WHITE: State = 2 << COLOR_SHIFT;
/// Garbage being freed by `collect`: its count is the sentinel below, and
/// drops of handles to it change nothing else.
const FREEING: State = 3 << COLOR_SHIFT;
/// In the root buffer.
const BUFFERED: State = 1 << (State::BITS - 2);
/// Never on a cycle: a function value's object that holds no tracked
/// handle. Allocated with `BUFFERED` set too, so a decrement never buffers
/// it; a free deallocates it whatever `BUFFERED` says.
const ACYCLIC: State = 1 << (State::BITS - 1);
/// The count a white object is given while its cycle is freed, so the
/// decrements the cycle's own handles make never reach zero.
const SENTINEL: State = 1 << (COLOR_SHIFT - 1);

/// An object's header, at the start of its box.
#[repr(C)]
pub(crate) struct Header {
    state: Cell<State>,
    /// The object's index in the root buffer while `BUFFERED` is set (and
    /// `ACYCLIC` is not): a free takes it out of the buffer in O(1).
    root: Cell<u32>,
    ops: &'static Ops,
}

/// A tracked object: its header, then its value.
#[repr(C)]
pub(crate) struct CcBox<T: ?Sized> {
    header: Header,
    value: T,
}

/// What the collector can do with an object it only has the header of: the
/// value's type, erased.
pub(crate) struct Ops {
    /// Visit the handles in the value.
    trace: unsafe fn(NonNull<Header>, &mut dyn Visitor),
    /// Drop the value in place, leaving the box allocated.
    drop_value: unsafe fn(NonNull<Header>),
    /// Free the box, whose value is already dropped.
    dealloc: unsafe fn(NonNull<Header>),
}

struct OpsOf<T>(PhantomData<T>);

impl<T: Trace> OpsOf<T> {
    const OPS: Ops = Ops {
        trace: trace_value::<T>,
        drop_value: drop_value::<T>,
        dealloc: dealloc_box::<T>,
    };
}

/// # Safety
/// `header` heads a live `CcBox<T>` whose value is not dropped.
unsafe fn trace_value<T: Trace>(header: NonNull<Header>, visitor: &mut dyn Visitor) {
    // SAFETY: the caller's contract; the value is only read.
    unsafe { (*header.cast::<CcBox<T>>().as_ptr()).value.trace(visitor) }
}

/// # Safety
/// `header` heads a `CcBox<T>` whose value is not dropped and is not
/// referenced; it is never read again.
unsafe fn drop_value<T>(header: NonNull<Header>) {
    // SAFETY: the caller's contract.
    unsafe { std::ptr::drop_in_place(&raw mut (*header.cast::<CcBox<T>>().as_ptr()).value) }
}

/// # Safety
/// `header` heads a `CcBox<T>` allocated by [`alloc`] whose value is
/// dropped; nothing points to it any more.
unsafe fn dealloc_box<T>(header: NonNull<Header>) {
    #[cfg(feature = "mem-stats")]
    crate::mem::on_cc_dealloc(header);
    // SAFETY: the box was allocated by `Box::new` with this layout.
    unsafe { std::alloc::dealloc(header.as_ptr().cast(), Layout::new::<CcBox<T>>()) }
}

/// The collector's view of one tracked object: what [`HeapRef::object`]
/// returns for a tracked handle.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Object(NonNull<Header>);

/// The owning pointer behind a tracked handle.
#[doc(hidden)]
pub struct Ptr<T: ?Sized> {
    ptr: NonNull<CcBox<T>>,
    owns: PhantomData<CcBox<T>>,
}

impl<T: ?Sized> Ptr<T> {
    #[inline]
    fn header(&self) -> &Header {
        // SAFETY: a handle keeps its box allocated; the header is never
        // mutably borrowed (it is all `Cell`s).
        unsafe { &(*self.ptr.as_ptr()).header }
    }
}

impl<T: ?Sized> Clone for Ptr<T> {
    #[inline]
    fn clone(&self) -> Self {
        let state = &self.header().state;
        let next = state.get() + 1;
        if next & RC_MASK == 0 {
            // The count overflowed into the color bits, as `Rc` aborts.
            std::process::abort();
        }
        state.set(next);
        Self {
            ptr: self.ptr,
            owns: PhantomData,
        }
    }
}

impl<T: ?Sized> Drop for Ptr<T> {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: this handle owned one count of a live box.
        unsafe { decrement(self.ptr.cast()) }
    }
}

/// Drop one count of the object at `header`.
///
/// # Safety
/// The caller owned that count.
#[inline]
unsafe fn decrement(header: NonNull<Header>) {
    // SAFETY: the count kept the box allocated until now.
    let state = unsafe { &header.as_ref().state };
    let next = state.get() - 1;
    state.set(next);
    if next & RC_MASK == 0 {
        // SAFETY: that was the last count.
        unsafe { released(header) }
    } else if next & BUFFERED == 0 {
        possible_root(header);
    }
}

/// Allocate `value` in a tracked box with a count of one.
fn alloc<T: Trace>(value: T, acyclic: bool) -> Ptr<T> {
    let state = if acyclic { 1 | ACYCLIC | BUFFERED } else { 1 };
    let boxed = Box::new(CcBox {
        header: Header {
            state: Cell::new(state),
            root: Cell::new(0),
            ops: &OpsOf::<T>::OPS,
        },
        value,
    });
    // SAFETY: `Box::into_raw` is never null.
    let ptr = unsafe { NonNull::new_unchecked(Box::into_raw(boxed)) };
    #[cfg(feature = "mem-stats")]
    crate::mem::on_alloc(crate::mem::Registered::Cc(ptr.cast()));
    Ptr {
        ptr,
        owns: PhantomData,
    }
}

impl Kind for Tracked {
    type Ptr<T: ?Sized> = Ptr<T>;

    #[inline]
    fn alloc<T: Trace>(value: T) -> Ptr<T> {
        alloc(value, false)
    }

    #[inline]
    fn alloc_callable<A: 'static, R: 'static, C: Callable<A, R>>(
        object: C,
        acyclic: bool,
    ) -> Ptr<dyn Callable<A, R>> {
        let sized = ManuallyDrop::new(alloc(object, acyclic));
        let ptr: NonNull<CcBox<dyn Callable<A, R>>> = sized.ptr;
        Ptr {
            ptr,
            owns: PhantomData,
        }
    }

    #[inline]
    fn get<T: ?Sized>(ptr: &Ptr<T>) -> &T {
        // SAFETY: a handle keeps its value alive, and the value is only
        // dropped once no handle is left.
        unsafe { &(*ptr.ptr.as_ptr()).value }
    }

    #[inline]
    fn addr<T: ?Sized>(ptr: &Ptr<T>) -> *const () {
        ptr.ptr.as_ptr().cast_const().cast::<()>()
    }

    #[inline]
    fn strong_count<T: ?Sized>(ptr: &Ptr<T>) -> usize {
        (ptr.header().state.get() & RC_MASK) as usize
    }

    #[inline]
    fn object<T: ?Sized>(ptr: &Ptr<T>) -> Option<Object> {
        Some(Object(ptr.ptr.cast()))
    }
}

/// Counters for the collector on this thread.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    /// Collections that had roots to examine.
    pub collections: u64,
    /// Buffered roots those collections took (live or already freed).
    pub roots: u64,
    /// Objects `mark_gray` reached: the work trial deletion did.
    pub traced: u64,
    /// Objects freed as garbage cycles.
    pub freed: u64,
    /// Total time inside collections, in nanoseconds.
    pub pause_total_ns: u64,
    /// The longest collection, in nanoseconds.
    pub pause_max_ns: u64,
}

/// The collector's state for this thread: a native program runs on one.
struct Gc {
    /// Possible roots: live objects decremented to a nonzero count since
    /// the last collection.
    roots: RefCell<Vec<NonNull<Header>>>,
    /// Objects whose count reached zero while another free was running.
    pending: RefCell<Vec<NonNull<Header>>>,
    /// Whether values are being dropped: frees queue, collections wait.
    freeing: Cell<bool>,
    /// Whether a collection is running.
    collecting: Cell<bool>,
    /// The buffer length that triggers a collection; 0 until the settings
    /// are read, which the first buffered root does.
    threshold: Cell<usize>,
    /// The configured threshold: where an adaptive one starts and the
    /// lowest it goes.
    floor: Cell<usize>,
    /// Whether the threshold adapts to how much the collections free.
    adaptive: Cell<bool>,
    stats: Cell<Stats>,
    /// Every pause, in nanoseconds, when `BAML_CC_STATS=1`.
    pauses: RefCell<Option<Vec<u64>>>,
}

thread_local! {
    static GC: Gc = const {
        Gc {
            roots: RefCell::new(Vec::new()),
            pending: RefCell::new(Vec::new()),
            freeing: Cell::new(false),
            collecting: Cell::new(false),
            threshold: Cell::new(0),
            floor: Cell::new(0),
            adaptive: Cell::new(false),
            stats: Cell::new(Stats {
                collections: 0,
                roots: 0,
                traced: 0,
                freed: 0,
                pause_total_ns: 0,
                pause_max_ns: 0,
            }),
            pauses: RefCell::new(None),
        }
    };
}

/// The default buffer length that triggers a collection. See
/// [`threshold`].
pub const DEFAULT_THRESHOLD: usize = 10_000;
/// The adaptive threshold's ceiling.
const MAX_THRESHOLD: usize = 1 << 22;

impl Drop for Gc {
    // `BAML_CC_STATS=1` asks for the report on stderr: the measurement hook.
    #[allow(clippy::print_stderr)]
    fn drop(&mut self) {
        // Buffered roots are live (garbage nobody collected is left to the
        // process); objects freed from now on find no buffer to leave.
        if let Some(pauses) = self.pauses.get_mut() {
            eprintln!("{}", report_text(self.stats.get(), pauses));
        }
    }
}

impl Gc {
    /// Read the settings once: `BAML_CC_THRESHOLD=n` sets the threshold
    /// (the floor of an adaptive one), `BAML_CC_ADAPTIVE=0` fixes it,
    /// `BAML_CC_STATS=1` records every pause and prints a report when the
    /// thread exits.
    #[cold]
    fn configure(&self) {
        let var = |name: &str| std::env::var(name).ok();
        let threshold = var("BAML_CC_THRESHOLD")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_THRESHOLD);
        self.threshold.set(threshold);
        self.floor.set(threshold);
        self.adaptive
            .set(var("BAML_CC_ADAPTIVE").is_none_or(|value| value != "0"));
        if var("BAML_CC_STATS").is_some_and(|value| value == "1") {
            *self.pauses.borrow_mut() = Some(Vec::new());
        }
    }
}

/// The object's count reached zero: free it, or queue it when a free is
/// already running.
///
/// # Safety
/// No handle to the object is left.
#[cold]
#[inline(never)]
unsafe fn released(header: NonNull<Header>) {
    let queued = GC.try_with(|gc| {
        if gc.freeing.get() {
            gc.pending.borrow_mut().push(header);
            return true;
        }
        gc.freeing.set(true);
        // SAFETY: the caller's contract, and every queued object's.
        unsafe { free(Some(gc), header) };
        loop {
            let next = gc.pending.borrow_mut().pop();
            match next {
                // SAFETY: queued objects had no handle left either.
                Some(next) => unsafe { free(Some(gc), next) },
                None => break,
            }
        }
        gc.freeing.set(false);
        if gc.roots.borrow().len() >= gc.threshold.get() {
            maybe_collect(gc);
        }
        true
    });
    if queued.is_err() {
        // The thread is exiting and its collector (with its buffer) is
        // gone: free directly.
        // SAFETY: the caller's contract.
        unsafe { free(None, header) };
    }
}

/// Drop the value of an object no handle reaches, take it out of the root
/// buffer if it is there (`gc` is `None` once the thread's collector is
/// gone, buffer and all), and deallocate its box.
///
/// # Safety
/// No handle to the object is left and its value is not dropped.
unsafe fn free(gc: Option<&Gc>, header: NonNull<Header>) {
    // SAFETY: the caller's contract.
    unsafe {
        let ops = header.as_ref().ops;
        (ops.drop_value)(header);
        #[cfg(feature = "mem-stats")]
        crate::mem::on_free();
        let state = header.as_ref().state.get();
        if let Some(gc) = gc
            && state & BUFFERED != 0
            && state & ACYCLIC == 0
        {
            unbuffer(gc, header);
        }
        (ops.dealloc)(header);
    }
}

/// Take a buffered object out of the root buffer, moving the last root
/// into its slot.
fn unbuffer(gc: &Gc, header: NonNull<Header>) {
    // SAFETY: the object's box is allocated (its free is running).
    let index = unsafe { header.as_ref().root.get() };
    let slot = index as usize;
    let mut roots = gc.roots.borrow_mut();
    debug_assert_eq!(roots.get(slot), Some(&header), "a stale root index");
    roots.swap_remove(slot);
    if let Some(&moved) = roots.get(slot) {
        // SAFETY: buffered objects are allocated.
        unsafe { moved.as_ref().root.set(index) };
    }
}

/// The object's count went down and it lives: buffer it as a possible root
/// of a garbage cycle, and collect once the buffer is full.
#[cold]
#[inline(never)]
fn possible_root(header: NonNull<Header>) {
    // SAFETY: the object is alive (its count is not zero).
    let state = unsafe { &header.as_ref().state };
    // A white being freed has `BUFFERED` set, so `decrement` never gets here.
    debug_assert_ne!(color(state.get()), FREEING);
    let _ = GC.try_with(|gc| {
        state.set(state.get() | BUFFERED);
        let len = {
            let mut roots = gc.roots.borrow_mut();
            let Ok(index) = u32::try_from(roots.len()) else {
                // 2^32 buffered roots: as a count overflow, abort.
                std::process::abort();
            };
            // SAFETY: as above.
            unsafe { header.as_ref().root.set(index) };
            roots.push(header);
            roots.len()
        };
        if len >= gc.threshold.get() {
            maybe_collect(gc);
        }
        if torture() {
            collect_in(gc);
        }
    });
}

/// Run the collector over the buffered roots. A no-op while values are being
/// freed or a collection is running (the buffer waits for the next one).
pub fn collect() {
    let _ = GC.try_with(collect_in);
}

/// This thread's collector counters.
pub fn stats() -> Stats {
    GC.try_with(|gc| gc.stats.get()).unwrap_or_default()
}

/// The buffer length that triggers the next collection. It starts at
/// `BAML_CC_THRESHOLD` if set, else [`DEFAULT_THRESHOLD`], and adapts
/// (unless `BAML_CC_ADAPTIVE=0`): it doubles after a collection that freed
/// less than half of what it traced while tracing more than four objects
/// per live root, and shrinks by a third, never below where it started,
/// after one that freed more.
pub fn threshold() -> usize {
    GC.try_with(|gc| {
        if gc.threshold.get() == 0 {
            gc.configure();
        }
        gc.threshold.get()
    })
    .unwrap_or(DEFAULT_THRESHOLD)
}

/// Set the trigger threshold (and the adaptive one's floor) for this
/// thread: tests and measurements.
pub fn set_threshold(threshold: usize, adaptive: bool) {
    let _ = GC.try_with(|gc| {
        if gc.threshold.get() == 0 {
            gc.configure();
        }
        gc.threshold.set(threshold.max(1));
        gc.floor.set(threshold.max(1));
        gc.adaptive.set(adaptive);
    });
}

/// The collector's counters and pause distribution as text: what
/// `BAML_CC_STATS=1` prints when the thread exits.
pub fn report() -> String {
    GC.try_with(|gc| {
        let pauses = gc.pauses.borrow();
        report_text(gc.stats.get(), pauses.as_deref().unwrap_or(&[]))
    })
    .unwrap_or_default()
}

fn report_text(stats: Stats, pauses: &[u64]) -> String {
    let mut sorted = pauses.to_vec();
    sorted.sort_unstable();
    let p99 = if sorted.is_empty() {
        0
    } else {
        sorted[(sorted.len() * 99).div_ceil(100).saturating_sub(1)]
    };
    format!(
        "baml-cc collections={} roots={} traced={} freed={} pause_total_ns={} pause_max_ns={} pause_p99_ns={}",
        stats.collections,
        stats.roots,
        stats.traced,
        stats.freed,
        stats.pause_total_ns,
        stats.pause_max_ns,
        p99
    )
}

/// The state word of an object the collector holds (a buffered root, or
/// one reached from it during a collection: allocated by the invariants
/// above). Read and written by value, so no reference outlives the call.
#[inline]
fn state(header: NonNull<Header>) -> State {
    // SAFETY: the box is allocated; the header is only ever shared.
    unsafe { header.as_ref().state.get() }
}

#[inline]
fn set_state(header: NonNull<Header>, state: State) {
    // SAFETY: as in `state`.
    unsafe { header.as_ref().state.set(state) }
}

#[inline]
fn color(state: State) -> State {
    state & COLOR_MASK
}

#[inline]
fn set_color(header: NonNull<Header>, color: State) {
    set_state(header, (state(header) & !COLOR_MASK) | color);
}

/// The collector's visitor: every tracked child, by header.
struct Children<F>(F);

impl<F: FnMut(NonNull<Header>)> Visitor for Children<F> {
    #[inline]
    fn visit(&mut self, handle: &dyn HeapRef) {
        // An edge reported without its object at hand: a tracked handle
        // traced through some other path, or (usually) an untracked one.
        if let Some(Object(child)) = handle.object() {
            (self.0)(child);
        }
    }

    #[inline]
    fn visit_tracked(&mut self, Object(child): Object, _handle: &dyn HeapRef) {
        (self.0)(child);
    }
}

/// Visit the tracked objects `header`'s value holds handles to.
#[inline]
fn children(header: NonNull<Header>, each: impl FnMut(NonNull<Header>)) {
    // SAFETY: the collector only traces live boxes whose values are not
    // dropped.
    unsafe {
        let ops = header.as_ref().ops;
        (ops.trace)(header, &mut Children(each));
    }
}

/// Collect if the buffer has reached the threshold (reading the settings
/// the first time).
#[cold]
fn maybe_collect(gc: &Gc) {
    if gc.threshold.get() == 0 {
        gc.configure();
    }
    if gc.roots.borrow().len() >= gc.threshold.get() {
        collect_in(gc);
    }
}

fn collect_in(gc: &Gc) {
    if gc.freeing.get() || gc.collecting.get() {
        return;
    }
    let roots = std::mem::take(&mut *gc.roots.borrow_mut());
    if roots.is_empty() {
        return;
    }
    gc.collecting.set(true);
    let start = clock::now();
    let mut stats = gc.stats.get();
    stats.collections += 1;
    stats.roots += roots.len() as u64;

    // A free takes its object out of the buffer, so every root is live.
    let live = roots;
    debug_assert!(live.iter().all(|&root| state(root) & RC_MASK > 0));
    // Mark: subtract the internal counts below every live root.
    let mut stack = Vec::new();
    let mut traced = 0;
    for &header in &live {
        traced += mark_gray(header, &mut stack);
    }
    // Scan: what keeps a count is live, with everything it reaches.
    let mut blacken = Vec::new();
    for &header in &live {
        scan(header, &mut stack, &mut blacken);
    }
    // Collect: gather the white objects, giving each the sentinel count.
    let mut whites = Vec::new();
    for &header in &live {
        set_state(header, state(header) & !BUFFERED);
        collect_white(header, &mut whites, &mut stack);
    }
    stats.traced += traced;
    stats.freed += whites.len() as u64;
    // Free: drop every white value, then deallocate the boxes. Drops of
    // handles into the cycle meet the sentinel; drops of handles out of it
    // free or buffer as usual.
    gc.freeing.set(true);
    for &header in &whites {
        // SAFETY: white objects are referenced only by each other, and each
        // value is dropped once.
        unsafe { (header.as_ref().ops.drop_value)(header) };
        #[cfg(feature = "mem-stats")]
        crate::mem::on_free();
    }
    loop {
        let next = gc.pending.borrow_mut().pop();
        match next {
            // SAFETY: queued objects had no handle left. Every root of this
            // collection had its buffered bit cleared above, so a buffered
            // one is in the new buffer.
            Some(next) => unsafe { free(Some(gc), next) },
            None => break,
        }
    }
    for &header in &whites {
        // SAFETY: every value in the cycle is dropped; nothing points to
        // the boxes any more.
        unsafe { (header.as_ref().ops.dealloc)(header) };
    }
    gc.freeing.set(false);
    gc.collecting.set(false);

    let pause = clock::elapsed_ns(start);
    stats.pause_total_ns += pause;
    stats.pause_max_ns = stats.pause_max_ns.max(pause);
    gc.stats.set(stats);
    if let Some(pauses) = gc.pauses.borrow_mut().as_mut() {
        pauses.push(pause);
    }
    if gc.adaptive.get() {
        // Nim's rule, floored at the configured threshold and with growth
        // kept for collections that retrace much more than they were given:
        // one that freed at least half of what it traced brings the
        // threshold back down by a third; one that freed less and traced
        // more than four objects per live root (each root reaching a large
        // live structure, as every old head of a list built by prepending
        // does) doubles it, so retracing stays n log n rather than
        // quadratic. Anything else keeps it: tracing about one object per
        // root is the cost of the roots themselves, and a bigger buffer
        // only makes the pauses longer.
        let threshold = gc.threshold.get();
        let freed = whites.len() as u64;
        let next = if freed * 2 >= traced {
            (threshold / 3 * 2).max(gc.floor.get())
        } else if traced > 4 * live.len() as u64 {
            threshold.saturating_mul(2).min(MAX_THRESHOLD)
        } else {
            threshold
        };
        gc.threshold.set(next);
    }
}

/// Pause timing. `std::time::Instant` panics on wasm32-unknown-unknown,
/// which has no clock without JavaScript; there every pause reads as 0.
mod clock {
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    pub(super) fn now() -> std::time::Instant {
        std::time::Instant::now()
    }

    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    pub(super) fn elapsed_ns(start: std::time::Instant) -> u64 {
        u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    pub(super) struct NoClock;

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    pub(super) fn now() -> NoClock {
        NoClock
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    pub(super) fn elapsed_ns(_start: NoClock) -> u64 {
        0
    }
}

#[cfg(feature = "mem-stats")]
fn torture() -> bool {
    crate::mem::torture()
}

#[cfg(not(feature = "mem-stats"))]
fn torture() -> bool {
    false
}

/// Gray everything reachable from `root`, taking one count away for every
/// edge followed. Returns how many objects it grayed.
fn mark_gray(root: NonNull<Header>, stack: &mut Vec<NonNull<Header>>) -> u64 {
    if color(state(root)) == GRAY {
        return 0;
    }
    set_color(root, GRAY);
    stack.push(root);
    let mut grayed = 1;
    while let Some(header) = stack.pop() {
        children(header, |child| {
            let next = state(child) - 1;
            set_state(child, next);
            if color(next) != GRAY {
                set_state(child, (next & !COLOR_MASK) | GRAY);
                stack.push(child);
                grayed += 1;
            }
        });
    }
    grayed
}

/// Whiten the gray objects below `root` that kept no count, and blacken
/// (restoring counts) what a counted one reaches.
/// `blacken` is `scan_black`'s stack.
fn scan(
    root: NonNull<Header>,
    stack: &mut Vec<NonNull<Header>>,
    blacken: &mut Vec<NonNull<Header>>,
) {
    stack.push(root);
    while let Some(header) = stack.pop() {
        let current = state(header);
        if color(current) != GRAY {
            continue;
        }
        if current & RC_MASK > 0 {
            scan_black(header, blacken);
        } else {
            set_color(header, WHITE);
            children(header, |child| stack.push(child));
        }
    }
}

/// Blacken `root` and everything below it, giving back the counts
/// `mark_gray` took.
fn scan_black(root: NonNull<Header>, stack: &mut Vec<NonNull<Header>>) {
    set_color(root, BLACK);
    stack.push(root);
    while let Some(header) = stack.pop() {
        children(header, |child| {
            let next = state(child) + 1;
            set_state(child, next);
            if color(next) != BLACK {
                set_state(child, (next & !COLOR_MASK) | BLACK);
                stack.push(child);
            }
        });
    }
}

/// Gather the white objects below `root` that are not still buffered (a
/// buffered one is gathered from its own root; an acyclic one is never in
/// the buffer), marking each as being freed, and give back the count of
/// every edge out of it.
fn collect_white(
    root: NonNull<Header>,
    whites: &mut Vec<NonNull<Header>>,
    stack: &mut Vec<NonNull<Header>>,
) {
    stack.push(root);
    while let Some(header) = stack.pop() {
        let current = state(header);
        if color(current) == WHITE && (current & BUFFERED == 0 || current & ACYCLIC != 0) {
            set_state(header, SENTINEL | BUFFERED | FREEING);
            whites.push(header);
            children(header, |child| {
                // `mark_gray` took this edge's count away and nothing gave
                // it back; the white value's drop takes it away again, so
                // give it back now. A child that is garbage too gets the
                // sentinel (overwriting this) when it is gathered.
                let count = state(child);
                if color(count) != FREEING {
                    set_state(child, count + 1);
                }
                stack.push(child);
            });
        }
    }
}

/// The count of a tracked object and the tracked or untracked handles its
/// value holds, for the `mem-stats` registry. `None` once the value is
/// dropped.
///
/// # Safety
/// `header` heads a box that is still allocated.
#[cfg(feature = "mem-stats")]
pub(crate) unsafe fn inspect(header: NonNull<Header>, visitor: &mut dyn Visitor) -> Option<usize> {
    // SAFETY: the caller's contract.
    unsafe {
        let count = header.as_ref().state.get() & RC_MASK;
        if count == 0 || color(header.as_ref().state.get()) == FREEING {
            return None;
        }
        (header.as_ref().ops.trace)(header, visitor);
        Some(count as usize)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell as StdCell;

    use super::*;
    use crate::{
        Thrown,
        cell::{self, CcCell},
        func::{self, Callable, CcFunc},
        handle::{Cc, Shared, cc, shared},
        mem::trace_fields,
    };

    thread_local! {
        /// Values of `Probe` dropped on this thread.
        static DROPPED: StdCell<usize> = const { StdCell::new(0) };
    }

    /// A leaf value that counts its drops: what the tests watch to see an
    /// object freed.
    struct Probe;

    impl Drop for Probe {
        fn drop(&mut self) {
            DROPPED.with(|dropped| dropped.set(dropped.get() + 1));
        }
    }

    impl Trace for Probe {
        fn trace(&self, _visitor: &mut dyn Visitor) {}
    }

    fn dropped() -> usize {
        DROPPED.with(StdCell::get)
    }

    struct Node {
        value: i64,
        next: Option<Cc<Node>>,
        _probe: Probe,
    }
    trace_fields!(Node, next);

    fn node(value: i64) -> Cc<Node> {
        cc(Node {
            value,
            next: None,
            _probe: Probe,
        })
    }

    /// Run `f`, collect, and return how many probes were dropped.
    fn freed_by(f: impl FnOnce()) -> usize {
        collect();
        let before = dropped();
        f();
        collect();
        dropped() - before
    }

    #[test]
    fn a_self_loop_is_collected() {
        let freed = freed_by(|| {
            let a = node(1);
            a.borrow_mut().next = Some(a.clone());
            assert_eq!(a.strong_count(), 2);
        });
        assert_eq!(freed, 1);
    }

    #[test]
    fn a_two_cycle_is_collected() {
        let freed = freed_by(|| {
            let a = node(1);
            let b = node(2);
            a.borrow_mut().next = Some(b.clone());
            b.borrow_mut().next = Some(a);
        });
        assert_eq!(freed, 2);
    }

    #[test]
    fn a_cycle_with_an_outside_handle_survives_until_it_goes() {
        collect();
        let before = dropped();
        let a = node(1);
        let b = node(2);
        a.borrow_mut().next = Some(b.clone());
        b.borrow_mut().next = Some(a.clone());
        // An untracked object holding a handle into the cycle: the collector
        // does not follow it, so `b` counts as referenced from outside.
        let outside: Shared<Vec<Cc<Node>>> = shared(vec![b.clone()]);
        drop((a, b));
        collect();
        assert_eq!(dropped(), before, "the outside handle keeps the cycle");
        let survivor = outside.borrow()[0].clone();
        let next = survivor.borrow().next.clone().expect("a");
        assert_eq!(next.borrow().value, 1);
        assert_eq!(survivor.borrow().value, 2);
        drop((survivor, next));
        collect();
        assert_eq!(dropped(), before, "still referenced");
        drop(outside);
        collect();
        assert_eq!(dropped() - before, 2, "freed once the outside handle goes");
    }

    #[test]
    fn a_cycle_broken_before_the_drop_is_freed_by_counting() {
        collect();
        let before = dropped();
        let a = node(1);
        let b = node(2);
        a.borrow_mut().next = Some(b.clone());
        b.borrow_mut().next = Some(a.clone());
        b.borrow_mut().next = None;
        drop(b);
        drop(a);
        assert_eq!(dropped() - before, 2, "no collection needed");
        collect();
        assert_eq!(dropped() - before, 2);
    }

    #[test]
    fn a_cycle_broken_after_buffering_is_collected_once() {
        let freed = freed_by(|| {
            let a = node(1);
            let b = node(2);
            a.borrow_mut().next = Some(b.clone());
            b.borrow_mut().next = Some(a.clone());
            // Buffer both as possible roots, then break the cycle: they free
            // by counting while still in the buffer, which the next
            // collection empties without touching their values.
            drop(a.clone());
            drop(b);
            a.borrow_mut().next = None;
        });
        assert_eq!(freed, 2);
    }

    #[test]
    fn a_long_chain_frees_without_recursing() {
        let length = if cfg!(miri) { 2_000 } else { 1_000_000 };
        // A thread with a small stack: a recursive free would overflow it.
        let freed = std::thread::Builder::new()
            .stack_size(256 << 10)
            .spawn(move || {
                freed_by(|| {
                    let mut head: Option<Cc<Node>> = None;
                    for value in 0..length {
                        head = Some(cc(Node {
                            value,
                            next: head.take(),
                            _probe: Probe,
                        }));
                    }
                    assert_eq!(
                        head.as_ref().map(|head| head.borrow().value),
                        Some(length - 1)
                    );
                })
            })
            .expect("thread")
            .join()
            .expect("no overflow");
        assert_eq!(freed, usize::try_from(length).unwrap());
    }

    #[test]
    fn a_long_cycle_is_collected_without_recursing() {
        let length = if cfg!(miri) { 500 } else { 200_000 };
        let freed = std::thread::Builder::new()
            .stack_size(256 << 10)
            .spawn(move || {
                freed_by(|| {
                    let tail = node(0);
                    let mut head = tail.clone();
                    for value in 1..length {
                        head = cc(Node {
                            value,
                            next: Some(head),
                            _probe: Probe,
                        });
                    }
                    tail.borrow_mut().next = Some(head);
                })
            })
            .expect("thread")
            .join()
            .expect("no overflow");
        assert_eq!(freed, usize::try_from(length).unwrap());
    }

    struct GNode {
        id: usize,
        edges: Cc<Vec<Cc<GNode>>>,
        _probe: Probe,
    }
    trace_fields!(GNode, edges);

    /// The `cycles` fixture's graph: a ring plus a chord per node.
    fn graph(n: usize) {
        let nodes: Vec<Cc<GNode>> = (0..n)
            .map(|id| {
                cc(GNode {
                    id,
                    edges: cc(Vec::new()),
                    _probe: Probe,
                })
            })
            .collect();
        for node in &nodes {
            let id = node.borrow().id;
            let edges = node.borrow().edges.clone();
            edges.borrow_mut().push(nodes[(id + 1) % n].clone());
            edges.borrow_mut().push(nodes[(id * 7 + 3) % n].clone());
        }
    }

    /// A garbage cycle with an edge to a live object: `mark_gray` takes the
    /// edge out of the live object's count, and freeing the cycle must not
    /// take it out a second time.
    #[test]
    fn a_garbage_cycle_does_not_take_a_live_objects_count_twice() {
        let keep = cc(GNode {
            id: 7,
            edges: cc(Vec::new()),
            _probe: Probe,
        });
        let freed = freed_by(|| {
            let a = cc(GNode {
                id: 1,
                edges: cc(Vec::new()),
                _probe: Probe,
            });
            let b = cc(GNode {
                id: 2,
                edges: cc(vec![a.clone(), keep.clone()]),
                _probe: Probe,
            });
            let edges = a.borrow().edges.clone();
            edges.borrow_mut().push(b);
        });
        assert_eq!(freed, 2, "the cycle, not `keep`");
        assert_eq!(keep.strong_count(), 1, "only `keep`'s own handle is left");
        assert_eq!(keep.borrow().id, 7);
    }

    #[test]
    fn graphs_are_collected() {
        let freed = freed_by(|| {
            for n in [1, 2, 8, 33] {
                graph(n);
            }
        });
        assert_eq!(freed, 1 + 2 + 8 + 33);
    }

    /// A closure's environment: one captured cell holding an array of
    /// closures, as `closure_in_owner` builds.
    struct Env {
        c0: CcCell<Cc<Vec<CcFunc<(), i64>>>>,
        _probe: Probe,
    }
    trace_fields!(Env, c0);

    impl Callable<(), i64> for Env {
        fn call(&self, (): ()) -> Result<i64, Thrown> {
            Ok(i64::try_from(cell::get(&self.c0)?.borrow().len()).unwrap_or(0))
        }
    }

    #[test]
    fn a_cycle_through_a_closure_environment_is_collected() {
        let freed = freed_by(|| {
            let cell: CcCell<Cc<Vec<CcFunc<(), i64>>>> = cell::fresh();
            let closures: Cc<Vec<CcFunc<(), i64>>> = cc(Vec::new());
            cell::set(&cell, closures.clone());
            let closure: CcFunc<(), i64> = func::alloc(Env {
                c0: cell,
                _probe: Probe,
            });
            closures.borrow_mut().push(closure.clone());
            closures.borrow_mut().push(closure.clone());
            assert_eq!(closure.call(()).unwrap(), 2);
        });
        assert_eq!(freed, 1, "the environment, with its cell and array");
    }

    /// An environment that captures nothing tracked.
    struct Leaf {
        _probe: Probe,
    }
    trace_fields!(Leaf);

    impl Callable<(), i64> for Leaf {
        fn call(&self, (): ()) -> Result<i64, Thrown> {
            Ok(7)
        }
    }

    #[test]
    fn an_acyclic_closure_is_never_buffered_and_dies_with_its_cycle() {
        collect();
        let start = stats();
        let freed = freed_by(|| {
            let leaf: CcFunc<(), i64> = func::alloc_acyclic(Leaf { _probe: Probe });
            let copy = leaf.clone();
            drop(copy);
            assert_eq!(stats().roots, start.roots, "not buffered");
            // Held only by a garbage cycle: white with it, freed with it.
            let cell: CcCell<Cc<Vec<CcFunc<(), i64>>>> = cell::fresh();
            let closures: Cc<Vec<CcFunc<(), i64>>> = cc(vec![leaf]);
            cell::set(&cell, closures.clone());
            let closure: CcFunc<(), i64> = func::alloc(Env {
                c0: cell,
                _probe: Probe,
            });
            closures.borrow_mut().push(closure);
            assert_eq!(closures.borrow()[0].call(()).unwrap(), 7);
        });
        assert_eq!(freed, 2, "the leaf closure and the environment");
        let item: CcFunc<(), i64> = func::item(|| Ok(1));
        drop(item.clone());
        assert_eq!(item.call(()).unwrap(), 1);
    }

    #[test]
    fn a_store_in_progress_is_read_as_no_edges() {
        let freed = freed_by(|| {
            let a = node(1);
            let b = node(2);
            a.borrow_mut().next = Some(b.clone());
            b.borrow_mut().next = Some(a.clone());
            drop(b);
            {
                // A collection while `a` is mutably borrowed (a store whose
                // old value's drop triggers one) cannot read `a`: the cycle
                // is kept, never freed under the borrow.
                let mut guard = a.borrow_mut();
                collect();
                guard.value = 3;
            }
            assert_eq!(a.borrow().value, 3);
        });
        assert_eq!(freed, 2, "collected once the borrow is gone");
    }

    /// A value whose drop runs the collector: frees must not let it in.
    struct Reenter {
        next: Option<Cc<Reenter>>,
        _probe: Probe,
    }
    trace_fields!(Reenter, next);

    impl Drop for Reenter {
        fn drop(&mut self) {
            collect();
        }
    }

    #[test]
    fn a_drop_that_collects_does_not_reenter() {
        let freed = freed_by(|| {
            let a = cc(Reenter {
                next: None,
                _probe: Probe,
            });
            let b = cc(Reenter {
                next: Some(a.clone()),
                _probe: Probe,
            });
            a.borrow_mut().next = Some(b);
            // And a chain freed by counting, whose drops also collect.
            let mut head = None;
            for _ in 0..10 {
                head = Some(cc(Reenter {
                    next: head.take(),
                    _probe: Probe,
                }));
            }
        });
        assert_eq!(freed, 12);
    }

    #[test]
    fn the_threshold_triggers_collections() {
        let start = stats();
        let threshold = threshold();
        set_threshold(4, false);
        collect();
        let before = dropped();
        for value in 0..8 {
            let a = node(value);
            a.borrow_mut().next = Some(a.clone());
            // Buffered when the outer handle goes.
        }
        assert!(stats().collections > start.collections);
        assert!(dropped() - before >= 4, "freed without an explicit collect");
        collect();
        assert_eq!(dropped() - before, 8);
        set_threshold(threshold, false);
        assert!(report().starts_with("baml-cc collections="));
    }

    fn buffered() -> usize {
        GC.with(|gc| gc.roots.borrow().len())
    }

    #[test]
    fn roots_that_die_buffered_leave_the_buffer() {
        if torture() {
            // Torture collects at every buffered decrement: nothing stays.
            return;
        }
        let threshold = threshold();
        set_threshold(1 << 30, false);
        collect();
        let start = stats();
        let count = if cfg!(miri) { 64 } else { 4096 };
        let freed = freed_by(|| {
            let mut kept: Vec<_> = (0..count)
                .map(|value| {
                    let a = node(i64::try_from(value).unwrap());
                    // Buffered by the copy's drop.
                    drop(a.clone());
                    a
                })
                .collect();
            assert_eq!(buffered(), count);
            // Free every third root, from the back and the middle, so
            // removals move the last root into freed slots.
            let mut index = kept.len();
            while index > 0 {
                index -= 1;
                if index % 3 == 0 {
                    drop(kept.swap_remove(index));
                }
            }
            assert_eq!(buffered(), kept.len());
            // The rest leave in an order unrelated to their slots.
            kept.reverse();
            while kept.len() > 1 {
                let middle = kept.len() / 2;
                drop(kept.swap_remove(middle));
                assert_eq!(buffered(), kept.len());
            }
            drop(kept);
            assert_eq!(buffered(), 0);
            assert_eq!(stats().collections, start.collections, "no collection");
        });
        assert_eq!(freed, count);
        assert_eq!(stats().roots, start.roots, "no root was left to collect");
        set_threshold(threshold, true);
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn the_header_is_as_big_as_an_rc_s_counts() {
        assert_eq!(size_of::<Header>(), 16);
    }

    #[test]
    fn untracked_handles_are_not_collected_or_buffered() {
        collect();
        let start = stats();
        let a = shared(vec![1_i64]);
        let b = a.clone();
        drop(b);
        collect();
        assert_eq!(stats().roots, start.roots, "nothing buffered");
        assert_eq!(*a.borrow(), vec![1]);
    }
}
