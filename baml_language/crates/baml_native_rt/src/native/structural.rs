//! `==` on values no one instruction compares: `baml.ops.equals_equals`,
//! and the `eq` a class has when it writes none,
//! `baml._equals_structural_default`.
//!
//! Two values are equal when they are the same kind of thing and hold equal
//! things: strings by text, numbers by value with a NaN equal to itself,
//! arrays element by element, maps key by key in whatever order, instances
//! field by field. Two instances of a class with an `eq` of its own are
//! what that says, and are not looked inside.
//!
//! The walk allocates nothing and calls nothing. It takes each pair of
//! instances with an `eq` of their own to be equal and hands the pair out,
//! and the native calls the `eq` of every pair once the walk has found
//! nothing else unequal. An `eq` reads its arguments and answers, so
//! neither the order of the calls nor one that is not made shows.

use std::collections::HashSet;

use super::{
    BytesRef, Elem, NativeBamlOps, Natives, Scope, Thrown, TypeId, View, array_size,
    drive::BamlFn,
    json::{string_keyed, type_of, values_are_refs},
    root::float_order,
};
use crate::{
    describe::{Desc, Types},
    heap::load,
    json_layout::{
        KIND_ARRAY, KIND_FLOAT, KIND_MAP, REPR_BOOL, REPR_BOXED, REPR_FLOAT, REPR_NULL, REPR_PAIR,
        REPR_REF,
    },
    layout::{
        ARRAY_DATA, ARRAY_LENGTH, BOX_VALUE, HEADER_FLOATS, STRING_TYPES_END, TYPE_ARRAY_OF_REFS,
        TYPE_ARRAY_OF_VALUES, TYPE_BIGINT, TYPE_BOX_BOOL, TYPE_BOX_FLOAT, TYPE_BOX_INT, TYPE_BYTES,
        TYPE_FUTURE, TYPE_MAP, TYPE_OWNED, TYPE_PLAIN, WORD,
    },
    map::{self, KeyKind},
};

unsafe extern "C" {
    /// The classes with an `eq` of their own, which a program that calls
    /// either native lists: a count, and for each class its type id and
    /// the entry that calls its `eq`, sorted by type id.
    static baml_native_equals: u64;
}

/// The entry of the `eq` of the class whose instances have the type id
/// `type_id`, when the class has one of its own.
fn own_eq(type_id: usize) -> Option<BamlFn> {
    // SAFETY: the list is in the executable's data, laid out as above.
    unsafe {
        let list = &raw const baml_native_equals;
        let classes = std::slice::from_raw_parts(list.add(1).cast::<[u64; 2]>(), *list as usize);
        let found = classes
            .binary_search_by_key(&(type_id as u64), |class| class[0])
            .ok()?;
        BamlFn::at(classes[found][1] as usize)
    }
}

/// What a slot or a reference holds, as far as comparing it goes.
#[derive(Clone, Copy)]
enum Val {
    Null,
    Bool(bool),
    /// An `int`, or the position of an enum's variant.
    Word(usize),
    Float(f64),
    /// An object, and its static type when something says it.
    Ref(usize, Option<Desc>),
}

/// What the slot holding `word`, and `next` after it, holds: a value of the
/// type described.
unsafe fn of_slot(word: usize, next: usize, described: Desc) -> Val {
    let scalar = |repr: u32, bits: usize| match repr {
        REPR_FLOAT => Val::Float(f64::from_bits(bits as u64)),
        REPR_BOOL => Val::Bool(bits & 0xFF != 0),
        _ => Val::Word(bits),
    };
    match described.repr() {
        REPR_NULL => Val::Null,
        REPR_PAIR if next == 0 => Val::Null,
        REPR_PAIR => scalar(described.not_null().repr(), word),
        REPR_REF | REPR_BOXED => of_reference(word, Some(described.not_null())),
        repr => scalar(repr, word),
    }
}

/// What the reference `object` holds: a scalar out of its box, and
/// otherwise the object.
fn of_reference(object: usize, described: Option<Desc>) -> Val {
    if object == 0 {
        return Val::Null;
    }
    // SAFETY: a reference that is not null is to a live object.
    unsafe {
        match type_of(object) {
            TYPE_BOX_INT => Val::Word(load(object + BOX_VALUE)),
            TYPE_BOX_FLOAT => Val::Float(f64::from_bits(load(object + BOX_VALUE) as u64)),
            TYPE_BOX_BOOL => Val::Bool(load(object + BOX_VALUE) & 0xFF != 0),
            _ => Val::Ref(object, described),
        }
    }
}

/// A pair of words two containers hold: references, `float`s where
/// `floats` or `described` says so, or values whose words say no more.
fn held(
    left: usize,
    right: usize,
    references: bool,
    floats: bool,
    described: Option<Desc>,
) -> (Val, Val) {
    if references {
        (
            of_reference(left, described.map(Desc::not_null)),
            of_reference(right, described.map(Desc::not_null)),
        )
    } else if floats || described.is_some_and(|described| described.kind() == KIND_FLOAT) {
        (
            Val::Float(f64::from_bits(left as u64)),
            Val::Float(f64::from_bits(right as u64)),
        )
    } else {
        (Val::Word(left), Val::Word(right))
    }
}

