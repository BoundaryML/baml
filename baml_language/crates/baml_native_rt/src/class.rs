//! Class instances as the runtime sees them: how one is written when it is
//! thrown and nothing catches it, and how the runtime makes one of the
//! standard library's panics and errors.
//!
//! Both read the description the compiler leaves with a class's type; see
//! [`crate::layout::TYPE_HEADER`].

use std::fmt::Write as _;

use crate::{
    float_text,
    heap::{Mutator, load, store},
    layout::{
        ARRAY_DATA, ARRAY_LENGTH, ARRAY_SIZE, BOX_SIZE, BOX_VALUE, DESCRIPTION_FIELD,
        DESCRIPTION_HEADER, FIELD_BOOL, FIELD_ENUM, FIELD_FLOAT, FIELD_INT, FIELD_OPTIONAL,
        FIELD_REF, HEADER_PANIC, KIND_FIXED, STORAGE_CAPACITY, STORAGE_ELEMENTS, STRING_TYPES_END,
        TYPE_ARRAY_OF_REFS, TYPE_BOX_BOOL, TYPE_BOX_FLOAT, TYPE_BOX_INT, TYPE_DESCRIBED,
        TYPE_ID_MASK, TYPE_REFS, WELL_KNOWN_CONTEXT, WELL_KNOWN_PANICS, WELL_KNOWN_STACK_FRAME,
        WELL_KNOWN_STACK_TRACE, WORD,
    },
    string,
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

/// The text at `offset` of the type table.
///
/// # Safety
///
/// The compiler put `length` bytes of UTF-8 there, in the executable's
/// data.
unsafe fn name(types: usize, offset: usize, length: usize) -> &'static str {
    // SAFETY: the caller's contract.
    unsafe {
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(
            (types + offset) as *const u8,
            length,
        ))
    }
}

/// The description of a class: where in the type table it is.
#[derive(Clone, Copy)]
struct Description {
    types: usize,
    at: usize,
}

/// One field of a described class.
struct Field {
    name: &'static str,
    /// Its byte offset in an instance.
    offset: usize,
    kind: u32,
    /// The type id of the boxes of the enum it holds a variant of.
    enum_type: usize,
}

impl Description {
    /// The description of the type `type_id`, when it is a class's.
    ///
    /// # Safety
    ///
    /// `type_id` is an entry of the mutator's type table.
    unsafe fn of(mutator: &Mutator, type_id: usize) -> Option<Self> {
        let types = mutator.types();
        // SAFETY: the caller's contract: an entry is four words.
        let (kind, described) = unsafe { (word(types + type_id), word(types + type_id + 12)) };
        (kind == KIND_FIXED as usize && described & TYPE_DESCRIBED as usize != 0).then(|| Self {
            types,
            at: types + (described & !(TYPE_DESCRIBED as usize)),
        })
    }

    fn name(self) -> &'static str {
        // SAFETY: a description starts with where its name is.
        unsafe { name(self.types, word(self.at), word(self.at + 4)) }
    }

    fn field(self, index: usize) -> Field {
        let record = self.at + DESCRIPTION_HEADER + index * DESCRIPTION_FIELD;
        // SAFETY: the description has this field's record.
        unsafe {
            Field {
                name: name(self.types, word(record), word(record + 4)),
                offset: word(record + 8),
                kind: ((record + 12) as *const u32).read_unaligned(),
                enum_type: word(record + 16),
            }
        }
    }

    fn fields(self) -> impl Iterator<Item = Field> {
        // SAFETY: a description's third word is how many fields it has.
        let count = unsafe { word(self.at + 8) };
        (0..count).map(move |index| self.field(index))
    }
}

/// Write `bits`, a scalar of kind `kind`, as an uncaught throw writes it.
fn write_scalar(mutator: &Mutator, out: &mut String, kind: u32, enum_type: usize, bits: usize) {
    match kind {
        FIELD_INT => {
            let _ = write!(out, "{}", bits.cast_signed());
        }
        FIELD_FLOAT => out.push_str(&float_text(f64::from_bits(bits as u64))),
        FIELD_BOOL => out.push_str(if bits & 0xFF != 0 { "true" } else { "false" }),
        FIELD_ENUM => out.push_str(mutator.variant_name(enum_type, bits)),
        _ => out.push_str("<object>"),
    }
}

