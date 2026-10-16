//! Function values: `(A, B) -> R throws E`.
//!
//! A function value is a handle, [`Func`], to a heap object that a memory
//! manager can see into: the environment of a closure (the generated struct
//! `<lambda>__env`, one field per captured cell) or a declared function used
//! as a value ([`Item`], nothing captured). The object implements
//! [`Callable`] for the function type's parameters, as a tuple, and its
//! result, and [`Trace`] for what it captures; the handle is shared by
//! reference like [`Shared`](crate::Shared), and a call goes through one
//! dynamic dispatch, as the `Rc<dyn Fn>` it replaces did.
//!
//! Generated code builds a closure with [`alloc`] and a declared function
//! value with [`item`], and calls either with [`Func::call`]; nothing else
//! about the representation is public, so it can change here alone.

use std::fmt;

use crate::{
    Thrown,
    cc::{Object, Tracked},
    handle::{Kind, Untracked},
    mem::{HeapRef, Trace, Visitor},
};

/// A function value's object: called with the arguments as a tuple `A`.
pub trait Callable<A, R>: Trace {
    fn call(&self, args: A) -> Result<R, Thrown>;
}

/// A BAML function value of parameter tuple `A` and result `R`: a counted
/// handle to a [`Callable`]. `Clone` copies the handle. `H` is how it
/// counts ([`Kind`]); a function type whose closures can reach a value of
/// the type itself is [`CcFunc`].
pub struct Func<A, R, H: Kind = Untracked>(H::Ptr<dyn Callable<A, R>>);

/// A function value the cycle collector examines.
pub type CcFunc<A, R> = Func<A, R, Tracked>;

impl<A: 'static, R: 'static, H: Kind> Func<A, R, H> {
    /// Call the function.
    #[inline]
    pub fn call(&self, args: A) -> Result<R, Thrown> {
        H::get(&self.0).call(args)
    }
}

/// Allocate a function value over `object`, a closure environment, of the
/// kind the context asks for.
#[inline]
pub fn alloc<A: 'static, R: 'static, C: Callable<A, R>, H: Kind>(object: C) -> Func<A, R, H> {
    Func(H::alloc_callable(object, false))
}

/// [`alloc`] for an environment that holds no tracked handle (every cell
/// it captures is untracked): it can never be on a cycle, so even under a
/// tracked function type the collector never buffers it.
#[inline]
pub fn alloc_acyclic<A: 'static, R: 'static, C: Callable<A, R>, H: Kind>(
    object: C,
) -> Func<A, R, H> {
    Func(H::alloc_callable(object, true))
}

/// Allocate a function value calling the declared function `function`: it
/// captures nothing, so it is acyclic.
#[inline]
pub fn item<A: 'static, R: 'static, F: 'static, H: Kind>(function: F) -> Func<A, R, H>
where
    Item<F>: Callable<A, R>,
{
    alloc_acyclic(Item(function))
}

impl<A, R, H: Kind> Clone for Func<A, R, H> {
    #[inline]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<A, R, H: Kind> fmt::Debug for Func<A, R, H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<function>")
    }
}

/// A function value is one edge, as any handle is.
impl<A: 'static, R: 'static, H: Kind> Trace for Func<A, R, H> {
    #[inline]
    fn trace(&self, visitor: &mut dyn Visitor) {
        match H::object(&self.0) {
            Some(object) => visitor.visit_tracked(object, self),
            None => visitor.visit(self),
        }
    }
}

impl<A: 'static, R: 'static, H: Kind> HeapRef for Func<A, R, H> {
    #[inline]
    fn addr(&self) -> *const () {
        H::addr(&self.0)
    }

    fn trace_target(&self, visitor: &mut dyn Visitor) {
        H::get(&self.0).trace(visitor);
    }

    fn strong_count(&self) -> usize {
        H::strong_count(&self.0)
    }

    #[inline]
    fn object(&self) -> Option<Object> {
        H::object(&self.0)
    }
}

