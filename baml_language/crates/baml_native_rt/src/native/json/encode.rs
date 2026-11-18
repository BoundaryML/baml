//! Writing a value as JSON: text, or a `json` value.
//!
//! The walk keeps its own stack, so a value nests as deep as memory
//! allows, as it does in the interpreter.
//!
//! A value whose class has a `to_json` of its own is written as what that
//! returns, which is a `json`. The walk calls nothing: it is told what
//! each returned ([`Overrides::Splice`]), in the order [`overriding`] finds
//! the values in, or only counts the ones it meets ([`Overrides::Count`]).
//! The interpreter works the same way, and for the same reason runs every
//! `to_json` before the walk that can refuse a value.

use std::{collections::HashSet, fmt::Write as _};

use super::{Refused, STRING_MAP_REQUIRED, out::Out, string_keyed, type_of, values_are_refs};
use crate::{
    describe::{Desc, Types},
    heap::load,
    json_layout::{
        HOOK_TO_JSON, KIND_ARRAY, KIND_ENUM, KIND_MAP, KIND_VARIANT, REPR_BOOL, REPR_BOXED,
        REPR_FLOAT, REPR_INT, REPR_NULL, REPR_PAIR, REPR_REF,
    },
    layout::{
        ARRAY_DATA, ARRAY_LENGTH, BOX_VALUE, STRING_TYPES_END, TYPE_ARRAY_OF_REFS,
        TYPE_ARRAY_OF_VALUES, TYPE_BOX_BOOL, TYPE_BOX_FLOAT, TYPE_BOX_INT, TYPE_MAP, WORD,
    },
    map,
};

/// The widest indent `stringify` takes.
const MAX_INDENT: usize = 10;

#[derive(Clone, Copy)]
pub(super) struct Options {
    pub(super) aliased: bool,
    pub(super) omit_nulls: bool,
}

/// What the walk writes to.
pub(super) trait Sink {
    /// Whether a `float` that is not finite is written as it is.
    const KEEPS_NON_FINITE: bool;
    fn null(&mut self);
    fn bool(&mut self, value: bool);
    fn int(&mut self, value: i64);
    fn float(&mut self, value: f64);
    /// A string: its text, and the object when it is one already.
    fn string(&mut self, text: &str, object: usize);
    fn begin_array(&mut self);
    fn item(&mut self, first: bool);
    fn end_array(&mut self, empty: bool);
    fn begin_object(&mut self);
    fn key(&mut self, first: bool, text: &str, object: usize);
    fn end_object(&mut self, empty: bool);
}

/// The text `stringify` writes for one nesting level, from its `indent`
/// argument: an `int` or a `string`, as a union holds them.
pub(super) fn indent_text(indent: usize) -> Result<String, Refused> {
    // SAFETY: an `int | string` is a box or a string, never `null`.
    let refused = unsafe {
        if indent != 0 && type_of(indent) == TYPE_BOX_INT {
            let count = load(indent + BOX_VALUE) as i64;
            match usize::try_from(count) {
                Ok(count) if count <= MAX_INDENT => return Ok(" ".repeat(count)),
                Ok(_) => format!("indent must be at most {MAX_INDENT} spaces, got {count}"),
                Err(_) => format!("indent must not be negative, got {count}"),
            }
        } else if indent != 0 && type_of(indent) < STRING_TYPES_END {
            let text = crate::string::text(indent);
            let width = text.chars().count();
            if width > MAX_INDENT {
                format!("indent must be at most {MAX_INDENT} characters, got {width}")
            } else if text.chars().all(|c| matches!(c, ' ' | '\t' | '\n' | '\r')) {
                return Ok(text.to_owned());
            } else {
                format!("indent must hold only spaces, tabs, and line breaks, got {text:?}")
            }
        } else {
            "indent must be an int or a string".to_owned()
        }
    };
    Err(Refused {
        message: refused,
        path: String::new(),
        reason: "indent",
    })
}

