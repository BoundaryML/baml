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
//! descend, through [`HeapRef::trace_target`]. That serves both shapes of
//! collector:
//!
//! - trial deletion (Bacon-Rajan / Nim ORC): `trace_target` on a candidate,
//!   and for every child the visitor sees, decrement / restore / scan its
//!   count by [`HeapRef::addr`] (the object's header address);
//! - mark-sweep: `trace_target` on a root's object, mark every child the
//!   visitor sees by its address and push it for a later `trace_target`.
//!
//! Today handles are `Rc` and nothing calls a collector: [`collect`] is a
//! no-op, and `Trace` is only exercised by the tests and the `mem-stats`
//! unreachable count (`unreachable`, under that feature).

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

/// The contents of a handle's cell, read through a shared borrow.
impl<T: Trace> Trace for std::cell::RefCell<T> {
    #[inline]
    fn trace(&self, visitor: &mut dyn Visitor) {
        self.borrow().trace(visitor);
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

/// Run the collector. A no-op under reference counting; a prototype
/// collector fills it in. Generated code never calls it; hosts, tests and
/// the torture hook do.
#[inline]
pub fn collect() {}

#[cfg(feature = "mem-stats")]
pub(crate) use stats::{Erased, Registered, on_alloc, on_free};
#[cfg(feature = "mem-stats")]
pub use stats::{Stats, stats, torture, unreachable};

/// Allocation accounting. Every heap object (see the module docs) is counted
/// when allocated and when freed, per thread: a native program runs on one.
#[cfg(feature = "mem-stats")]
mod stats {
    use std::{
        cell::{Cell, RefCell},
        collections::{HashMap, HashSet},
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

    /// A live object as the registry keeps it: weakly, so the registry
    /// never keeps anything alive.
    pub(crate) type Registered = Weak<dyn Erased>;

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
        static REGISTRY: RefCell<Vec<Registered>> = const { RefCell::new(Vec::new()) };
    }

    /// The counters for this thread.
    pub fn stats() -> Stats {
        STATS.with(Cell::get)
    }

    /// Whether `BAML_MEM_TORTURE=1` was set when the first object was
    /// allocated: then every allocation runs the collector, right after the
    /// new object is created and before its handle is returned.
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
        REGISTRY.with(|registry| {
            let mut registry = registry.borrow_mut();
            // Forget freed objects once they outnumber the live ones.
            let live = usize::try_from(stats().live()).unwrap_or(usize::MAX);
            if registry.len() >= 2 * live.max(1024) {
                registry.retain(|entry| entry.strong_count() > 0);
            }
            registry.push(object);
        });
    }

    /// Count a free.
    #[inline(never)]
    pub(crate) fn on_free() {
        STATS.with(|stats| {
            let mut now = stats.get();
            now.frees += 1;
            stats.set(now);
        });
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
        let mut external: HashMap<*const (), usize> = objects
            .iter()
            .map(|object| {
                (
                    Rc::as_ptr(object).cast::<()>(),
                    Rc::strong_count(object) - 1,
                )
            })
            .collect();
        let mut children: HashMap<*const (), Vec<*const ()>> = HashMap::new();
        for object in &objects {
            let mut edges = Vec::new();
            object.trace_contents(&mut |child: &dyn HeapRef| edges.push(child.addr()));
            for child in &edges {
                if let Some(count) = external.get_mut(child) {
                    *count = count.saturating_sub(1);
                }
            }
            children.insert(Rc::as_ptr(object).cast::<()>(), edges);
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
        let total = objects.len();
        drop(objects);
        (total - live.len()) as u64
    }
}

#[cfg(all(test, feature = "mem-stats"))]
mod tests {
    use super::*;
    use crate::{
        cell,
        func::{self, Callable, Func},
        handle::{Shared, shared},
    };

    struct Node {
        next: Option<Shared<Node>>,
    }
    trace_fields!(Node, next);

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

    #[test]
    #[cfg_attr(miri, ignore = "leaks a cycle on purpose")]
    fn frees_are_counted_and_cycles_are_found() {
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
        let after = stats();
        assert_eq!(after.allocs - start.allocs, 2);
        assert_eq!(after.live() - start.live(), 2, "known leak under Rc");
        assert_eq!(unreachable() - start_garbage, 2);
        {
            let a = shared(Node { next: None });
            let _b = shared(Node { next: Some(a) });
        }
        assert_eq!(stats().live() - start.live(), 2, "an acyclic pair is freed");
    }

    #[test]
    #[cfg_attr(miri, ignore = "leaks a cycle on purpose")]
    fn a_closure_cycle_through_its_cell_is_found() {
        let (start, start_garbage) = (stats(), unreachable());
        {
            let cell = cell::fresh();
            let closures: Shared<Vec<Func<(), i64>>> = shared(Vec::new());
            cell::set(&cell, closures.clone());
            let closure = func::alloc(Env { c0: cell });
            closures.borrow_mut().push(closure.clone());
            assert_eq!(closure.call(()).unwrap(), 1);
            assert_eq!(unreachable(), start_garbage);
        }
        // The cell, the array and the closure.
        assert_eq!(stats().live() - start.live(), 3, "known leak under Rc");
        assert_eq!(unreachable() - start_garbage, 3);
    }

    #[test]
    fn collect_is_a_no_op_under_rc() {
        let start = stats();
        collect();
        assert_eq!(stats(), start);
        assert!(!torture() || std::env::var("BAML_MEM_TORTURE").as_deref() == Ok("1"));
    }
}