struct Walk<'a, F> {
    types: &'a Types,
    /// The pairs still to compare.
    pending: Vec<(Val, Val)>,
    /// The pairs of objects looked inside so far: one met again is one the
    /// walk is inside, or has compared, and is taken to be equal.
    seen: HashSet<(usize, usize)>,
    /// Handed each pair of instances whose class has an `eq` of its own.
    own: F,
}

impl<F: FnMut(usize, usize)> Walk<'_, F> {
    /// Whether everything pending is equal, as far as the walk decides.
    unsafe fn run(&mut self) -> bool {
        while let Some(pair) = self.pending.pop() {
            let equal = match pair {
                (Val::Null, Val::Null) => true,
                (Val::Bool(a), Val::Bool(b)) => a == b,
                (Val::Word(a), Val::Word(b)) => a == b,
                (Val::Float(a), Val::Float(b)) => float_order(a, b).is_eq(),
                // SAFETY: both are live objects.
                (Val::Ref(a, described), Val::Ref(b, other)) => unsafe {
                    self.objects(a, b, described.or(other))
                },
                _ => false,
            };
            if !equal {
                return false;
            }
        }
        true
    }

    /// Whether the objects at `a` and `b` are equal, as far as can be said
    /// before what they hold is compared, which is left pending.
    unsafe fn objects(&mut self, a: usize, b: usize, described: Option<Desc>) -> bool {
        if a == b {
            return true;
        }
        // SAFETY: the caller's contract, for every object reached.
        unsafe {
            let (kind, other) = (type_of(a), type_of(b));
            if kind < STRING_TYPES_END && other < STRING_TYPES_END {
                return crate::string::text(a) == crate::string::text(b);
            }
            if kind != other {
                return false;
            }
            match kind {
                TYPE_BIGINT => return crate::bigint::value(a) == crate::bigint::value(b),
                // Both are scanned once and nothing runs meanwhile: a copy
                // of either would be made only to be compared.
                TYPE_BYTES => {
                    return BytesRef::from_raw(a).as_slice() == BytesRef::from_raw(b).as_slice();
                }
                // Each is what it is and nothing else.
                TYPE_FUTURE | TYPE_OWNED | TYPE_PLAIN => return false,
                _ => {}
            }
            if !self.seen.insert((a.min(b), a.max(b))) {
                return true;
            }
            match kind {
                TYPE_ARRAY_OF_REFS | TYPE_ARRAY_OF_VALUES => self.arrays(a, b, described),
                TYPE_MAP => self.maps(a, b, described),
                _ => self.instances(a, b, kind),
            }
        }
    }

    unsafe fn arrays(&mut self, a: usize, b: usize, described: Option<Desc>) -> bool {
        let element = described
            .filter(|described| described.kind() == KIND_ARRAY)
            .map(Desc::of_a);
        // SAFETY: two live arrays of one type.
        unsafe {
            let length = load(a + ARRAY_LENGTH);
            if length != load(b + ARRAY_LENGTH) {
                return false;
            }
            let references = type_of(a) == TYPE_ARRAY_OF_REFS;
            // An array of `float`s says so itself, for one nothing
            // describes.
            let floats = (load(a) | load(b)) & HEADER_FLOATS != 0;
            let (from, to) = (load(a + ARRAY_DATA), load(b + ARRAY_DATA));
            for index in (0..length).rev() {
                let (left, right) = (load(from + index * WORD), load(to + index * WORD));
                self.pending
                    .push(held(left, right, references, floats, element));
            }
        }
        true
    }

    unsafe fn maps(&mut self, a: usize, b: usize, described: Option<Desc>) -> bool {
        let described = described.filter(|described| described.kind() == KIND_MAP);
        // SAFETY: two live maps, which nothing changes during the walk.
        unsafe {
            if map::len(a) != map::len(b) {
                return false;
            }
            if map::len(a) == 0 {
                return true;
            }
            if string_keyed(a) != string_keyed(b) || values_are_refs(a) != values_are_refs(b) {
                return false;
            }
            let kind = if string_keyed(a) {
                KeyKind::Str
            } else if described.is_some_and(|map| map.of_a().kind() == KIND_FLOAT) {
                KeyKind::Float
            } else {
                KeyKind::Word
            };
            let references = values_are_refs(a);
            let floats = (load(a) | load(b)) & HEADER_FLOATS != 0;
            let values = described.map(Desc::of_b);
            let mut every_key = true;
            let mut pairs = Vec::new();
            map::for_each(a, |key, value| match map::get(b, kind, key) {
                // Where the value is, in a map nothing changes meanwhile.
                Some(other) => pairs.push((value, load(other))),
                None => every_key = false,
            });
            for (left, right) in pairs {
                self.pending
                    .push(held(left, right, references, floats, values));
            }
            every_key
        }
    }

    /// Two objects of one type that is none of the runtime's own: two
    /// instances of a class, two tuples, two variants of an enum.
    unsafe fn instances(&mut self, a: usize, b: usize, type_id: usize) -> bool {
        if own_eq(type_id).is_some() {
            (self.own)(a, b);
            return true;
        }
        // SAFETY: two live objects of the type.
        unsafe {
            if let Some(class) = self.types.of_class(type_id) {
                let fields: Vec<_> = (0..class.count())
                    .filter_map(|field| class.field_place(field))
                    .collect();
                for (offset, described) in fields.into_iter().rev() {
                    let next = |object: usize| {
                        if described.repr() == REPR_PAIR {
                            load(object + offset + WORD)
                        } else {
                            1
                        }
                    };
                    self.pending.push((
                        of_slot(load(a + offset), next(a), described),
                        of_slot(load(b + offset), next(b), described),
                    ));
                }
                return true;
            }
            // Nothing describes the fields: a word the collector follows
            // is a reference, and any other is compared as it is.
            let (size, references) = self.types.fixed(type_id);
            for offset in (WORD..size).step_by(WORD) {
                let (left, right) = (load(a + offset), load(b + offset));
                if references.contains(&(offset as u32)) {
                    self.pending
                        .push((of_reference(left, None), of_reference(right, None)));
                } else if left != right {
                    return false;
                }
            }
        }
        true
    }
}