/// A value, found.
#[derive(Clone, Copy)]
enum Val {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(usize),
    /// A variant of the enum described, by position.
    Variant(Desc, usize),
    /// An array, and what its elements are when the static type says.
    Array(usize, Option<Desc>),
    /// A map, and what its keys and values are when the static type says.
    Map(usize, Option<(Desc, Desc)>),
    Instance(usize, Desc),
    Refused(&'static str, &'static str),
}

/// What is left to write of a container.
enum Rest {
    Array {
        elements: Option<Desc>,
    },
    Map {
        entries: Vec<(usize, usize)>,
        values: Option<Desc>,
    },
    Instance {
        /// The key, the field's word and the one after it, and its type.
        fields: Vec<(&'static str, usize, usize, Desc)>,
    },
}

struct Frame {
    object: usize,
    next: usize,
    length: usize,
    rest: Rest,
    /// Whether the container is part of a `json` being copied: an enum
    /// variant in it is its declared name, and what has no JSON form is
    /// `null`.
    json: bool,
}

/// What the walk does at a value whose class has a `to_json` of its own.
pub(super) enum Overrides {
    /// Write `null` and count it.
    Count(usize),
    /// Write what its `to_json` returned. `held` is the address of the
    /// words of `count` such values, in the order [`overriding`] found
    /// them, and then of what the `to_json` of each returned; `next` is
    /// which the walk meets next. A value that is not that one is written
    /// by its fields, as in the interpreter.
    Splice {
        held: usize,
        count: usize,
        next: usize,
    },
}

struct Walker<'a> {
    types: &'a Types,
    options: Options,
    overrides: &'a mut Overrides,
    frames: Vec<Frame>,
    /// The containers the walk is inside, once there are enough that a
    /// scan of the frames for one costs more than a set.
    open: Option<HashSet<usize>>,
}

/// Frames the walk is inside before a cycle is looked for in a set.
const OPEN_SCANNED: usize = 32;

/// Write `value` to `sink`. The value is as a union holds it, and
/// `described` is its static type when there is one. With `json` it is a
/// `json` being copied.
///
/// # Safety
///
/// `value` is zero or a live object of the type described, nothing is
/// allocated outside reserved room during the walk, and what `overrides`
/// splices in is a live `json` for each value [`overriding`] finds.
pub(super) unsafe fn walk(
    types: &Types,
    value: usize,
    described: Option<Desc>,
    options: Options,
    json: bool,
    overrides: &mut Overrides,
    sink: &mut impl Sink,
) -> Result<(), Refused> {
    let mut walker = Walker {
        types,
        options,
        overrides,
        frames: Vec::new(),
        open: None,
    };
    // SAFETY: the caller's contract, for every value reached from it.
    unsafe {
        let root = walker.of_reference(value, described, json);
        walker.visit(root, json, sink)?;
        loop {
            let Some(frame) = walker.frames.last_mut() else {
                return Ok(());
            };
            if frame.next == frame.length {
                let frame = walker.frames.pop();
                if let (Some(open), Some(frame)) = (&mut walker.open, &frame) {
                    open.remove(&frame.object);
                }
                match frame {
                    Some(Frame {
                        rest: Rest::Array { .. },
                        length,
                        ..
                    }) => sink.end_array(length == 0),
                    Some(frame) => sink.end_object(frame.length == 0),
                    None => {}
                }
                continue;
            }
            let index = frame.next;
            frame.next += 1;
            let first = index == 0;
            let json = frame.json;
            // The word of the child, the one after it, and its type.
            let (word, next, described) = match &frame.rest {
                Rest::Array { elements } => {
                    sink.item(first);
                    let word = load(load(frame.object + ARRAY_DATA) + index * WORD);
                    (word, 1, *elements)
                }
                Rest::Map { entries, values } => {
                    let (key, value) = entries[index];
                    sink.key(first, crate::string::text(key), key);
                    (value, 1, *values)
                }
                Rest::Instance { fields } => {
                    let (key, word, next, described) = fields[index];
                    sink.key(first, key, 0);
                    (word, next, Some(described))
                }
            };
            let child = match described {
                Some(described) => walker.of_slot(word, next, described),
                None => walker.of_reference(word, None, json),
            };
            walker.visit(child, json, sink)?;
        }
    }
}

