//! What a memory manager needs to see of native values: the edges between
//! heap objects ([`Trace`]), and, behind the `mem-stats` feature, counters
//! and a torture switch.
//!
//! A heap object is what a handle points to: a class instance or array
//! ([`Shared`](crate::Shared)), a map ([`Map`](crate::Map)), a captured
//! local's cell ([`cell::Cell`](crate::cell::Cell)) or a function value
//! ([`func::Func`](crate::func::Func)). Strings and bigints are counted
//! pointers too, but they hold no handles, so they can never be part of a
//! cycle and are leaves here.
//!
//! [`Trace`] is implemented by every value type a handle can hold, here for
//! the runtime's own types and by the backend for every generated class,
//! union, interface and closure environment. `trace` visits each handle the
//! value owns directly, without following it: a collector decides whether to
//! descend, through [`HeapRef::trace_target`] (or, in [`crate::cc`], the
//! object's type-erased ops table).
//!
//! Its reader is the cycle collector in [`crate::cc`]: trial deletion
//! subtracts one count from a child for every edge `trace` reports, so the
//! impls must report each handle a value owns **exactly once**. A missed
//! edge makes its target look referenced from outside, so a cycle through
//! it is never freed (a leak, which the `cycle_leaks` test and
//! `unreachable` catch); an edge reported twice would take a live object's
//! count to zero and free it while it is still reachable. The backend
//! generates one `trace` call per field, element or payload, and this
//! module's container impls visit each element once.
//!
//! [`collect`] runs the collector; without `mem-stats` nothing else here
//! has a runtime cost.

use crate::{BigInt, Int63, Panic, Str, Thrown};

/// A value whose directly owned handles a collector can enumerate.
///
/// `trace` calls `visitor.visit` once for every handle stored in `self`
/// (a field, an element, a variant's payload), and never follows one: what
/// lies behind a handle is that object's own `trace`. A value with no
/// handles visits nothing. Values are `'static`: a collected object cannot
/// borrow.
pub trait Trace: 'static {
    fn trace(&self, visitor: &mut dyn Visitor);
}

/// What a collector does with each edge a [`Trace`] impl reports.
pub trait Visitor {
    fn visit(&mut self, object: &dyn HeapRef);

    /// The edge of a tracked handle, with the cycle collector's view of its
    /// object already at hand, so the collector skips a dynamic call per
    /// edge. Every other visitor sees it as [`Visitor::visit`].
    #[doc(hidden)]
    #[inline]
    fn visit_tracked(&mut self, object: crate::cc::Object, handle: &dyn HeapRef) {
        let _ = object;
        self.visit(handle);
    }
}

impl<F: FnMut(&dyn HeapRef)> Visitor for F {
    #[inline]
    fn visit(&mut self, object: &dyn HeapRef) {
        self(object);
    }
}

/// A handle, erased of what it points to: the view a collector has of one
/// edge.
pub trait HeapRef {
    /// The object's address: its identity, the same for every handle to it
    /// (and, in a collector with a header, the header's address).
    fn addr(&self) -> *const ();

    /// Visit the object's own children: its contents' [`Trace`]. Reads the
    /// object through a shared borrow, which is free whenever no Rust code
    /// is mid-way through a store into it (generated code never holds a
    /// borrow across a call or an allocation).
    fn trace_target(&self, visitor: &mut dyn Visitor);

    /// The number of handles to the object. Under `Rc` this is the strong
    /// count; a collector that keeps its own count reports that.
    fn strong_count(&self) -> usize;

    /// The cycle collector's view of the object, for a tracked handle
    /// ([`crate::cc::Tracked`]); `None` for an untracked one, which the
    /// collector does not follow.
    #[doc(hidden)]
    #[inline]
    fn object(&self) -> Option<crate::cc::Object> {
        None
    }
}

/// A no-op `Trace` for the values that hold no handle.
macro_rules! leaf {
    ($($ty:ty),* $(,)?) => {$(
        impl Trace for $ty {
            #[inline(always)]
            fn trace(&self, _visitor: &mut dyn Visitor) {}
        }
    )*};
}

leaf!(
    Int63,
    i64,
    bool,
    f64,
    Str,
    BigInt,
    (),
    Panic,
    String,
    &'static str
);

impl<T: Trace> Trace for Option<T> {
    #[inline]
    fn trace(&self, visitor: &mut dyn Visitor) {
        if let Some(value) = self {
            value.trace(visitor);
        }
    }
}

impl<T: Trace> Trace for Vec<T> {
    #[inline]
    fn trace(&self, visitor: &mut dyn Visitor) {
        for value in self {
            value.trace(visitor);
        }
    }
}

