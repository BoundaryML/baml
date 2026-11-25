//! Class instances as the runtime sees them: how one is written when it is
//! thrown and nothing catches it, and how the runtime makes one of the
//! standard library's panics and errors.
//!
//! Both read the description the compiler leaves with a class's type; see
//! [`crate::layout::TYPE_HEADER`].

use std::fmt::Write as _;

use crate::{
    array,
    describe::{Desc, Types},
    float_text,
    heap::{Mutator, Value, load, store},
    json_layout::{
        KIND_ARRAY, KIND_ENUM, KIND_MAP, KIND_OPAQUE, KIND_VARIANT, REPR_BOOL, REPR_FLOAT, REPR_INT,
    },
    layout::{
        ARRAY_LENGTH, BOX_SIZE, BOX_VALUE, HEADER, HEADER_PANIC, STRING_TAGS_END, TAG_ARRAY,
        TAG_BOOL, TAG_BOX_BOOL, TAG_BOX_FLOAT, TAG_BOX_INT, TAG_FLOAT, TAG_INT, TAG_MAP, TAG_NULL,
        TYPE_ID_MASK, WELL_KNOWN_CONTEXT, WELL_KNOWN_EXIT, WELL_KNOWN_PANICS,
        WELL_KNOWN_STACK_FRAME, WELL_KNOWN_STACK_TRACE,
    },
    map, string,
};

/// Read the 32-bit word at `address`.
///
/// # Safety
///
/// `address` is inside the type table.
unsafe fn word(address: usize) -> usize {
    // SAFETY: the caller's contract.
    unsafe { (address as *const u32).read_unaligned() as usize }
}

/// The description of the class whose instances have the type `type_id`.
///
/// # Safety
///
/// `type_id` is an entry of the mutator's type table.
unsafe fn description(mutator: &Mutator, type_id: usize) -> Option<Desc> {
    Types::of_table(mutator.types()).of_class(type_id)
}

/// The byte offset in an instance of field `index` of the class
/// `description` is of.
fn field_at(description: Desc, index: usize) -> usize {
    description
        .fields()
        .nth(index)
        .map(|field| field.offset)
        .expect("the standard library declares the field")
}

/// The value in slot `index` of the instance at `object` of the class
/// `description` is of.
///
/// # Safety
///
/// `object` is a live instance of that class.
unsafe fn field(object: usize, description: Desc, index: usize) -> Value {
    // SAFETY: the caller's contract: the description says where the field
    // is.
    unsafe { Value::load(object + field_at(description, index)) }
}

/// Give field `index` of the instance at `object` of the class
/// `description` is of the value `value`.
///
/// # Safety
///
/// `object` is a live instance of that class, and `value` a value of the
/// field's type in the form its slot holds.
unsafe fn set_field(object: usize, description: Desc, index: usize, value: Value) {
    // SAFETY: the caller's contract.
    unsafe { Value::store(object + field_at(description, index), value) }
}

/// Lay out an instance of `size` bytes whose header is `header` at
/// `object`: its word for its class zero, and every slot `null`.
///
/// # Safety
///
/// `object` is `size` fresh bytes, the size of an instance.
pub(crate) unsafe fn format(object: usize, header: usize, size: usize) {
    // SAFETY: the caller's contract. A zeroed slot is `null`.
    unsafe {
        std::ptr::write_bytes((object + HEADER) as *mut u8, 0, size - HEADER);
        store(object, header);
    }
}

/// Write the scalar `value`, of the type `ty` when something says it, as
/// an uncaught throw writes it: an enum's variant by its name, and
/// anything else as its tag says.
fn write_scalar(mutator: &Mutator, out: &mut String, ty: Option<Desc>, value: Value) {
    let bits = value.payload;
    // The enum a variant held as its position is of.
    let variant_of = ty.and_then(|ty| match ty.kind() {
        KIND_ENUM => Some(ty),
        KIND_VARIANT => Some(ty.of_a()),
        _ => None,
    });
    match (value.id(), variant_of) {
        (TAG_INT, Some(of)) => out.push_str(mutator.variant_name(of.a(), bits)),
        (TAG_INT, None) => {
            let _ = write!(out, "{}", bits.cast_signed());
        }
        (TAG_FLOAT, _) => out.push_str(&float_text(f64::from_bits(bits as u64))),
        (TAG_BOOL, _) => out.push_str(if bits & 0xFF != 0 { "true" } else { "false" }),
        _ => out.push_str("<object>"),
    }
}