/// The fields of the instance at `object` the walk writes: for each its
/// key, its word and the one after it, and its type. Two fields under one
/// key are one entry, where the first sits, holding the last value.
///
/// # Safety
///
/// `object` is a live instance of the class described.
unsafe fn written_fields(
    object: usize,
    described: Desc,
    options: Options,
) -> Vec<(&'static str, usize, usize, Desc)> {
    let mut fields: Vec<(&'static str, usize, usize, Desc)> = Vec::new();
    for field in described.fields() {
        let Some(ty) = field.ty else { continue };
        // SAFETY: the instance has the field, and the word after it when
        // the field is two slots.
        let (word, next) = unsafe {
            let word = load(object + field.offset);
            let next = if ty.repr() == REPR_PAIR {
                load(object + field.offset + WORD)
            } else {
                1
            };
            (word, next)
        };
        let is_null = match ty.repr() {
            REPR_NULL => true,
            REPR_PAIR => next == 0,
            REPR_INT | REPR_FLOAT | REPR_BOOL => false,
            _ => word == 0,
        };
        let written =
            field.shown(options.aliased) && !((options.omit_nulls || field.omit_null) && is_null);
        if !written {
            continue;
        }
        let key = field.key(options.aliased);
        match fields.iter_mut().find(|(seen, ..)| *seen == key) {
            Some(seen) => *seen = (key, word, next, ty),
            None => fields.push((key, word, next, ty)),
        }
    }
    fields
}

/// The entry of the `to_json` of the class of the instance at `object`,
/// when the class has one of its own.
///
/// # Safety
///
/// `object` is a live object.
pub(super) unsafe fn own_to_json(
    types: &Types,
    object: usize,
) -> Option<crate::native::drive::BamlFn> {
    // SAFETY: the caller's contract.
    types
        .of_type_id(unsafe { type_of(object) })
        .and_then(|described| described.hook(HOOK_TO_JSON))
}

/// Hand `each` every value under `value` whose class has a `to_json` of
/// its own, in the order the walk meets them. Such a value is not looked
/// inside: its `to_json` writes all of it.
///
/// Unlike the walk this refuses nothing, so that every `to_json` runs
/// before a value is refused, as in the interpreter. It looks inside what
/// the walk looks inside (the elements of an array, the values of a map,
/// the fields of an instance the walk writes) and only skips a container
/// it is already inside.
///
/// # Safety
///
/// `value` is zero or a live object, and nothing is allocated outside
/// reserved room meanwhile.
pub(super) unsafe fn overriding(
    types: &Types,
    value: usize,
    options: Options,
    mut each: impl FnMut(usize),
) {
    // Each container the search is inside, with its children and the next
    // of them to look at.
    let mut open: Vec<(usize, Vec<usize>, usize)> = Vec::new();
    let mut inside: HashSet<usize> = HashSet::new();
    let mut next = Some(value);
    loop {
        if let Some(object) = next.take().filter(|object| *object != 0) {
            // SAFETY: the caller's contract, for every value reached.
            unsafe {
                if own_to_json(types, object).is_some() {
                    each(object);
                } else if !inside.contains(&object) {
                    let children = children(types, object, options);
                    if !children.is_empty() {
                        inside.insert(object);
                        open.push((object, children, 0));
                    }
                }
            }
        }
        let Some((object, children, cursor)) = open.last_mut() else {
            return;
        };
        match children.get(*cursor) {
            Some(child) => {
                *cursor += 1;
                next = Some(*child);
            }
            None => {
                inside.remove(object);
                open.pop();
            }
        }
    }
}

/// The objects the walk goes on to from the one at `object`.
///
/// # Safety
///
/// `object` is a live object.
unsafe fn children(types: &Types, object: usize, options: Options) -> Vec<usize> {
    let mut children = Vec::new();
    // SAFETY: the caller's contract: the header says what the object is.
    unsafe {
        match type_of(object) {
            TYPE_ARRAY_OF_REFS => {
                let data = load(object + ARRAY_DATA);
                children.extend((0..load(object + ARRAY_LENGTH)).map(|i| load(data + i * WORD)));
            }
            TYPE_MAP if values_are_refs(object) && map::len(object) != 0 => {
                map::for_each(object, |_, value| children.push(value));
            }
            other => {
                if let Some(described) = types.of_type_id(other).filter(|found| found.is_class()) {
                    children.extend(
                        written_fields(object, described, options)
                            .into_iter()
                            .filter(|(.., ty)| matches!(ty.repr(), REPR_REF | REPR_BOXED))
                            .map(|(_, word, ..)| word),
                    );
                }
            }
        }
    }
    children
}

impl Walker<'_> {
    /// What the slot holding `word`, and `next` after it, holds: a value
    /// of the type described.
    unsafe fn of_slot(&self, word: usize, next: usize, described: Desc) -> Val {
        let scalar = |repr: u32, described: Desc, bits: usize| match repr {
            REPR_FLOAT => Val::Float(f64::from_bits(bits as u64)),
            REPR_BOOL => Val::Bool(bits & 0xFF != 0),
            _ => match described.kind() {
                KIND_ENUM => Val::Variant(described, bits),
                KIND_VARIANT => Val::Variant(described.of_a(), bits),
                _ => Val::Int(bits as i64),
            },
        };
        match described.repr() {
            REPR_NULL => Val::Null,
            REPR_PAIR if next == 0 => Val::Null,
            REPR_PAIR => {
                let inner = described.not_null();
                scalar(inner.repr(), inner, word)
            }
            repr @ (REPR_INT | REPR_FLOAT | REPR_BOOL) => scalar(repr, described, word),
            // SAFETY: the slot holds a reference, or zero.
            _ => unsafe { self.of_reference(word, Some(described), false) },
        }
    }

    /// What `value` refers to: what its header says, with what its static
    /// type adds. With `json` it is part of a `json` being copied.
    unsafe fn of_reference(&self, value: usize, described: Option<Desc>, json: bool) -> Val {
        if value == 0 {
            return Val::Null;
        }
        let described = described.map(Desc::not_null);
        // The member of the static type a container can be, when only one
        // can.
        let member = |kind: u32| {
            let described = described?;
            if described.kind() == kind {
                return Some(described);
            }
            let mut members = Vec::new();
            described.members(&mut members);
            let mut fitting = members
                .into_iter()
                .map(Desc::not_null)
                .filter(|member| member.kind() == kind);
            let only = fitting.next()?;
            fitting.next().is_none().then_some(only)
        };
        // SAFETY: the caller's contract: a live object, whose header says
        // what it is.
        unsafe {
            match type_of(value) {
                string if string < STRING_TYPES_END => Val::Str(value),
                TYPE_BOX_INT => Val::Int(load(value + BOX_VALUE) as i64),
                TYPE_BOX_FLOAT => Val::Float(f64::from_bits(load(value + BOX_VALUE) as u64)),
                TYPE_BOX_BOOL => Val::Bool(load(value + BOX_VALUE) & 0xFF != 0),
                array @ (TYPE_ARRAY_OF_REFS | TYPE_ARRAY_OF_VALUES) => {
                    let elements = member(KIND_ARRAY).map(Desc::of_a);
                    if elements.is_none()
                        && array == TYPE_ARRAY_OF_VALUES
                        && load(value + ARRAY_LENGTH) != 0
                    {
                        // Its elements are words only the static type
                        // could tell apart.
                        return Self::no_form(json);
                    }
                    Val::Array(value, elements)
                }
                TYPE_MAP => {
                    let described = member(KIND_MAP).map(|map| (map.of_a(), map.of_b()));
                    if described.is_none() && !values_are_refs(value) {
                        return Self::no_form(json);
                    }
                    Val::Map(value, described)
                }
                other => match self.types.of_type_id(other) {
                    Some(found) if found.is_class() && !json => Val::Instance(value, found),
                    Some(found) if found.is_enum() => Val::Variant(found, load(value + BOX_VALUE)),
                    _ => Self::no_form(json),
                },
            }
        }
    }

    fn no_form(json: bool) -> Val {
        if json {
            Val::Null
        } else {
            Val::Refused("value has no json representation", "unserializable")
        }
    }

    /// Where the walk is, in the notation of `DecodeError.path`.
    fn path(&self) -> String {
        let mut path = String::new();
        for frame in &self.frames {
            let Some(index) = frame.next.checked_sub(1) else {
                continue;
            };
            let _ = match &frame.rest {
                Rest::Array { .. } => write!(path, "[{index}]"),
                // SAFETY: the keys of a map being walked are live strings.
                Rest::Map { entries, .. } => match entries.get(index) {
                    Some((key, _)) => {
                        write!(path, "[{:?}]", unsafe { crate::string::text(*key) })
                    }
                    None => Ok(()),
                },
                Rest::Instance { fields } => match fields.get(index) {
                    Some((key, ..)) => write!(path, ".{key}"),
                    None => Ok(()),
                },
            };
        }
        path
    }

    fn refuse(&self, message: impl Into<String>, reason: &'static str) -> Refused {
        Refused {
            message: message.into(),
            path: self.path(),
            reason,
        }
    }

    fn push(
        &mut self,
        object: usize,
        length: usize,
        rest: Rest,
        json: bool,
    ) -> Result<(), Refused> {
        let inside = match &self.open {
            Some(open) => open.contains(&object),
            None => self.frames.iter().any(|frame| frame.object == object),
        };
        if inside {
            return Err(if json {
                self.refuse("cyclic json value: the document contains itself", "cycle")
            } else {
                self.refuse("value contains itself, so it has no JSON form", "cycle")
            });
        }
        if self.open.is_none() && self.frames.len() >= OPEN_SCANNED {
            self.open = Some(self.frames.iter().map(|frame| frame.object).collect());
        }
        if let Some(open) = &mut self.open {
            open.insert(object);
        }
        self.frames.push(Frame {
            object,
            next: 0,
            length,
            rest,
            json,
        });
        Ok(())
    }

    /// Write `value`, or begin it when it is a container. With `json` it
    /// is part of a `json` being copied.
    unsafe fn visit<S: Sink>(
        &mut self,
        value: Val,
        json: bool,
        sink: &mut S,
    ) -> Result<(), Refused> {
        // An instance of a class with a `to_json` of its own is written
        // as what that returned, which is a `json`.
        let (value, json) = match value {
            Val::Instance(object, described) if described.hook(HOOK_TO_JSON).is_some() => {
                match self.overrides {
                    Overrides::Count(met) => {
                        *met += 1;
                        (Val::Null, json)
                    }
                    // SAFETY: the caller of `walk` has `count` values and
                    // as many live `json` values there.
                    Overrides::Splice { held, count, next }
                        if *next < *count && unsafe { load(*held + *next * WORD) } == object =>
                    {
                        // SAFETY: as above.
                        let returned = unsafe { load(*held + (*count + *next) * WORD) };
                        *next += 1;
                        // SAFETY: a live `json`.
                        (unsafe { self.of_reference(returned, None, true) }, true)
                    }
                    Overrides::Splice { .. } => (value, json),
                }
            }
            other => (other, json),
        };
        match value {
            Val::Null => sink.null(),
            Val::Bool(value) => sink.bool(value),
            Val::Int(value) => sink.int(value),
            Val::Float(value) if value.is_finite() || S::KEEPS_NON_FINITE => sink.float(value),
            Val::Float(value) => {
                return Err(self.refuse(
                    format!("{value} has no JSON form: a JSON number is finite"),
                    "non_finite",
                ));
            }
            // SAFETY: a live string.
            Val::Str(string) => sink.string(unsafe { crate::string::text(string) }, string),
            Val::Variant(described, index) => match described.variant(index) {
                // A `json` holding a variant reads it as its declared name.
                Some(variant) if json => sink.string(variant.name, 0),
                Some(variant) if variant.shown(self.options.aliased) => {
                    sink.string(variant.key(self.options.aliased), 0);
                }
                Some(variant) => {
                    return Err(self.refuse(
                        format!(
                            "enum variant `{}.{}` is `@skip`, so JSON has no name for it",
                            described.full_name(),
                            variant.name
                        ),
                        "enum",
                    ));
                }
                None => sink.string("", 0),
            },
            Val::Array(array, elements) => {
                // SAFETY: a live array.
                let length = unsafe { load(array + ARRAY_LENGTH) };
                self.push(array, length, Rest::Array { elements }, json)?;
                sink.begin_array();
            }
            Val::Map(object, described) => {
                // SAFETY: a live map, which nothing changes during the
                // walk.
                let string_keys = unsafe {
                    string_keyed(object) && described.is_none_or(|(keys, _)| keys.is_string_key())
                };
                if !string_keys {
                    return Err(self.refuse(STRING_MAP_REQUIRED, "map_key"));
                }
                let mut entries = Vec::new();
                // SAFETY: as above.
                unsafe {
                    if map::len(object) != 0 {
                        map::for_each(object, |key, value| entries.push((key, value)));
                    }
                }
                let values = described.map(|(_, values)| values);
                self.push(object, entries.len(), Rest::Map { entries, values }, json)?;
                sink.begin_object();
            }
            Val::Instance(object, described) => {
                // SAFETY: a live instance of the class described.
                let fields = unsafe { written_fields(object, described, self.options) };
                self.push(object, fields.len(), Rest::Instance { fields }, false)?;
                sink.begin_object();
            }
            Val::Refused(message, reason) => return Err(self.refuse(message, reason)),
        }
        Ok(())
    }
}