/// The contents of a handle's cell, read through a shared borrow. A cell
/// that is mutably borrowed (a store in progress when a collection runs)
/// reports no edges: the collector then keeps everything it might reach.
impl<T: Trace> Trace for std::cell::RefCell<T> {
    #[inline]
    fn trace(&self, visitor: &mut dyn Visitor) {
        if let Ok(value) = self.try_borrow() {
            value.trace(visitor);
        }
    }
}

impl<T: Trace + ?Sized> Trace for Box<T> {
    #[inline]
    fn trace(&self, visitor: &mut dyn Visitor) {
        (**self).trace(visitor);
    }
}

impl<K: Trace, V: Trace, S: 'static> Trace for indexmap::IndexMap<K, V, S> {
    #[inline]
    fn trace(&self, visitor: &mut dyn Visitor) {
        for (key, value) in self {
            key.trace(visitor);
            value.trace(visitor);
        }
    }
}

macro_rules! tuple {
    ($($name:ident),+) => {
        impl<$($name: Trace),+> Trace for ($($name,)+) {
            #[inline]
            #[allow(non_snake_case)]
            fn trace(&self, visitor: &mut dyn Visitor) {
                let ($($name,)+) = self;
                $($name.trace(visitor);)+
            }
        }
    };
}

tuple!(A);
tuple!(A, B);
tuple!(A, B, C);
tuple!(A, B, C, D);

/// `Trace` for a test struct, field by field: what the backend generates
/// for a class.
#[cfg(test)]
macro_rules! trace_fields {
    ($ty:ty $(, $field:ident)*) => {
        impl $crate::mem::Trace for $ty {
            fn trace(&self, _visitor: &mut dyn $crate::mem::Visitor) {
                $($crate::mem::Trace::trace(&self.$field, _visitor);)*
            }
        }
    };
}
#[cfg(test)]
pub(crate) use trace_fields;

/// A thrown value: a panic holds no handle; a thrown class instance is the
/// handle it was thrown as.
impl Trace for Thrown {
    #[inline]
    fn trace(&self, visitor: &mut dyn Visitor) {
        match self {
            Self::Panic(_) => {}
            Self::Error(error) => error.as_ref().trace(visitor),
        }
    }
}

/// Run the cycle collector over the possible roots buffered so far
/// ([`crate::cc::collect`]). Untracked objects are freed by their counts
/// alone. Generated code never calls it; hosts, tests and the torture hook
/// do, and the collector runs itself when its buffer fills.
#[inline]
pub fn collect() {
    crate::cc::collect();
}

#[cfg(feature = "mem-stats")]
pub(crate) use stats::{Erased, Registered, on_alloc, on_cc_dealloc, on_free};
#[cfg(feature = "mem-stats")]
pub use stats::{Stats, stats, torture, unreachable};

/// Allocation accounting. Every heap object (see the module docs) is counted
/// when allocated and when freed, per thread: a native program runs on one.
#[cfg(feature = "mem-stats")]
mod stats {
    use std::{
        cell::{Cell, RefCell},
        collections::{HashMap, HashSet},
        ptr::NonNull,
        rc::{Rc, Weak},
        sync::OnceLock,
    };

    use super::{HeapRef, Visitor};