/// A declared function as a function value: it captures nothing.
#[derive(Clone, Copy)]
pub struct Item<F>(pub F);

impl<F: 'static> Trace for Item<F> {
    #[inline(always)]
    fn trace(&self, _visitor: &mut dyn Visitor) {}
}

macro_rules! item_arity {
    ($($arg:ident),*) => {
        impl<F, R, $($arg),*> Callable<($($arg,)*), R> for Item<F>
        where
            F: Fn($($arg),*) -> Result<R, Thrown> + 'static,
        {
            #[inline]
            #[allow(non_snake_case)]
            fn call(&self, ($($arg,)*): ($($arg,)*)) -> Result<R, Thrown> {
                (self.0)($($arg),*)
            }
        }
    };
}

item_arity!();
item_arity!(A0);
item_arity!(A0, A1);
item_arity!(A0, A1, A2);
item_arity!(A0, A1, A2, A3);
item_arity!(A0, A1, A2, A3, A4);
item_arity!(A0, A1, A2, A3, A4, A5);
item_arity!(A0, A1, A2, A3, A4, A5, A6);
item_arity!(A0, A1, A2, A3, A4, A5, A6, A7);
item_arity!(A0, A1, A2, A3, A4, A5, A6, A7, A8);
item_arity!(A0, A1, A2, A3, A4, A5, A6, A7, A8, A9);
item_arity!(A0, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10);
item_arity!(A0, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Int63, cell,
        handle::{Shared, shared},
        int,
    };

    fn n(v: i64) -> Int63 {
        Int63::new(v).unwrap()
    }

    fn add(a: Int63, b: Int63) -> Result<Int63, Thrown> {
        int::add(a, b).map_err(Thrown::from)
    }

    /// What the backend generates for `let k = ..; (x: int) -> int { x + k }`.
    struct AddK {
        c0: cell::Cell<Int63>,
    }

    impl Trace for AddK {
        fn trace(&self, visitor: &mut dyn Visitor) {
            self.c0.trace(visitor);
        }
    }

    impl Callable<(Int63,), Int63> for AddK {
        fn call(&self, (x,): (Int63,)) -> Result<Int63, Thrown> {
            add(x, cell::get(&self.c0)?)
        }
    }

    #[test]
    fn closures_and_items_call_through_one_handle_type() {
        let k = cell::with(n(10));
        let closure: Func<(Int63,), Int63> = alloc(AddK { c0: k.clone() });
        assert_eq!(closure.call((n(1),)).unwrap(), n(11));
        cell::set(&k, n(20));
        let copy = closure.clone();
        assert_eq!(copy.call((n(1),)).unwrap(), n(21), "the cell is shared");
        assert_eq!(copy.addr(), closure.addr(), "a copy is the same object");
        let item: Func<(Int63, Int63), Int63> = item(add);
        assert_eq!(item.call((n(2), n(3))).unwrap(), n(5));
        let nullary: Func<(), Int63> = super::item(|| Ok(n(7)));
        assert_eq!(nullary.call(()).unwrap(), n(7));
    }

    #[test]
    fn a_closure_traces_its_cells() {
        let k = cell::with(n(1));
        let closure: Func<(Int63,), Int63> = alloc(AddK { c0: k });
        let mut edges = 0;
        closure.trace(&mut |_: &dyn HeapRef| edges += 1);
        assert_eq!(edges, 1, "the function value itself");
        let mut cells = 0;
        closure.trace_target(&mut |_: &dyn HeapRef| cells += 1);
        assert_eq!(cells, 1, "its one captured cell");
        let functions: Shared<Vec<Func<(Int63,), Int63>>> = shared(vec![closure.clone()]);
        let mut seen = Vec::new();
        functions.trace_target(&mut |object: &dyn HeapRef| seen.push(object.addr()));
        assert_eq!(seen, vec![closure.addr()]);
    }
}
