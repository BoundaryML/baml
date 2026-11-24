//! `baml.json`: JSON text and `json` values, written from and read into
//! values of any type the compiler describes.
//!
//! The collector moves objects, and these natives build trees. They do it
//! in two passes over the same code: the first only adds up the bytes the
//! tree takes and finds every reason to refuse; the room is then reserved
//! at once, which is the one point a collection can happen; the second
//! pass builds the tree in that room with plain stores, where nothing
//! moves and nothing can fail. No pass holds an address across the
//! reservation: the roots are read again after it.
//!
//! A value is found either by its static type, which a description gives
//! (see [`crate::json_layout`]), or by its header when the static type
//! says nothing.

#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "words of the heap are read as the types their descriptions say"
)]

pub(super) mod decode;
mod encode;
mod out;
mod text;

use out::Out;

use super::{
    Arr, Elem, NativeBamlJson, Natives, Root, Scope, Slot, Str, StrRef, Thrown, TypeId, array_size,
};
use crate::{
    describe::Types,
    heap::{load, store},
    layout::{
        ARRAY_DATA, ARRAY_LENGTH, BOX_VALUE, MAP_ENTRIES, STRING_TYPES_END, TYPE_ARRAY_OF_REFS,
        TYPE_ARRAY_OF_VALUES, TYPE_BIGINT, TYPE_BOX_BOOL, TYPE_BOX_FLOAT, TYPE_BOX_INT,
        TYPE_ENTRIES_KEY_REF, TYPE_ENTRIES_REFS, TYPE_ENTRIES_VALUE_REF, TYPE_ID_MASK, TYPE_MAP,
        WELL_KNOWN_JSON_DECODE_ERROR, WELL_KNOWN_JSON_PARSE_ERROR, WELL_KNOWN_JSON_PATH_ERROR,
        WELL_KNOWN_JSON_SERIALIZATION_ERROR,
    },
    map::{self, KeyKind},
};

/// How deep arrays and objects nest in text, and how deep a decode follows
/// a value: the interpreter's limits.
const MAX_DEPTH: usize = 128;

const STRING_MAP_REQUIRED: &str = "JSON requires maps with string keys";

/// Why a native refuses: the message, where in the value, and for a
/// `SerializationError` the reason's tag.
struct Refused {
    message: String,
    path: String,
    reason: &'static str,
}

fn parse_error<'s>(scope: &mut Scope<'s>, message: &str) -> Thrown<'s> {
    scope.error_of(WELL_KNOWN_JSON_PARSE_ERROR, &[message])
}

fn decode_error<'s>(scope: &mut Scope<'s>, refused: &Refused) -> Thrown<'s> {
    scope.error_of(
        WELL_KNOWN_JSON_DECODE_ERROR,
        &[&refused.message, &refused.path],
    )
}

fn serialization_error<'s>(scope: &mut Scope<'s>, refused: &Refused) -> Thrown<'s> {
    scope.error_of(
        WELL_KNOWN_JSON_SERIALIZATION_ERROR,
        &[&refused.message, &refused.path, refused.reason],
    )
}

fn path_error<'s>(scope: &mut Scope<'s>, message: &str, selector: &str) -> Thrown<'s> {
    scope.error_of(WELL_KNOWN_JSON_PATH_ERROR, &[message, selector])
}

/// The type id in the header of the object at `object`.
///
/// # Safety
///
/// `object` is a live object.
pub(super) unsafe fn type_of(object: usize) -> usize {
    // SAFETY: the caller's contract.
    unsafe { load(object) & TYPE_ID_MASK }
}