/// JSON text, with the interpreter's layout.
pub(super) struct TextSink<'i> {
    out: String,
    indent: &'i str,
    level: usize,
}

impl<'i> TextSink<'i> {
    pub(super) fn new(indent: &'i str) -> Self {
        Self {
            out: String::new(),
            indent,
            level: 0,
        }
    }

    pub(super) fn into_string(self) -> String {
        self.out
    }

    fn newline(&mut self) {
        self.out.push('\n');
        for _ in 0..self.level {
            self.out.push_str(self.indent);
        }
    }

    fn separator(&mut self, first: bool) {
        if !first {
            self.out.push(',');
        }
        if !self.indent.is_empty() {
            self.newline();
        }
    }

    fn close(&mut self, empty: bool, bracket: char) {
        self.level -= 1;
        if !self.indent.is_empty() && !empty {
            self.newline();
        }
        self.out.push(bracket);
    }

    fn escaped(&mut self, text: &str) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        self.out.push('"');
        let mut start = 0;
        for (i, byte) in text.bytes().enumerate() {
            let short = match byte {
                b'"' => '"',
                b'\\' => '\\',
                0x08 => 'b',
                0x0c => 'f',
                b'\n' => 'n',
                b'\r' => 'r',
                b'\t' => 't',
                0x00..=0x1f => 'u',
                _ => continue,
            };
            self.out.push_str(&text[start..i]);
            self.out.push('\\');
            self.out.push(short);
            if short == 'u' {
                self.out.push_str("00");
                self.out.push(char::from(HEX[usize::from(byte >> 4)]));
                self.out.push(char::from(HEX[usize::from(byte & 0xf)]));
            }
            start = i + 1;
        }
        self.out.push_str(&text[start..]);
        self.out.push('"');
    }
}