    /// Counters since the thread started.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct Stats {
        /// Heap objects allocated.
        pub allocs: u64,
        /// Heap objects freed.
        pub frees: u64,
        /// Calls to [`super::collect`] made by the torture hook.
        pub torture_collections: u64,
    }

    impl Stats {
        /// Objects allocated and not yet freed.
        pub fn live(&self) -> u64 {
            self.allocs - self.frees
        }
    }

    /// A live object as the registry keeps it, never keeping it alive: a
    /// weak `Rc` for an untracked object, the header of a tracked one
    /// (taken out of the registry when its box is deallocated).
    pub(crate) enum Registered {
        Rc(Weak<dyn Erased>),
        Cc(NonNull<crate::cc::Header>),
    }

    /// A heap object behind an erased pointer: what the registry can ask of
    /// it.
    pub(crate) trait Erased {
        /// Visit the handles the object's contents hold.
        fn trace_contents(&self, visitor: &mut dyn Visitor);
    }

    thread_local! {
        static STATS: Cell<Stats> = const {
            Cell::new(Stats { allocs: 0, frees: 0, torture_collections: 0 })
        };
        static REGISTRY: RefCell<Vec<Weak<dyn Erased>>> = const { RefCell::new(Vec::new()) };
        static TRACKED: RefCell<HashSet<NonNull<crate::cc::Header>>> =
            RefCell::new(HashSet::new());
    }

    /// The counters for this thread.
    pub fn stats() -> Stats {
        STATS.with(Cell::get)
    }

    /// Whether `BAML_MEM_TORTURE=1` was set when the first object was
    /// allocated: then every allocation runs the collector, right after the
    /// new object is created and before its handle is returned, and so does
    /// every decrement that buffers a possible root.
    pub fn torture() -> bool {
        static TORTURE: OnceLock<bool> = OnceLock::new();
        *TORTURE.get_or_init(|| std::env::var_os("BAML_MEM_TORTURE").is_some_and(|v| v == "1"))
    }

    /// Count an allocation: the torture hook runs first (the new object is
    /// already allocated, held only by the allocating function), then the
    /// object is registered.
    #[inline(never)]
    pub(crate) fn on_alloc(object: Registered) {
        if torture() {
            super::collect();
            STATS.with(|stats| {
                let mut now = stats.get();
                now.torture_collections += 1;
                stats.set(now);
            });
        }
        STATS.with(|stats| {
            let mut now = stats.get();
            now.allocs += 1;
            stats.set(now);
        });
        match object {
            Registered::Rc(weak) => REGISTRY.with(|registry| {
                let mut registry = registry.borrow_mut();
                // Forget freed objects once they outnumber the live ones.
                let live = usize::try_from(stats().live()).unwrap_or(usize::MAX);
                if registry.len() >= 2 * live.max(1024) {
                    registry.retain(|entry| entry.strong_count() > 0);
                }
                registry.push(weak);
            }),
            Registered::Cc(header) => {
                let _ = TRACKED.try_with(|tracked| tracked.borrow_mut().insert(header));
            }
        }
    }

    /// Count a free: an object's value dropped.
    #[inline(never)]
    pub(crate) fn on_free() {
        let _ = STATS.try_with(|stats| {
            let mut now = stats.get();
            now.frees += 1;
            stats.set(now);
        });
    }

    /// Forget a tracked object whose box is deallocated.
    pub(crate) fn on_cc_dealloc(header: NonNull<crate::cc::Header>) {
        let _ = TRACKED.try_with(|tracked| tracked.borrow_mut().remove(&header));
    }

    /// How many live objects no handle outside the heap reaches: the
    /// garbage a cycle collector would free. Computed by trial deletion
    /// over every live object, with each object's strong count and the
    /// edges [`super::Trace`] reports, so it is also the check that the
    /// `Trace` impls are complete: a missed edge makes its target look
    /// referenced from outside, and the count drops below the leak.
    pub fn unreachable() -> u64 {
        let objects: Vec<_> =
            REGISTRY.with(|registry| registry.borrow().iter().filter_map(Weak::upgrade).collect());
        // Every object starts with its strong count (less the one the
        // upgrade above added) as references from outside; each edge from
        // another object takes one away.
        let mut external: HashMap<*const (), usize> = HashMap::new();
        let mut children: HashMap<*const (), Vec<*const ()>> = HashMap::new();
        for object in &objects {
            let mut edges = Vec::new();
            object.trace_contents(&mut |child: &dyn HeapRef| edges.push(child.addr()));
            let addr = Rc::as_ptr(object).cast::<()>();
            external.insert(addr, Rc::strong_count(object) - 1);
            children.insert(addr, edges);
        }
        let tracked: Vec<_> = TRACKED.with(|tracked| tracked.borrow().iter().copied().collect());
        for header in tracked {
            let mut edges = Vec::new();
            // SAFETY: the registry holds only allocated boxes.
            #[allow(unsafe_code)]
            let count = unsafe {
                crate::cc::inspect(header, &mut |child: &dyn HeapRef| edges.push(child.addr()))
            };
            if let Some(count) = count {
                let addr = header.as_ptr().cast_const().cast::<()>();
                external.insert(addr, count);
                children.insert(addr, edges);
            }
        }
        for edges in children.values() {
            for child in edges {
                if let Some(count) = external.get_mut(child) {
                    *count = count.saturating_sub(1);
                }
            }
        }
        // Everything reachable from an object with an outside handle lives.
        let mut live: HashSet<*const ()> = HashSet::new();
        let mut stack: Vec<*const ()> = external
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(addr, _)| *addr)
            .collect();
        while let Some(addr) = stack.pop() {
            if live.insert(addr)
                && let Some(edges) = children.get(&addr)
            {
                stack.extend(edges.iter().copied());
            }
        }
        let total = external.len();
        drop(objects);
        (total - live.len()) as u64
    }
}

#[cfg(all(test, feature = "mem-stats"))]
mod tests {
    use super::*;
    use crate::{
        cell,
        func::{self, Callable, CcFunc, Func},
        handle::{Cc, Shared, cc, shared},
    };

    struct Node {
        next: Option<Shared<Node>>,
    }
    trace_fields!(Node, next);

    struct CcNode {
        next: Option<Cc<CcNode>>,
    }
    trace_fields!(CcNode, next);

    /// A closure over one cell holding an array of closures.
    struct Env {
        c0: cell::Cell<Shared<Vec<Func<(), i64>>>>,
    }
    trace_fields!(Env, c0);

