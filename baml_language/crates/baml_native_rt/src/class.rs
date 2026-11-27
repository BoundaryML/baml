//! Class instances as the runtime sees them: how one is written when it is
//! thrown and nothing catches it, and how the runtime makes one of the
//! standard library's panics and errors.
//!
//! Both read the class's object and the description it names; see
//! [`crate::layout::CLASS_PACKAGE`].

use std::fmt::Write as _;

use crate::{
    array,
    describe::{Desc, Types},
    float_text,
    heap::{Mutator, Value, load, store},
    json_layout::{KIND_ARRAY, KIND_ENUM, KIND_MAP, KIND_OPAQUE, REPR_BOOL, REPR_FLOAT, REPR_INT},
    layout::{
        ARRAY_LENGTH, CLASS_KIND, CLASS_PANIC, CLASS_SLOT_COUNT, CLASS_VARIANTS, HEADER,
        HEADER_PANIC, INSTANCE_CLASS, STRING_TAGS_END, TAG_ARRAY, TAG_BOOL, TAG_ENUM_VARIANT,
        TAG_FLOAT, TAG_INSTANCE, TAG_INT, TAG_MAP, TAG_NULL, TYPE_ID_MASK, VARIANT_NAME,
        WELL_KNOWN_CONTEXT, WELL_KNOWN_EXIT, WELL_KNOWN_STACK_FRAME, WELL_KNOWN_STACK_TRACE, WORD,
        instance_size,
    },
    map, string,
};

/// Read the 32-bit word at `address`.
///
/// # Safety
///
/// `address` is a `u32` of a live class object.
unsafe fn word(address: usize) -> usize {
    // SAFETY: the caller's contract.
    unsafe { (address as *const u32).read() as usize }
}

/// How many slots an instance of the class whose object is at `class` has,
/// or how many variants the enum whose object it is has.
///
/// # Safety
///
/// `class` is a live class object.
pub(crate) unsafe fn slot_count(class: usize) -> usize {
    // SAFETY: the caller's contract.
    unsafe { word(class + CLASS_SLOT_COUNT) }
}

/// The object of variant `index` of the enum whose class object is at
/// `class`.
///
/// # Safety
///
/// `class` is the live class object of an enum with that many variants.
pub(crate) unsafe fn variant(class: usize, index: usize) -> usize {
    // SAFETY: the caller's contract: an enum's class object has the
    // address of a word for each of its variants.
    unsafe {
        debug_assert!(index < slot_count(class));
        load(load(class + CLASS_VARIANTS) + index * WORD)
    }
}

/// The name of the enum variant whose object is at `variant`.
///
/// # Safety
///
/// `variant` is a live variant object, and the result is not used after
/// the next allocation.
pub(crate) unsafe fn variant_name<'a>(variant: usize) -> &'a str {
    // SAFETY: the caller's contract: a variant holds its name, a string.
    unsafe { string::text(load(variant + VARIANT_NAME)) }
}

/// The description of the class of the instance at `object`.
///
/// # Safety
///
/// `object` is a live instance.
unsafe fn description(mutator: &Mutator, object: usize) -> Option<Desc> {
    // SAFETY: the caller's contract: an instance holds its class object.
    unsafe { Types::of_table(mutator.types()).of_class(load(object + INSTANCE_CLASS)) }
}

/// The description of the well-known class `class`.
fn well_known_description(mutator: &Mutator, class: usize) -> Desc {
    let types = Types::of_table(mutator.types());
    // SAFETY: the program's table holds live class objects.
    unsafe { types.of_class(types.well_known(class)) }
        .expect("a well-known class has a description")
}