impl Sink for TextSink<'_> {
    const KEEPS_NON_FINITE: bool = false;

    fn null(&mut self) {
        self.out.push_str("null");
    }

    fn bool(&mut self, value: bool) {
        self.out.push_str(if value { "true" } else { "false" });
    }

    fn int(&mut self, value: i64) {
        let _ = write!(self.out, "{value}");
    }

    fn float(&mut self, value: f64) {
        self.out.push_str(zmij::Buffer::new().format_finite(value));
    }

    fn string(&mut self, text: &str, _: usize) {
        self.escaped(text);
    }

    fn begin_array(&mut self) {
        self.level += 1;
        self.out.push('[');
    }

    fn item(&mut self, first: bool) {
        self.separator(first);
    }

    fn end_array(&mut self, empty: bool) {
        self.close(empty, ']');
    }

    fn begin_object(&mut self) {
        self.level += 1;
        self.out.push('{');
    }

    fn key(&mut self, first: bool, text: &str, _: usize) {
        self.separator(first);
        self.escaped(text);
        self.out
            .push_str(if self.indent.is_empty() { ":" } else { ": " });
    }

    fn end_object(&mut self, empty: bool) {
        self.close(empty, '}');
    }
}

/// A `json` value, built in an [`Out`].
pub(super) struct JsonSink {
    out: Out,
    /// The values of the containers being built, innermost last: for an
    /// object, each key before its value.
    items: Vec<usize>,
    /// Where in `items` each container being built starts.
    starts: Vec<usize>,
}