/// What a `json` value is, read from its header.
#[derive(Clone, Copy)]
enum Node {
    Null,
    Bool(bool),
    Int(i64),
    /// A `bigint` object: an integral number past an `int`.
    Bigint(usize),
    Float(f64),
    /// A string object.
    String(usize),
    /// An enum variant, which reads as its name.
    Variant(&'static str),
    Array(usize),
    Object(usize),
}

impl Node {
    /// # Safety
    ///
    /// `value` is zero or a live object.
    unsafe fn of(types: &Types, value: usize) -> Self {
        if value == 0 {
            return Self::Null;
        }
        // SAFETY: the caller's contract: a box is a header and a value.
        unsafe {
            match type_of(value) {
                string if string < STRING_TYPES_END => Self::String(value),
                TYPE_BOX_INT => Self::Int(load(value + BOX_VALUE) as i64),
                TYPE_BIGINT => Self::Bigint(value),
                TYPE_BOX_FLOAT => Self::Float(f64::from_bits(load(value + BOX_VALUE) as u64)),
                TYPE_BOX_BOOL => Self::Bool(load(value + BOX_VALUE) & 0xFF != 0),
                TYPE_ARRAY_OF_REFS => Self::Array(value),
                // Only the empty one can be a `json` array: it has no
                // elements to read as references.
                TYPE_ARRAY_OF_VALUES if load(value + ARRAY_LENGTH) == 0 => Self::Array(value),
                TYPE_MAP if values_are_refs(value) => Self::Object(value),
                other => match types.of_type_id(other) {
                    Some(described) if described.is_enum() => described
                        .variant(load(value + BOX_VALUE))
                        .map_or(Self::Null, |variant| Self::Variant(variant.name)),
                    // Anything else in a `json` value reads as `null`.
                    _ => Self::Null,
                },
            }
        }
    }

    fn is_number(self) -> bool {
        matches!(self, Self::Int(_) | Self::Bigint(_) | Self::Float(_))
    }

    fn text(self) -> Option<&'static str> {
        match self {
            // SAFETY: a live string, read before anything is allocated.
            Self::String(string) => Some(unsafe { crate::string::text(string) }),
            Self::Variant(name) => Some(name),
            _ => None,
        }
    }
}

/// Whether the values of the map at `map` are references, as far as its
/// entries say: an empty one has whatever values are asked of it.
///
/// # Safety
///
/// `map` is a live map.
pub(super) unsafe fn values_are_refs(map: usize) -> bool {
    // SAFETY: the caller's contract.
    unsafe {
        map::len(map) == 0
            || matches!(
                type_of(load(map + MAP_ENTRIES)),
                TYPE_ENTRIES_VALUE_REF | TYPE_ENTRIES_REFS
            )
    }
}

/// Whether the map at `map` has string keys, as far as its entries say: an
/// empty one has whatever keys are asked of it.
///
/// # Safety
///
/// `map` is a live map.
pub(super) unsafe fn string_keyed(map: usize) -> bool {
    // SAFETY: the caller's contract.
    unsafe {
        map::len(map) == 0
            || matches!(
                type_of(load(map + MAP_ENTRIES)),
                TYPE_ENTRIES_KEY_REF | TYPE_ENTRIES_REFS
            )
    }
}

/// The value under `key` in the string-keyed map at `map`.
///
/// # Safety
///
/// `map` is a live map of string keys.
unsafe fn object_get(map: usize, key: &str) -> Option<usize> {
    let mut found = None;
    // SAFETY: the caller's contract. The keys are compared as text: a
    // probe wants a string object, and making one would allocate.
    unsafe {
        if map::len(map) == 0 {
            return None;
        }
        map::for_each(map, |held, value| {
            if found.is_none() && crate::string::text(held) == key {
                found = Some(value);
            }
        });
    }
    found
}

/// The value under the string object `key`, by the map's own probe.
///
/// # Safety
///
/// `map` is a live map of string keys and `key` a live string.
unsafe fn object_get_keyed(map: usize, key: usize) -> Option<usize> {
    // SAFETY: the caller's contract.
    unsafe {
        if map::len(map) == 0 {
            return None;
        }
        map::get(map, KeyKind::Str, key).map(|slot| load(slot))
    }
}

impl NativeBamlJson for Natives {
    // Reading text and decoding recurse as deep as the value nests, which
    // is more than a task's stack keeps for the runtime.
    fn parse<'s>(scope: &mut Scope<'s>, s: Root<'s, Str>) -> Result<usize, Thrown<'s>> {
        scope.with_room(|scope| parse_rooted(scope, s))
    }

