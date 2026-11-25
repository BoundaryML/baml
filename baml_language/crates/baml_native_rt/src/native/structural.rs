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
    BytesRef, Elem, NativeBamlOps, Natives, Scope, Slot, Thrown, TypeRef, View, array_size,
    drive::BamlFn,
    json::{string_keyed, type_of},
    root::float_order,
};
use crate::{
    describe::{Desc, Types},
    heap::{Value, load},
    json_layout::{KIND_ARRAY, KIND_MAP},
    layout::{
        ARRAY_LENGTH, BOUND_CODE, BOUND_RECEIVER, BOUND_TYPE_ARG_COUNT, BOUND_TYPE_ARGS,
        CLASS_INDEX, CLOSURE_CAPTURE_COUNT, CLOSURE_CAPTURES, CLOSURE_CODE, INSTANCE_CLASS,
        INSTANCE_SLOTS, STRING_TAGS_END, TAG_ARRAY, TAG_BIGINT, TAG_BOOL, TAG_BOUND_METHOD,
        TAG_BYTES, TAG_CLOSURE, TAG_FLOAT, TAG_INSTANCE, TAG_INT, TAG_MAP, TAG_NULL, TAG_TUPLE,
        TUPLE_COUNT, VALUE, WORD,
    },
    map,
};

unsafe extern "C" {
    /// The classes with an `eq` of their own, which a program that calls
    /// either native lists: a count, and for each class the index its
    /// class object has and the entry that calls its `eq`, sorted by
    /// index.
    static baml_native_equals: u64;
}

/// The entry of the `eq` of the class of the instance at `instance`, when
/// the class has one of its own.
///
/// # Safety
///
/// `instance` is a live instance.
unsafe fn own_eq(instance: usize) -> Option<BamlFn> {
    // SAFETY: the caller's contract: an instance holds its class object,
    // which has its index. The list is in the executable's data, laid out
    // as above.
    unsafe {
        let index = ((load(instance + INSTANCE_CLASS) + CLASS_INDEX) as *const u32).read();
        let list = &raw const baml_native_equals;
        let classes = std::slice::from_raw_parts(list.add(1).cast::<[u64; 2]>(), *list as usize);
        let found = classes
            .binary_search_by_key(&u64::from(index), |class| class[0])
            .ok()?;
        BamlFn::at(classes[found][1] as usize)
    }
}

/// What a value is, as far as comparing it goes.
#[derive(Clone, Copy)]
enum Val {
    Null,
    Bool(bool),
    /// An `int`.
    Word(usize),
    Float(f64),
    /// An object, and its static type when something says it.
    Ref(usize, Option<Desc>),
}

