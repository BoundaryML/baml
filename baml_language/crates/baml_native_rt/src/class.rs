//! Class instances as the runtime sees them: how one is written when it is
//! thrown and nothing catches it, and how the runtime makes one of the
//! standard library's panics and errors.
//!
//! Both read the description the compiler leaves with a class's type; see
//! [`crate::layout::TYPE_HEADER`].

use std::fmt::Write as _;

use crate::{
    describe::{Desc, Types},
    float_text,
    heap::{Mutator, load, store},
    json_layout::{
        KIND_ARRAY, KIND_ENUM, KIND_MAP, KIND_OPAQUE, KIND_VARIANT, REPR_BOOL, REPR_BOXED,
        REPR_FLOAT, REPR_INT, REPR_NULL, REPR_PAIR, REPR_REF,
    },
    layout::{
        ARRAY_DATA, ARRAY_LENGTH, ARRAY_SIZE, BOX_SIZE, BOX_VALUE, HEADER_PANIC, MAP_ENTRIES,
        STORAGE_CAPACITY, STORAGE_ELEMENTS, STRING_TYPES_END, TYPE_ARRAY_OF_REFS,
        TYPE_ARRAY_OF_VALUES, TYPE_BOX_BOOL, TYPE_BOX_FLOAT, TYPE_BOX_INT, TYPE_ENTRIES_KEY_REF,
        TYPE_ENTRIES_REFS, TYPE_ENTRIES_VALUE_REF, TYPE_ID_MASK, TYPE_MAP, TYPE_REFS,
        WELL_KNOWN_CONTEXT, WELL_KNOWN_EXIT, WELL_KNOWN_PANICS, WELL_KNOWN_STACK_FRAME,
        WELL_KNOWN_STACK_TRACE, WORD,
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

/// Write `bits`, a scalar of the type `ty`, as an uncaught throw writes
/// it.
fn write_scalar(mutator: &Mutator, out: &mut String, ty: Desc, bits: usize) {
    match (ty.kind(), ty.repr()) {
        (KIND_ENUM, _) => out.push_str(mutator.variant_name(ty.a(), bits)),
        (KIND_VARIANT, _) => out.push_str(mutator.variant_name(ty.of_a().a(), bits)),
        (KIND_OPAQUE, _) => out.push_str("<object>"),
        (_, REPR_INT) => {
            let _ = write!(out, "{}", bits.cast_signed());
        }
        (_, REPR_FLOAT) => out.push_str(&float_text(f64::from_bits(bits as u64))),
        (_, REPR_BOOL) => out.push_str(if bits & 0xFF != 0 { "true" } else { "false" }),
        _ => out.push_str("<object>"),
    }
}

/// Whether one word that is not a reference is all of a value of the type
/// `ty`, which [`write_scalar`] then writes.
fn is_scalar(ty: Desc) -> bool {
    ty.kind() != KIND_OPAQUE && matches!(ty.repr(), REPR_INT | REPR_FLOAT | REPR_BOOL)
}

/// Write the value in the slot at `slot`, whose type is `ty`.
///
/// # Safety
///
/// `slot` is a slot of a live object that holds a value of the type.
unsafe fn write_slot(mutator: &Mutator, out: &mut String, ty: Desc, slot: usize) {
    // SAFETY: the caller's contract: an optional scalar is two slots, the
    // second whether there is a value.
    unsafe {
        match ty.repr() {
            _ if ty.kind() == KIND_OPAQUE => out.push_str("<object>"),
            REPR_NULL => out.push_str("null"),
            REPR_REF | REPR_BOXED => write_value(mutator, out, load(slot), Some(ty)),
            REPR_PAIR if load(slot + WORD) == 0 => out.push_str("null"),
            REPR_PAIR => write_scalar(mutator, out, ty.not_null(), load(slot)),
            _ => write_scalar(mutator, out, ty, load(slot)),
        }
    }
}

/// Write `held`, an element of an array or a key or a value of a map,
/// whose type is `ty` when something says it.
///
/// # Safety
///
/// `held` is a reference, zero or to a live object, when `is_ref`.
unsafe fn write_held(
    mutator: &Mutator,
    out: &mut String,
    ty: Option<Desc>,
    held: usize,
    is_ref: bool,
) {
    match ty {
        // SAFETY: the caller's contract.
        _ if is_ref => unsafe { write_value(mutator, out, held, ty) },
        Some(ty) => write_scalar(mutator, out, ty, held),
        None => out.push_str("<object>"),
    }
}

/// Write the value `value` refers to as an uncaught throw of it is
/// reported: what its header says it is, and what `ty`, the type of the
/// place it was read from, says of the words of an array or a map that
/// are not references.
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
            string if string < STRING_TYPES_END => {
                let _ = write!(out, "{:?}", string::text(value));
            }
            TYPE_BOX_INT => {
                let _ = write!(out, "{}", load(value + BOX_VALUE).cast_signed());
            }
            crate::layout::TYPE_BIGINT => {
                let _ = write!(out, "{}", crate::bigint::value(value));
            }
            TYPE_BOX_FLOAT => {
                out.push_str(&float_text(f64::from_bits(load(value + BOX_VALUE) as u64)));
            }
            TYPE_BOX_BOOL => out.push_str(if load(value + BOX_VALUE) & 0xFF != 0 {
                "true"
            } else {
                "false"
            }),
            crate::layout::TYPE_BYTES => {
                let _ = write!(out, "<bytes:{}>", load(value + crate::layout::BYTES_LENGTH));
            }
            TYPE_ARRAY_OF_REFS | TYPE_ARRAY_OF_VALUES => {
                let element = ty.filter(|ty| ty.kind() == KIND_ARRAY).map(Desc::of_a);
                let is_ref = type_id == TYPE_ARRAY_OF_REFS;
                let length = load(value + ARRAY_LENGTH);
                if !is_ref && length != 0 && !element.is_some_and(is_scalar) {
                    // Words only their type could tell apart.
                    out.push_str("<object>");
                    return;
                }
                out.push('[');
                for index in 0..length {
                    if index != 0 {
                        out.push_str(", ");
                    }
                    let held = load(load(value + ARRAY_DATA) + index * WORD);
                    write_held(mutator, out, element, held, is_ref);
                }
                out.push(']');
            }
            TYPE_MAP => {
                let described = ty.filter(|ty| ty.kind() == KIND_MAP);
                let (key, held) = (described.map(Desc::of_a), described.map(Desc::of_b));
                let entries = load(load(value + MAP_ENTRIES)) & TYPE_ID_MASK;
                let key_is_ref = matches!(entries, TYPE_ENTRIES_KEY_REF | TYPE_ENTRIES_REFS);
                let held_is_ref = matches!(entries, TYPE_ENTRIES_VALUE_REF | TYPE_ENTRIES_REFS);
                if map::len(value) != 0
                    && !((key_is_ref || key.is_some_and(is_scalar))
                        && (held_is_ref || held.is_some_and(is_scalar)))
                {
                    out.push_str("<object>");
                    return;
                }
                out.push('{');
                let mut first = true;
                map::for_each(value, |entry_key, entry_value| {
                    if !std::mem::take(&mut first) {
                        out.push_str(", ");
                    }
                    write_held(mutator, out, key, entry_key, key_is_ref);
                    out.push_str(": ");
                    write_held(mutator, out, held, entry_value, held_is_ref);
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
                        Some(ty) => write_slot(mutator, out, ty, value + field.offset),
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
        let code = load(error + field_at(description(mutator, class)?, 0));
        let code = i64::from_ne_bytes(code.to_ne_bytes());
        Some(i32::try_from(code).unwrap_or(if code < 0 { i32::MIN } else { i32::MAX }))
    }
}

/// A new instance of the well-known class `class` with every field zero,
/// and where each of its fields is. May collect.
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
        let object = mutator.allocate_object(type_id | flags, size);
        std::ptr::write_bytes((object + WORD) as *mut u8, 0, size - WORD);
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
                let boxed = mutator.allocate_object(TYPE_BOX_INT, BOX_SIZE);
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
                Init::Int(value) => usize::from_ne_bytes(value.to_ne_bytes()),
                Init::BoxedInt(_) | Init::Text(_) | Init::Big(_) => slots.add(index).read(),
            };
            store(object + field_at(description, index), value);
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
        let frames = mutator.allocate(ARRAY_SIZE + STORAGE_ELEMENTS + count * WORD);
        let storage = frames + ARRAY_SIZE;
        store(frames, TYPE_ARRAY_OF_REFS);
        store(frames + ARRAY_LENGTH, count);
        store(frames + ARRAY_DATA, storage + STORAGE_ELEMENTS);
        store(storage, TYPE_REFS);
        store(storage + STORAGE_CAPACITY, count);
        std::ptr::write_bytes((storage + STORAGE_ELEMENTS) as *mut usize, 0, count);
        slots.add(1).write(frames);
        slots.add(2).write(string::from_str(mutator, ""));
        for (index, function) in functions.iter().rev().enumerate() {
            slots.add(3).write(string::from_str(mutator, function));
            let (frame, described) = blank(mutator, WELL_KNOWN_STACK_FRAME);
            store(frame + field_at(described, 0), slots.add(2).read());
            store(frame + field_at(described, 2), slots.add(3).read());
            store(load(slots.add(1).read() + ARRAY_DATA) + index * WORD, frame);
        }
        let (trace, described) = blank(mutator, WELL_KNOWN_STACK_TRACE);
        store(trace + field_at(described, 0), slots.add(1).read());
        slots.add(3).write(trace);
        let (context, described) = blank(mutator, WELL_KNOWN_CONTEXT);
        store(context + field_at(described, 0), slots.add(0).read());
        store(context + field_at(described, 1), slots.add(3).read());
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
        let field = |object: usize, class: usize, index: usize| {
            let description = description(mutator, well_known_type(mutator, class))
                .expect("a well-known type is a described class");
            load(object + field_at(description, index))
        };
        let trace = field(context, WELL_KNOWN_CONTEXT, 1);
        let frames = field(trace, WELL_KNOWN_STACK_TRACE, 0);
        (0..load(frames + ARRAY_LENGTH))
            .rev()
            .map(|index| {
                let frame = load(load(frames + ARRAY_DATA) + index * WORD);
                string::text(field(frame, WELL_KNOWN_STACK_FRAME, 2))
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
        let error = load(value + field_at(description(mutator, class)?, 0));
        if error != 0 && load(error) & TYPE_ID_MASK < STRING_TYPES_END {
            text.push_str(string::text(error));
        } else {
            write_value(mutator, &mut text, error, None);
        }
        Some(text)
    }
}