    async fn _decode<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 1],
        s: Root<'s, Str>,
        aliased: bool,
    ) -> Result<usize, Thrown<'s>> {
        let json = scope.with_room(|scope| parse_rooted(scope, s))?;
        // SAFETY: a `json` value just built.
        let json = unsafe { scope.elem_raw(json, TypeId(0)) };
        decode::to(scope, types[0], json, aliased, false).await
    }

    async fn _stringify<'s>(
        scope: &mut Scope<'s>,
        value_type: TypeId,
        value: Elem<'s>,
        aliased: bool,
        indent: Elem<'s>,
        omit_nulls: bool,
    ) -> Result<Root<'s, Str>, Thrown<'s>> {
        let options = encode::Options {
            aliased,
            omit_nulls,
        };
        let indent = match encode::indent_text(indent.get(scope)) {
            Ok(indent) => indent,
            Err(refused) => return Err(serialization_error(scope, &refused)),
        };
        let write = move |scope: &Scope<'s>, overrides: &mut encode::Overrides| {
            let types = Types::of(scope);
            let mut sink = encode::TextSink::new(&indent);
            // SAFETY: the value compiled code passed, of the static type
            // it described; nothing is allocated while it is walked.
            unsafe {
                encode::walk(
                    &types,
                    value.get(scope),
                    types.described(value_type),
                    options,
                    false,
                    overrides,
                    &mut sink,
                )?;
            }
            Ok(sink.into_string())
        };
        let mut met = encode::Overrides::Count(0);
        let mut written = write(scope, &mut met);
        // Nothing to call when the walk met no `to_json` and went to the
        // end. One that was refused stopped before it had met them all.
        if !(written.is_ok() && matches!(met, encode::Overrides::Count(0)))
            && let Some(held) = to_json_of_each(scope, value, options).await?
        {
            let mut spliced = held.splice(scope);
            written = write(scope, &mut spliced);
        }
        match written {
            Ok(text) => Ok(scope.alloc_str(&text)),
            Err(refused) => Err(serialization_error(scope, &refused)),
        }
    }

    async fn _from<'s>(
        scope: &mut Scope<'s>,
        value_type: TypeId,
        value: Elem<'s>,
        aliased: bool,
        omit_nulls: bool,
    ) -> Result<usize, Thrown<'s>> {
        to_json_value(scope, value_type, value, aliased, omit_nulls).await
    }

    async fn _to<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 1],
        j: Elem<'s>,
        aliased: bool,
    ) -> Result<usize, Thrown<'s>> {
        decode::to(scope, types[0], j, aliased, false).await
    }

    fn _path<'s>(
        scope: &mut Scope<'s>,
        j: Elem<'s>,
        selector: Root<'s, Str>,
    ) -> Result<usize, Thrown<'s>> {
        let types = Types::of(scope);
        // SAFETY: a `json` value, and nothing is allocated while it is
        // walked.
        let found = unsafe { text::select(&types, j.get(scope), selector.get(scope).as_str()) };
        found.map_err(|message| {
            let selector = selector.get(scope).as_str().to_owned();
            path_error(scope, &message, &selector)
        })
    }

    fn field(j: usize, key: StrRef<'_>) -> usize {
        // SAFETY: `j` is zero or a live object, and a map found in a
        // `json` value has string keys.
        unsafe {
            if j == 0 || type_of(j) != TYPE_MAP || !string_keyed(j) {
                return 0;
            }
            object_get_keyed(j, super::View::raw(key)).unwrap_or(0)
        }
    }
}

/// The values under one being written whose class has a `to_json` of its
/// own, and after them what the `to_json` of each returned: `count` words
/// and `count` more.
struct Held<'s> {
    array: Root<'s, Arr<Slot>>,
    count: usize,
}

impl Held<'_> {
    /// How the walk writes the value now. The addresses are good until
    /// the next allocation.
    fn splice(&self, scope: &Scope<'_>) -> encode::Overrides {
        encode::Overrides::Splice {
            // SAFETY: a live array of `2 * count` elements.
            held: unsafe { load(self.array.raw(scope) + ARRAY_DATA) },
            count: self.count,
            next: 0,
        }
    }
}