impl JsonSink {
    pub(super) fn new(out: Out) -> Self {
        Self {
            out,
            items: Vec::new(),
            starts: Vec::new(),
        }
    }

    /// The value built, zero while counting, and the `Out` as the build
    /// left it.
    pub(super) fn finish(mut self) -> (usize, Out) {
        (self.items.pop().unwrap_or(0), self.out)
    }
}

impl Sink for JsonSink {
    const KEEPS_NON_FINITE: bool = true;

    fn null(&mut self) {
        self.items.push(0);
    }

    fn bool(&mut self, value: bool) {
        let boxed = self.out.boxed(TYPE_BOX_BOOL, usize::from(value));
        self.items.push(boxed);
    }

    fn int(&mut self, value: i64) {
        let boxed = self.out.boxed(TYPE_BOX_INT, value as usize);
        self.items.push(boxed);
    }

    fn float(&mut self, value: f64) {
        let boxed = self.out.boxed(TYPE_BOX_FLOAT, value.to_bits() as usize);
        self.items.push(boxed);
    }

    fn string(&mut self, text: &str, object: usize) {
        // A string is never changed: the copy holds the same one.
        let string = if object == 0 {
            self.out.string(text)
        } else if self.out.is_building() {
            object
        } else {
            0
        };
        self.items.push(string);
    }

    fn begin_array(&mut self) {
        self.starts.push(self.items.len());
    }

    fn item(&mut self, _: bool) {}

    fn end_array(&mut self, _: bool) {
        let start = self.starts.pop().unwrap_or(0);
        let array = self
            .out
            .array(self.items.len() - start, true, &self.items[start..]);
        self.items.truncate(start);
        self.items.push(array);
    }

    fn begin_object(&mut self) {
        self.starts.push(self.items.len());
    }

    fn key(&mut self, _: bool, text: &str, object: usize) {
        // A key holds its own text.
        // SAFETY: `object` is zero or a live string.
        let flat = object != 0 && unsafe { type_of(object) } == crate::layout::TYPE_STRING;
        let key = if flat && self.out.is_building() {
            object
        } else if flat {
            0
        } else {
            self.out.string(text)
        };
        self.items.push(key);
    }

    fn end_object(&mut self, _: bool) {
        let start = self.starts.pop().unwrap_or(0);
        let entries: Vec<(usize, usize)> = self.items[start..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[key, value]| (*key, *value))
            .collect();
        let map = self.out.map(entries.len(), true, &entries);
        self.items.truncate(start);
        self.items.push(map);
    }
}