/// Whether the object at `object` is an instance of the well-known class
/// `class`.
///
/// # Safety
///
/// `object` is a live object.
pub(crate) unsafe fn is_well_known(mutator: &Mutator, object: usize, class: usize) -> bool {
    // SAFETY: the caller's contract.
    unsafe {
        load(object) & TYPE_ID_MASK == TAG_INSTANCE
            && load(object + INSTANCE_CLASS) == Types::of_table(mutator.types()).well_known(class)
    }
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

/// The bytes of an instance of the class whose object is at `class`.
///
/// # Safety
///
/// `class` is the live class object of a class.
pub(crate) unsafe fn size(class: usize) -> usize {
    // SAFETY: the caller's contract.
    instance_size(unsafe { slot_count(class) })
}

/// Lay out an instance of the class whose object is at `class` at
/// `object`: its header, with what says it is a panic when the class is
/// one, its class, and every slot `null`.
///
/// # Safety
///
/// `object` is fresh bytes, as many as [`size`] says, and `class` the live
/// class object of a class.
pub(crate) unsafe fn format(object: usize, class: usize) {
    // SAFETY: the caller's contract. A zeroed slot is `null`.
    unsafe {
        let panic = word(class + CLASS_KIND) & CLASS_PANIC as usize != 0;
        std::ptr::write_bytes((object + HEADER) as *mut u8, 0, size(class) - HEADER);
        store(object, TAG_INSTANCE | if panic { HEADER_PANIC } else { 0 });
        store(object + INSTANCE_CLASS, class);
    }
}

/// Write the scalar `value` as an uncaught throw writes it.
fn write_scalar(out: &mut String, value: Value) {
    let bits = value.payload;
    match value.id() {
        TAG_INT => {
            let _ = write!(out, "{}", bits.cast_signed());
        }
        TAG_FLOAT => out.push_str(&float_text(f64::from_bits(bits as u64))),
        TAG_BOOL => out.push_str(if bits & 0xFF != 0 { "true" } else { "false" }),
        _ => out.push_str("<object>"),
    }
}

/// Write `value` as an uncaught throw of it is reported: what its tag
/// says it is and, for an object, what its header does. `ty` is the type
/// of the place it was read from, which says what nothing writes.
///
/// # Safety
///
/// `value` is a value as a slot holds one, of the type `ty` when there is
/// one: when its tag is a pointer's, its payload is a live object.
pub(crate) unsafe fn write_value(
    mutator: &Mutator,
    out: &mut String,
    value: Value,
    ty: Option<Desc>,
) {
    let ty = ty.map(Desc::not_null);
    match value.id() {
        _ if ty.is_some_and(|ty| ty.kind() == KIND_OPAQUE) => out.push_str("<object>"),
        TAG_NULL => out.push_str("null"),
        TAG_BOOL | TAG_INT | TAG_FLOAT => write_scalar(out, value),
        // SAFETY: the caller's contract: any other tag is of a pointer.
        _ => unsafe { write_object(mutator, out, value.payload, ty) },
    }
}

/// Write the object at `value`: what its header says it is, and what `ty`
/// says of what it holds.
///
/// # Safety
///
/// `value` is a live object, of the type `ty` when there is one.
unsafe fn write_object(mutator: &Mutator, out: &mut String, value: usize, ty: Option<Desc>) {
    // SAFETY: the caller's contract: a variant holds its name, and an
    // instance has the fields its class's description names.
    unsafe {
        let type_id = load(value) & TYPE_ID_MASK;
        match type_id {
            string if string < STRING_TAGS_END => {
                let _ = write!(out, "{:?}", string::text(value));
            }
            crate::layout::TAG_BIGINT => {
                let _ = write!(out, "{}", crate::bigint::value(value));
            }
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
                    write_value(mutator, out, held, element);
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
                    write_value(mutator, out, entry_key, key);
                    out.push_str(": ");
                    write_value(mutator, out, entry_value, held);
                });
                out.push('}');
            }
            TAG_ENUM_VARIANT => out.push_str(variant_name(value)),
            TAG_INSTANCE => {
                let Some(description) = description(mutator, value) else {
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
                            write_value(mutator, out, held, Some(ty));
                        }
                        None => out.push_str("<object>"),
                    }
                }
                out.push('}');
            }
            // Nothing says how a tuple, a function or a future is written.
            _ => out.push_str("<object>"),
        }
    }
}