/// Call the `to_json` of every value under `value` whose class has one of
/// its own, in the order the walk meets them. `None` when there is none.
///
/// Every `to_json` runs before the caller's next walk can refuse a value,
/// as in the interpreter.
async fn to_json_of_each<'s>(
    scope: &mut Scope<'s>,
    value: Elem<'s>,
    options: encode::Options,
) -> Result<Option<Held<'s>>, Thrown<'s>> {
    let types = Types::of(scope);
    let mut count = 0;
    // SAFETY: a value compiled code passed, and nothing is allocated.
    unsafe { encode::overriding(&types, value.get(scope), options, |_| count += 1) };
    if count == 0 {
        return Ok(None);
    }
    let reservation = scope.reserve(array_size(2 * count));
    let array = reservation.slots(2 * count, true);
    let mut found = 0;
    // SAFETY: as above: nothing moves while the reservation is open.
    unsafe {
        encode::overriding(&types, value.get(reservation.scope), options, |object| {
            array.set(found, object);
            found += 1;
        });
    }
    let kept = array.keep();
    let array = reservation.finish(kept);
    for index in 0..count {
        let object = array.get(scope).get(index).unwrap_or(0);
        // SAFETY: a live instance, found to have one.
        let Some(to_json) = (unsafe { encode::own_to_json(&types, object) }) else {
            continue;
        };
        // SAFETY: as above.
        let instance = unsafe { scope.elem_raw(object, TypeId(0)) };
        let returned = scope.call_baml(to_json, &[instance]).await?;
        // SAFETY: the array has `2 * count` elements, each a reference,
        // and neither slot is used again.
        unsafe {
            let data = load(array.raw(scope) + ARRAY_DATA);
            store(
                data + (count + index) * crate::layout::WORD,
                returned.get(scope),
            );
            scope.release_last(2);
        }
    }
    Ok(Some(Held { array, count }))
}

/// `value`, of the type `value_type` describes, as a `json` value: what
/// `baml.json.from` returns, and with declared names and every field what
/// a `to_json` nothing overrides does.
pub(super) async fn to_json_value<'s>(
    scope: &mut Scope<'s>,
    value_type: TypeId,
    value: Elem<'s>,
    aliased: bool,
    omit_nulls: bool,
) -> Result<usize, Thrown<'s>> {
    let options = encode::Options {
        aliased,
        omit_nulls,
    };
    let build = move |scope: &Scope<'s>, out: Out, overrides: &mut encode::Overrides| {
        let types = Types::of(scope);
        let mut sink = encode::JsonSink::new(out);
        // SAFETY: as in `_stringify`.
        unsafe {
            encode::walk(
                &types,
                value.get(scope),
                types.described(value_type),
                options,
                false,
                overrides,
                &mut sink,
            )?;
        }
        Ok(sink.finish())
    };
    let mut met = encode::Overrides::Count(0);
    let mut counted = build(scope, Out::counting(), &mut met);
    let mut held = None;
    if !(counted.is_ok() && matches!(met, encode::Overrides::Count(0))) {
        held = to_json_of_each(scope, value, options).await?;
        if let Some(held) = &held {
            let mut spliced = held.splice(scope);
            counted = build(scope, Out::counting(), &mut spliced);
        }
    }
    match counted {
        Ok((_, out)) => {
            let bytes = out.bytes();
            let reservation = scope.reserve(bytes);
            let base = reservation.take(bytes);
            // The values may have moved when the room was made.
            let mut overrides = held.as_ref().map_or(encode::Overrides::Count(0), |held| {
                held.splice(reservation.scope)
            });
            let built: Result<_, Refused> = build(
                reservation.scope,
                Out::building(base, bytes),
                &mut overrides,
            );
            Ok(built.map_or(0, |(root, _)| root))
        }
        Err(refused) => Err(serialization_error(scope, &refused)),
    }
}

/// Parse the text of `s` into a `json` value.
pub(super) fn parse_rooted<'s>(
    scope: &mut Scope<'s>,
    s: Root<'s, Str>,
) -> Result<usize, Thrown<'s>> {
    let counted = text::parse(s.get(scope).as_str(), Out::counting());
    match counted {
        Ok((_, out)) => {
            let bytes = out.bytes();
            let reservation = scope.reserve(bytes);
            let base = reservation.take(bytes);
            // The text may have moved when the room was made.
            let text = reservation.get(s).as_str();
            let built = text::parse(text, Out::building(base, bytes));
            Ok(built.map_or(0, |(root, _)| root))
        }
        Err(stop) => {
            let message = text::syntax_message(s.get(scope).as_str(), &stop);
            Err(parse_error(scope, &message))
        }
    }
}