/// What `value` is, which its tag says: a scalar, or an object.
/// `described` is its static type when something says it.
fn of_value(value: Value, described: Option<Desc>) -> Val {
    match value.id() {
        TAG_NULL => Val::Null,
        TAG_BOOL => Val::Bool(value.payload & 0xFF != 0),
        TAG_INT => Val::Word(value.payload),
        TAG_FLOAT => Val::Float(f64::from_bits(value.payload as u64)),
        _ => Val::Ref(value.payload, described.map(Desc::not_null)),
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
            if kind < STRING_TAGS_END && other < STRING_TAGS_END {
                return crate::string::text(a) == crate::string::text(b);
            }
            if kind != other {
                return false;
            }
            match kind {
                TAG_BIGINT => return crate::bigint::value(a) == crate::bigint::value(b),
                // Both are scanned once and nothing runs meanwhile: a copy
                // of either would be made only to be compared.
                TAG_BYTES => {
                    return BytesRef::from_raw(a).as_slice() == BytesRef::from_raw(b).as_slice();
                }
                _ => {}
            }
            if !self.seen.insert((a.min(b), a.max(b))) {
                return true;
            }
            match kind {
                TAG_ARRAY => self.arrays(a, b, described),
                TAG_MAP => self.maps(a, b, described),
                TAG_TUPLE => {
                    let count = load(a + TUPLE_COUNT);
                    count == load(b + TUPLE_COUNT) && self.slots(a, b, INSTANCE_SLOTS, count)
                }
                // Two functions are equal when they are the same code
                // over equal captures, or bound to equal receivers.
                TAG_CLOSURE => {
                    let count = load(a + CLOSURE_CAPTURE_COUNT);
                    load(a + CLOSURE_CODE) == load(b + CLOSURE_CODE)
                        && count == load(b + CLOSURE_CAPTURE_COUNT)
                        && self.slots(a, b, CLOSURE_CAPTURES, count)
                }
                TAG_BOUND_METHOD => {
                    let count = load(a + BOUND_TYPE_ARG_COUNT);
                    let type_arg =
                        |bound: usize, index: usize| load(bound + BOUND_TYPE_ARGS + index * WORD);
                    load(a + BOUND_CODE) == load(b + BOUND_CODE)
                        && count == load(b + BOUND_TYPE_ARG_COUNT)
                        && (0..count).all(|index| type_arg(a, index) == type_arg(b, index))
                        && self.slots(a, b, BOUND_RECEIVER, 1)
                }
                TAG_INSTANCE => {
                    load(a + INSTANCE_CLASS) == load(b + INSTANCE_CLASS) && self.instances(a, b)
                }
                // Any other object is what it is and nothing else: an
                // enum's variant is the one object.
                _ => false,
            }
        }
    }

    /// Leave the `count` slots from `first` on of the objects at `a` and
    /// `b`, which nothing describes, to be compared pair by pair.
    unsafe fn slots(&mut self, a: usize, b: usize, first: usize, count: usize) -> bool {
        for index in (0..count).rev() {
            let offset = first + index * VALUE;
            // SAFETY: both objects have that many slots there.
            let (left, right) = unsafe { (Value::load(a + offset), Value::load(b + offset)) };
            self.pending
                .push((of_value(left, None), of_value(right, None)));
        }
        true
    }

    unsafe fn arrays(&mut self, a: usize, b: usize, described: Option<Desc>) -> bool {
        let element = described
            .filter(|described| described.kind() == KIND_ARRAY)
            .map(Desc::of_a);
        // SAFETY: two live arrays.
        unsafe {
            let length = load(a + ARRAY_LENGTH);
            if length != load(b + ARRAY_LENGTH) {
                return false;
            }
            for index in (0..length).rev() {
                let (left, right) = (
                    Value::load(crate::array::element(a, index)),
                    Value::load(crate::array::element(b, index)),
                );
                self.pending
                    .push((of_value(left, element), of_value(right, element)));
            }
        }
        true
    }

    unsafe fn maps(&mut self, a: usize, b: usize, described: Option<Desc>) -> bool {
        let values = described
            .filter(|described| described.kind() == KIND_MAP)
            .map(Desc::of_b);
        // SAFETY: two live maps, which nothing changes during the walk.
        unsafe {
            if map::len(a) != map::len(b) {
                return false;
            }
            if map::len(a) == 0 {
                return true;
            }
            // A key of one is looked for in the other as a key of its
            // own kind.
            if string_keyed(a) != string_keyed(b) {
                return false;
            }
            let mut every_key = true;
            let mut pairs = Vec::new();
            map::for_each(a, |key, value| match map::get(b, key) {
                // Where the value is, in a map nothing changes meanwhile.
                Some(other) => pairs.push((value, Value::load(other))),
                None => every_key = false,
            });
            for (left, right) in pairs {
                self.pending
                    .push((of_value(left, values), of_value(right, values)));
            }
            every_key
        }
    }

    /// Two instances of one class.
    unsafe fn instances(&mut self, a: usize, b: usize) -> bool {
        // SAFETY: two live instances of the class.
        unsafe {
            if own_eq(a).is_some() {
                (self.own)(a, b);
                return true;
            }
            let class = load(a + INSTANCE_CLASS);
            if let Some(class) = self.types.of_class(class) {
                let fields: Vec<_> = (0..class.count())
                    .filter_map(|field| class.field_place(field))
                    .collect();
                for (offset, described) in fields.into_iter().rev() {
                    self.pending.push((
                        of_value(Value::load(a + offset), Some(described)),
                        of_value(Value::load(b + offset), Some(described)),
                    ));
                }
                return true;
            }
            // Nothing describes the fields: each slot says what it holds.
            self.slots(a, b, INSTANCE_SLOTS, crate::class::slot_count(class))
        }
    }
}