/// Write the value `value` refers to as an uncaught throw of it is
/// reported: what its header says it is.
///
/// # Safety
///
/// `value` is zero or a live object.
pub(crate) unsafe fn write_value(mutator: &Mutator, out: &mut String, value: usize) {
    if value == 0 {
        out.push_str("null");
        return;
    }
    // SAFETY: the caller's contract: a box is a header and one value, and
    // an instance has the fields its type's description names.
    unsafe {
        let type_id = load(value) & TYPE_ID_MASK;
        match type_id {
            string if string < STRING_TYPES_END => {
                let _ = write!(out, "{:?}", string::text(value));
            }
            TYPE_BOX_INT | TYPE_BOX_FLOAT | TYPE_BOX_BOOL => {
                let kind = match type_id {
                    TYPE_BOX_INT => FIELD_INT,
                    TYPE_BOX_FLOAT => FIELD_FLOAT,
                    _ => FIELD_BOOL,
                };
                write_scalar(mutator, out, kind, 0, load(value + BOX_VALUE));
            }
            enum_type if !mutator.is_reference(enum_type) => {
                out.push_str(mutator.variant_name(enum_type, load(value + BOX_VALUE)));
            }
            class => {
                let Some(description) = Description::of(mutator, class) else {
                    // The type table does not say how an array, a tuple or
                    // a function is written.
                    out.push_str("<object>");
                    return;
                };
                out.push_str(description.name());
                out.push_str(" {");
                for (index, field) in description.fields().enumerate() {
                    if index != 0 {
                        out.push_str(", ");
                    }
                    out.push_str(field.name);
                    out.push_str(": ");
                    let held = load(value + field.offset);
                    if field.kind == FIELD_REF {
                        write_value(mutator, out, held);
                    } else if field.kind & FIELD_OPTIONAL != 0
                        && load(value + field.offset + WORD) == 0
                    {
                        out.push_str("null");
                    } else {
                        let kind = field.kind & !FIELD_OPTIONAL;
                        write_scalar(mutator, out, kind, field.enum_type, held);
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

/// A new instance of the well-known class `class` with every field zero,
/// and where each of its fields is. May collect.
fn blank(mutator: &mut Mutator, class: usize) -> (usize, Description) {
    let type_id = well_known_type(mutator, class);
    // SAFETY: a well-known type is a class's: its entry says how large an
    // instance is.
    unsafe {
        let description =
            Description::of(mutator, type_id).expect("a well-known type is a described class");
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
                Init::BoxedInt(_) | Init::Text(_) => slots.add(index).read(),
            };
            store(object + description.field(index).offset, value);
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
            store(frame + described.field(0).offset, slots.add(2).read());
            store(frame + described.field(2).offset, slots.add(3).read());
            store(load(slots.add(1).read() + ARRAY_DATA) + index * WORD, frame);
        }
        let (trace, described) = blank(mutator, WELL_KNOWN_STACK_TRACE);
        store(trace + described.field(0).offset, slots.add(1).read());
        slots.add(3).write(trace);
        let (context, described) = blank(mutator, WELL_KNOWN_CONTEXT);
        store(context + described.field(0).offset, slots.add(0).read());
        store(context + described.field(1).offset, slots.add(3).read());
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
            let description = Description::of(mutator, well_known_type(mutator, class))
                .expect("a well-known type is a described class");
            load(object + description.field(index).offset)
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
        let error = load(value + Description::of(mutator, class)?.field(0).offset);
        if error != 0 && load(error) & TYPE_ID_MASK < STRING_TYPES_END {
            text.push_str(string::text(error));
        } else {
            write_value(mutator, &mut text, error);
        }
        Some(text)
    }
}