/// Write `value`, what a slot holds: a field of an instance, an element of
/// an array, or a key or a value of a map. Its type is `ty` when something
/// says it.
///
/// # Safety
///
/// `value` was read from a slot of a live object that holds a value of
/// the type.
unsafe fn write_slot(mutator: &Mutator, out: &mut String, ty: Option<Desc>, value: Value) {
    let ty = ty.map(Desc::not_null);
    match value.id() {
        _ if ty.is_some_and(|ty| ty.kind() == KIND_OPAQUE) => out.push_str("<object>"),
        TAG_NULL => out.push_str("null"),
        TAG_BOOL | TAG_INT | TAG_FLOAT => write_scalar(mutator, out, ty, value),
        // SAFETY: the caller's contract: any other tag is of a reference.
        _ => unsafe { write_value(mutator, out, value.payload, ty) },
    }
}

/// Write the value `value` refers to as an uncaught throw of it is
/// reported: what its header says it is, and what `ty`, the type of the
/// place it was read from, says of the enum variants it holds in place.
///
/// # Safety
///
/// `value` is zero or a live object, of the type `ty` when there is one.
pub(crate) unsafe fn write_value(
    mutator: &Mutator,
    out: &mut String,
    value: usize,
    ty: Option<Desc>,
) {
    if value == 0 {
        out.push_str("null");
        return;
    }
    let ty = ty.map(Desc::not_null);
    // SAFETY: the caller's contract: a box is a header and one value, and
    // an instance has the fields its type's description names.
    unsafe {
        let type_id = load(value) & TYPE_ID_MASK;
        match type_id {
            string if string < STRING_TAGS_END => {
                let _ = write!(out, "{:?}", string::text(value));
            }
            TAG_BOX_INT => {
                let _ = write!(out, "{}", load(value + BOX_VALUE).cast_signed());
            }
            crate::layout::TAG_BIGINT => {
                let _ = write!(out, "{}", crate::bigint::value(value));
            }
            TAG_BOX_FLOAT => {
                out.push_str(&float_text(f64::from_bits(load(value + BOX_VALUE) as u64)));
            }
            TAG_BOX_BOOL => out.push_str(if load(value + BOX_VALUE) & 0xFF != 0 {
                "true"
            } else {
                "false"
            }),
            crate::layout::TAG_BYTES => {
                let _ = write!(out, "<bytes:{}>", load(value + crate::layout::BYTES_LENGTH));
            }
            TAG_ARRAY => {
                let element = ty.filter(|ty| ty.kind() == KIND_ARRAY).map(Desc::of_a);
                out.push('[');
                for index in 0..load(value + ARRAY_LENGTH) {
                    if index != 0 {
                        out.push_str(", ");
                    }
                    let held = Value::load(array::element(value, index));
                    write_slot(mutator, out, element, held);
                }
                out.push(']');
            }
            TAG_MAP => {
                let described = ty.filter(|ty| ty.kind() == KIND_MAP);
                let (key, held) = (described.map(Desc::of_a), described.map(Desc::of_b));
                out.push('{');
                let mut first = true;
                map::for_each(value, |entry_key, entry_value| {
                    if !std::mem::take(&mut first) {
                        out.push_str(", ");
                    }
                    write_slot(mutator, out, key, entry_key);
                    out.push_str(": ");
                    write_slot(mutator, out, held, entry_value);
                });
                out.push('}');
            }
            enum_type if !mutator.is_reference(enum_type) => {
                out.push_str(mutator.variant_name(enum_type, load(value + BOX_VALUE)));
            }
            class => {
                let Some(description) = description(mutator, class) else {
                    // The type table does not say how a tuple, a function
                    // or a future is written.
                    out.push_str("<object>");
                    return;
                };
                out.push_str(description.full_name());
                out.push_str(" {");
                for (index, field) in description.fields().enumerate() {
                    if index != 0 {
                        out.push_str(", ");
                    }
                    out.push_str(field.name);
                    out.push_str(": ");
                    match field.ty {
                        Some(ty) => {
                            let held = Value::load(value + field.offset);
                            write_slot(mutator, out, Some(ty), held);
                        }
                        None => out.push_str("<object>"),
                    }
                }
                out.push('}');
            }
        }
    }
}