    impl Callable<(), i64> for Env {
        fn call(&self, (): ()) -> Result<i64, Thrown> {
            Ok(i64::try_from(cell::get(&self.c0)?.borrow().len()).unwrap_or(0))
        }
    }

    /// [`Env`] at the tracked kind, as the backend emits it for a function
    /// type whose closures can reach themselves.
    struct CcEnv {
        c0: cell::CcCell<Cc<Vec<CcFunc<(), i64>>>>,
    }
    trace_fields!(CcEnv, c0);

    impl Callable<(), i64> for CcEnv {
        fn call(&self, (): ()) -> Result<i64, Thrown> {
            Ok(i64::try_from(cell::get(&self.c0)?.borrow().len()).unwrap_or(0))
        }
    }

    /// An untracked cycle is found by `unreachable` and never freed: what
    /// the backend's type analysis prevents by tracking every type that can
    /// be on a cycle.
    #[test]
    #[cfg_attr(miri, ignore = "leaks a cycle on purpose")]
    fn frees_are_counted_and_untracked_cycles_are_found() {
        let (start, start_garbage) = (stats(), unreachable());
        {
            let a = shared(Node { next: None });
            let b = shared(Node {
                next: Some(a.clone()),
            });
            assert_eq!(stats().live() - start.live(), 2);
            assert_eq!(unreachable(), start_garbage, "both are held from outside");
            a.borrow_mut().next = Some(b);
            assert_eq!(unreachable(), start_garbage, "`a` still is");
        }
        collect();
        let after = stats();
        assert_eq!(after.allocs - start.allocs, 2);
        assert_eq!(after.live() - start.live(), 2, "an untracked cycle leaks");
        assert_eq!(unreachable() - start_garbage, 2);
        {
            let a = shared(Node { next: None });
            let _b = shared(Node { next: Some(a) });
        }
        assert_eq!(stats().live() - start.live(), 2, "an acyclic pair is freed");
    }

    #[test]
    fn tracked_cycles_are_counted_found_and_collected() {
        collect();
        let (start, start_garbage) = (stats(), unreachable());
        {
            let a = cc(CcNode { next: None });
            let b = cc(CcNode {
                next: Some(a.clone()),
            });
            assert_eq!(stats().live() - start.live(), 2);
            assert_eq!(unreachable(), start_garbage, "both are held from outside");
            a.borrow_mut().next = Some(b);
        }
        if !torture() {
            // (Torture collects at every buffered decrement.)
            assert_eq!(stats().live() - start.live(), 2, "kept until a collection");
            assert_eq!(unreachable() - start_garbage, 2, "found through `Trace`");
        }
        collect();
        assert_eq!(stats().live(), start.live(), "collected");
        assert_eq!(unreachable(), start_garbage);
    }

    #[test]
    #[cfg_attr(miri, ignore = "leaks a cycle on purpose")]
    fn an_untracked_closure_cycle_through_its_cell_is_found() {
        let (start, start_garbage) = (stats(), unreachable());
        {
            let cell = cell::fresh();
            let closures: Shared<Vec<Func<(), i64>>> = shared(Vec::new());
            cell::set(&cell, closures.clone());
            let closure: Func<(), i64> = func::alloc(Env { c0: cell });
            closures.borrow_mut().push(closure.clone());
            assert_eq!(closure.call(()).unwrap(), 1);
            assert_eq!(unreachable(), start_garbage);
        }
        // The cell, the array and the closure.
        assert_eq!(stats().live() - start.live(), 3, "an untracked cycle leaks");
        assert_eq!(unreachable() - start_garbage, 3);
    }

    #[test]
    fn a_tracked_closure_cycle_through_its_cell_is_collected() {
        collect();
        let (start, start_garbage) = (stats(), unreachable());
        {
            let cell = cell::fresh();
            let closures: Cc<Vec<CcFunc<(), i64>>> = cc(Vec::new());
            cell::set(&cell, closures.clone());
            let closure: CcFunc<(), i64> = func::alloc(CcEnv { c0: cell });
            closures.borrow_mut().push(closure.clone());
            assert_eq!(closure.call(()).unwrap(), 1);
        }
        if !torture() {
            assert_eq!(
                unreachable() - start_garbage,
                3,
                "the cell, array and closure"
            );
        }
        collect();
        assert_eq!(stats().live(), start.live());
        assert_eq!(unreachable(), start_garbage);
    }

    #[test]
    fn collect_leaves_untracked_objects_alone() {
        let a = shared(vec![1_i64]);
        let start = stats();
        collect();
        assert_eq!(stats(), start);
        assert_eq!(*a.borrow(), vec![1]);
        assert!(!torture() || std::env::var("BAML_MEM_TORTURE").as_deref() == Ok("1"));
    }
}