/// What the runtime gives a field of an instance it makes.
#[derive(Clone, Copy)]
pub(crate) enum Init<'a> {
    /// An `int`, in a field of that type or in one that holds any of
    /// several types.
    Int(i64),
    /// A `string` field.
    Text(&'a str),
    /// A field that holds any of several types, given a `bigint`.
    Big(&'a num_bigint::BigInt),
}

/// The most fields any class the runtime makes an instance of has.
pub(crate) const MOST_FIELDS: usize = 3;

/// The status a `baml.panics.Exit` ends the process with, when `error` is
/// one: its `code`, or the nearest status to it when it is past an `i32`.
///
/// # Safety
///
/// `error` is a value as a slot holds one.
pub(crate) unsafe fn exit_status(mutator: &Mutator, error: Value) -> Option<i32> {
    // SAFETY: the caller's contract; an `Exit` has its code in its one
    // field, as an `int`.
    unsafe {
        if !error.is_pointer() || !is_well_known(mutator, error.payload, WELL_KNOWN_EXIT) {
            return None;
        }
        let description = well_known_description(mutator, WELL_KNOWN_EXIT);
        let code = field(error.payload, description, 0).payload;
        let code = i64::from_ne_bytes(code.to_ne_bytes());
        Some(i32::try_from(code).unwrap_or(if code < 0 { i32::MIN } else { i32::MAX }))
    }
}

/// A new instance of the well-known class `class` with every field the
/// zero of its type, `0`, `0.0`, `false`, an enum's first variant or
/// `null`, and where each of its fields is. May collect.
fn blank(mutator: &mut Mutator, class: usize) -> (usize, Desc) {
    let description = well_known_description(mutator, class);
    // SAFETY: the program's table holds the live class object of each
    // well-known class, which says how large an instance is. It is read
    // again after the allocation, which may move it.
    unsafe {
        let types = Types::of_table(mutator.types());
        let object = mutator.allocate(size(types.well_known(class)));
        format(object, types.well_known(class));
        for field in description.fields() {
            let Some(ty) = field.ty.filter(|ty| ty.kind() != KIND_OPAQUE) else {
                continue;
            };
            let zero = match ty.repr() {
                REPR_INT => Value::int(0),
                REPR_FLOAT => Value::float(0.0),
                REPR_BOOL => Value::bool(false),
                _ if ty.kind() == KIND_ENUM && slot_count(ty.class_object()) != 0 => {
                    Value::reference(variant(ty.class_object(), 0))
                }
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
                Init::Text(_) | Init::Big(_) => Value::reference(slots.add(index).read()),
            };
            set_field(object, description, index, value);
        }
    }
    object
}

/// The `baml.errors.Context` of `error`, thrown through the functions
/// named, innermost first; its frames are outermost first. Nothing says
/// yet which file and line a frame was at, so every frame has no file and
/// line zero; and an error has no cause. May collect.
pub(crate) fn context(mutator: &mut Mutator, mut error: Value, functions: &[&str]) -> usize {
    // The frames, the file every frame names, and what is being put into
    // the object made next.
    let mut held = [0_usize; 3];
    let slots = held.as_mut_ptr();
    mutator.root_value(&mut error);
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
        slots.add(0).write(frames);
        slots.add(1).write(string::from_str(mutator, ""));
        for (index, function) in functions.iter().rev().enumerate() {
            slots.add(2).write(string::from_str(mutator, function));
            let (frame, described) = blank(mutator, WELL_KNOWN_STACK_FRAME);
            set_field(frame, described, 0, Value::reference(slots.add(1).read()));
            set_field(frame, described, 2, Value::reference(slots.add(2).read()));
            Value::store(
                array::element(slots.add(0).read(), index),
                Value::reference(frame),
            );
        }
        let (trace, described) = blank(mutator, WELL_KNOWN_STACK_TRACE);
        set_field(trace, described, 0, Value::reference(slots.add(0).read()));
        slots.add(2).write(trace);
        let (context, described) = blank(mutator, WELL_KNOWN_CONTEXT);
        let error = (&raw const error).read_volatile();
        set_field(context, described, 0, error);
        set_field(context, described, 1, Value::reference(slots.add(2).read()));
        mutator.unroot(1 + held.len());
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
            field(object, well_known_description(mutator, class), index).payload
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
        if !is_well_known(mutator, value, WELL_KNOWN_CONTEXT) {
            return None;
        }
        let mut text = String::from("Traceback (most recent call last):\n");
        for function in context_functions(mutator, value).into_iter().rev() {
            let _ = writeln!(text, "  in {function}");
        }
        let error = field(
            value,
            well_known_description(mutator, WELL_KNOWN_CONTEXT),
            0,
        );
        if error.is_pointer() && error.id() < STRING_TAGS_END {
            text.push_str(string::text(error.payload));
        } else {
            write_value(mutator, &mut text, error, None);
        }
        Some(text)
    }
}