/// Whether `a` and `b`, each a value as a union holds it, are equal as far
/// as the walk decides, with every pair it leaves to an `eq` handed to
/// `own`.
///
/// # Safety
///
/// Each is null or a live object, of the static type described when one
/// is, and nothing is allocated meanwhile.
unsafe fn walk(
    types: &Types,
    (a, a_type): (usize, Option<Desc>),
    (b, b_type): (usize, Option<Desc>),
    own: impl FnMut(usize, usize),
) -> bool {
    let mut walk = Walk {
        types,
        pending: vec![(of_reference(a, a_type), of_reference(b, b_type))],
        seen: HashSet::new(),
        own,
    };
    // SAFETY: the caller's contract.
    unsafe { walk.run() }
}

/// Whether `a` equals `b`.
pub(super) async fn equals<'s>(
    scope: &mut Scope<'s>,
    (a, a_type): (Elem<'s>, TypeId),
    (b, b_type): (Elem<'s>, TypeId),
) -> Result<bool, Thrown<'s>> {
    let types = Types::of(scope);
    let described = |ty: TypeId| types.described(ty).map(Desc::not_null);
    let (a_type, b_type) = (described(a_type), described(b_type));
    let mut count = 0;
    // SAFETY: the values compiled code passed, and nothing is allocated.
    let equal = unsafe {
        walk(
            &types,
            (a.get(scope), a_type),
            (b.get(scope), b_type),
            |_, _| count += 1,
        )
    };
    if !equal || count == 0 {
        return Ok(equal);
    }
    // The pairs left to an `eq`, held where the collector keeps them
    // current while each `eq` runs.
    let reservation = scope.reserve(array_size(2 * count));
    let array = reservation.slots(2 * count, true);
    let mut found = 0;
    // SAFETY: as above: nothing moves while the reservation is open, and
    // the walk meets the pairs it met before.
    unsafe {
        walk(
            &types,
            (a.get(reservation.scope), a_type),
            (b.get(reservation.scope), b_type),
            |left, right| {
                array.set(found, left);
                array.set(found + 1, right);
                found += 2;
            },
        );
    }
    let kept = array.keep();
    let array = reservation.finish(kept);
    for index in 0..count {
        let pair = array.get(scope);
        let (left, right) = (
            pair.get(2 * index).unwrap_or(0),
            pair.get(2 * index + 1).unwrap_or(0),
        );
        // SAFETY: a pair the walk handed out: two live instances of a
        // class that has an `eq`.
        let Some(eq) = own_eq(unsafe { type_of(left) }) else {
            continue;
        };
        // SAFETY: as above, and none of the three slots is used again.
        let equal = unsafe {
            let (left, right) = (
                scope.elem_raw(left, TypeId(0)),
                scope.elem_raw(right, TypeId(0)),
            );
            let answer = scope.call_baml(eq, &[left, right]).await?;
            let equal = answer.get(scope) != 0;
            scope.release_last(3);
            equal
        };
        if !equal {
            return Ok(false);
        }
    }
    Ok(true)
}

impl NativeBamlOps for Natives {
    async fn equals_equals<'s>(
        scope: &mut Scope<'s>,
        a_type: TypeId,
        a: Elem<'s>,
        b_type: TypeId,
        b: Elem<'s>,
    ) -> Result<bool, Thrown<'s>> {
        equals(scope, (a, a_type), (b, b_type)).await
    }
}