/// Whether the values `a` and `b` are equal as far as the walk decides,
/// with every pair it leaves to an `eq` handed to `own`.
///
/// # Safety
///
/// Each is a value as a slot holds one, of the static type described
/// when one is, and nothing is allocated meanwhile.
unsafe fn walk(
    types: &Types,
    (a, a_type): (Value, Option<Desc>),
    (b, b_type): (Value, Option<Desc>),
    own: impl FnMut(usize, usize),
) -> bool {
    let mut walk = Walk {
        types,
        pending: vec![(of_value(a, a_type), of_value(b, b_type))],
        seen: HashSet::new(),
        own,
    };
    // SAFETY: the caller's contract.
    unsafe { walk.run() }
}

/// Whether `a` equals `b`.
pub(super) async fn equals<'s>(
    scope: &mut Scope<'s>,
    (a, a_type): (Elem<'s>, TypeRef),
    (b, b_type): (Elem<'s>, TypeRef),
) -> Result<bool, Thrown<'s>> {
    let types = Types::of(scope);
    let described = |ty: TypeRef| types.described(ty).map(Desc::not_null);
    let (a_type, b_type) = (described(a_type), described(b_type));
    let mut count = 0;
    // SAFETY: the values compiled code passed, and nothing is allocated.
    let equal = unsafe {
        walk(
            &types,
            (a.value(scope), a_type),
            (b.value(scope), b_type),
            |_, _| count += 1,
        )
    };
    if !equal || count == 0 {
        return Ok(equal);
    }
    // The pairs left to an `eq`, held where the collector keeps them
    // current while each `eq` runs.
    let reservation = scope.reserve(array_size(2 * count));
    let array = reservation.array::<Slot>(2 * count);
    let mut found = 0;
    // SAFETY: as above: nothing moves while the reservation is open, the
    // walk meets the pairs it met before, and each is two live instances.
    unsafe {
        walk(
            &types,
            (a.value(reservation.scope), a_type),
            (b.value(reservation.scope), b_type),
            |left, right| {
                array.set(found, Value::reference(left));
                array.set(found + 1, Value::reference(right));
                found += 2;
            },
        );
    }
    let kept = array.keep();
    let array = reservation.finish(kept);
    for index in 0..count {
        let pair = array.get(scope);
        let (left, right) = (
            pair.get(2 * index).unwrap_or(Value::NULL),
            pair.get(2 * index + 1).unwrap_or(Value::NULL),
        );
        // A pair the walk handed out: two live instances of a class that
        // has an `eq`.
        // SAFETY: as the comment above says.
        let Some(eq) = (unsafe { own_eq(left.payload) }) else {
            continue;
        };
        // SAFETY: as above, and none of the three slots is used again.
        let equal = unsafe {
            let (left, right) = (scope.elem(left), scope.elem(right));
            let answer = scope.call_baml(eq, &[left, right]).await?;
            let equal = answer.value(scope).payload != 0;
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
        a_type: TypeRef,
        a: Elem<'s>,
        b_type: TypeRef,
        b: Elem<'s>,
    ) -> Result<bool, Thrown<'s>> {
        equals(scope, (a, a_type), (b, b_type)).await
    }
}