/// What the runtime gives a field of an instance it makes.
#[derive(Clone, Copy)]
pub(crate) enum Init<'a> {
    /// An `int` field.
    Int(i64),
    /// A field that holds any of several types, given an `int`: a
    /// reference to its box.
    BoxedInt(i64),
    /// A `string` field.
    Text(&'a str),
    /// A field that holds any of several types, given a `bigint`.
    Big(&'a num_bigint::BigInt),
}

/// The most fields any class the runtime makes an instance of has.
pub(crate) const MOST_FIELDS: usize = 3;

/// The type id of the class `class` of [`crate::layout::WELL_KNOWN`].
pub(crate) fn well_known_type(mutator: &Mutator, class: usize) -> usize {
    let types = mutator.types();
    // SAFETY: the type table starts with where its list of well-known
    // types is: a count, then the type id of each.
    unsafe {
        let list = types + word(types);
        assert!(
            word(types) != 0 && class < word(list),
            "the program names the standard library's panics and errors"
        );
        word(list + 4 + class * 4)
    }
}

/// The status a `baml.panics.Exit` ends the process with, when `error` is
/// one: its `code`, or the nearest status to it when it is past an `i32`.
///
/// # Safety
///
/// `error` is zero or a live object.
pub(crate) unsafe fn exit_status(mutator: &Mutator, error: usize) -> Option<i32> {
    // SAFETY: the caller's contract; an `Exit` has its code in its one
    // field, as an `int`.
    unsafe {
        if error == 0 {
            return None;
        }
        let class = well_known_type(mutator, WELL_KNOWN_EXIT);
        if load(error) & TYPE_ID_MASK != class {
            return None;
        }
        let code = field(error, description(mutator, class)?, 0).payload;
        let code = i64::from_ne_bytes(code.to_ne_bytes());
        Some(i32::try_from(code).unwrap_or(if code < 0 { i32::MIN } else { i32::MAX }))
    }
}

/// A new instance of the well-known class `class` with every field the
/// zero of its type, `0`, `0.0`, `false` or `null`, and where each of its
/// fields is. May collect.
fn blank(mutator: &mut Mutator, class: usize) -> (usize, Desc) {
    let type_id = well_known_type(mutator, class);
    // SAFETY: a well-known type is a class's: its entry says how large an
    // instance is.
    unsafe {
        let description =
            description(mutator, type_id).expect("a well-known type is a described class");
        let size = word(mutator.types() + type_id + 4);
        let flags = if class < WELL_KNOWN_PANICS {
            HEADER_PANIC
        } else {
            0
        };
        let object = mutator.allocate(size);
        format(object, type_id | flags, size);
        for field in description.fields() {
            let zero = match field
                .ty
                .filter(|ty| ty.kind() != KIND_OPAQUE)
                .map(Desc::repr)
            {
                Some(REPR_INT) => Value::int(0),
                Some(REPR_FLOAT) => Value::float(0.0),
                Some(REPR_BOOL) => Value::bool(false),
                _ => continue,
            };
            Value::store(object + field.offset, zero);
        }
        (object, description)
    }
}

/// A new instance of the class `class` of [`crate::layout::WELL_KNOWN`]
/// with these fields, in declaration order. May collect.
pub(crate) fn well_known(mutator: &mut Mutator, class: usize, fields: &[Init<'_>]) -> usize {
    // What the fields refer to is made first and kept through the
    // allocations after it.
    let mut made = [0_usize; MOST_FIELDS];
    assert!(fields.len() <= MOST_FIELDS);
    let slots = made.as_mut_ptr();
    for (index, field) in fields.iter().enumerate() {
        let object = match field {
            Init::Int(_) => 0,
            Init::BoxedInt(value) => {
                let boxed = mutator.allocate_object(TAG_BOX_INT, BOX_SIZE);
                // SAFETY: a box is a header and one value.
                unsafe { store(boxed + BOX_VALUE, usize::from_ne_bytes(value.to_ne_bytes())) };
                boxed
            }
            Init::Text(text) => string::from_str(mutator, text),
            Init::Big(value) => crate::bigint::make(mutator, value),
        };
        // SAFETY: `index` is within `made`, which outlives the roots.
        unsafe {
            slots.add(index).write(object);
            mutator.root_at(slots.add(index) as usize);
        }
    }
    let (object, description) = blank(mutator, class);
    mutator.unroot(fields.len());
    // SAFETY: the description says where each field of the instance is.
    unsafe {
        for (index, field) in fields.iter().enumerate() {
            let value = match field {
                Init::Int(value) => Value::int(*value),
                Init::BoxedInt(_) | Init::Text(_) | Init::Big(_) => {
                    Value::reference(slots.add(index).read())
                }
            };
            set_field(object, description, index, value);
        }
    }
    object
}

/// The `baml.errors.Context` of `error`, thrown through the functions
/// named, innermost first; its frames are outermost first. Nothing says yet which file and line a frame
/// was at, so every frame has no file and line zero; and an error has no
/// cause. May collect.
pub(crate) fn context(mutator: &mut Mutator, error: usize, functions: &[&str]) -> usize {
    // The error, the frames, the file every frame names, and what is being
    // put into the object made next.
    let mut held = [error, 0, 0, 0];
    let slots = held.as_mut_ptr();
    for index in 0..held.len() {
        // SAFETY: `index` is within `held`, which outlives the roots.
        mutator.root_at(unsafe { slots.add(index) } as usize);
    }
    let count = functions.len();
    // SAFETY: each object is filled in before the next allocation, and
    // every reference is read back from its root after one.
    unsafe {
        let frames = mutator.allocate(array::size(count));
        array::format(frames, count);
        slots.add(1).write(frames);
        slots.add(2).write(string::from_str(mutator, ""));
        for (index, function) in functions.iter().rev().enumerate() {
            slots.add(3).write(string::from_str(mutator, function));
            let (frame, described) = blank(mutator, WELL_KNOWN_STACK_FRAME);
            set_field(frame, described, 0, Value::reference(slots.add(2).read()));
            set_field(frame, described, 2, Value::reference(slots.add(3).read()));
            Value::store(
                array::element(slots.add(1).read(), index),
                Value::reference(frame),
            );
        }
        let (trace, described) = blank(mutator, WELL_KNOWN_STACK_TRACE);
        set_field(trace, described, 0, Value::reference(slots.add(1).read()));
        slots.add(3).write(trace);
        let (context, described) = blank(mutator, WELL_KNOWN_CONTEXT);
        set_field(context, described, 0, Value::reference(slots.add(0).read()));
        set_field(context, described, 1, Value::reference(slots.add(3).read()));
        mutator.unroot(held.len());
        context
    }
}

/// The names of the functions the context `context` was thrown through,
/// innermost first.
///
/// # Safety
///
/// `context` is a live `baml.errors.Context` made by [`context`].
pub(crate) unsafe fn context_functions<'a>(mutator: &Mutator, context: usize) -> Vec<&'a str> {
    // SAFETY: the caller's contract: the context's second field is its
    // stack trace, whose one field is its frames, whose third is a name.
    unsafe {
        let held = |object: usize, class: usize, index: usize| {
            let description = description(mutator, well_known_type(mutator, class))
                .expect("a well-known type is a described class");
            field(object, description, index).payload
        };
        let trace = held(context, WELL_KNOWN_CONTEXT, 1);
        let frames = held(trace, WELL_KNOWN_STACK_TRACE, 0);
        (0..load(frames + ARRAY_LENGTH))
            .rev()
            .map(|index| {
                let frame = Value::load(array::element(frames, index)).payload;
                string::text(held(frame, WELL_KNOWN_STACK_FRAME, 2))
            })
            .collect()
    }
}

/// A `baml.errors.Context` as its `to_string` writes it, when `value` is
/// one: the functions the error was thrown through, as an uncaught throw
/// lists them, and then the error.
///
/// # Safety
///
/// `value` is a live object.
pub(crate) unsafe fn context_text(mutator: &Mutator, value: usize) -> Option<String> {
    // SAFETY: the caller's contract; a context made by [`context`] has its
    // error in its first field.
    unsafe {
        let class = well_known_type(mutator, WELL_KNOWN_CONTEXT);
        if load(value) & TYPE_ID_MASK != class {
            return None;
        }
        let mut text = String::from("Traceback (most recent call last):\n");
        for function in context_functions(mutator, value).into_iter().rev() {
            let _ = writeln!(text, "  in {function}");
        }
        let error = field(value, description(mutator, class)?, 0).payload;
        if error != 0 && load(error) & TYPE_ID_MASK < STRING_TAGS_END {
            text.push_str(string::text(error));
        } else {
            write_value(mutator, &mut text, error, None);
        }
        Some(text)
    }
}
