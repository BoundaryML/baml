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
//! A value is found by the tag of the slot that holds it and, when it is
//! an object, by its header. Its static type, which a description gives
//! (see [`crate::json_layout`]), adds what those do not say: which enum a
//! variant held in place is of, and the names a class is written under.

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
    heap::{Value, load},
    layout::{
        BOX_VALUE, STRING_TAGS_END, TAG_ARRAY, TAG_BIGINT, TAG_BOOL, TAG_FLOAT, TAG_INT, TAG_MAP,
        TAG_NULL, TYPE_ID_MASK, WELL_KNOWN_JSON_DECODE_ERROR, WELL_KNOWN_JSON_PARSE_ERROR,
        WELL_KNOWN_JSON_PATH_ERROR, WELL_KNOWN_JSON_SERIALIZATION_ERROR,
    },
    map,
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

/// What a `json` value is, which its tag says: a scalar, or what its
/// object is.
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
    /// `value` is a value as a slot holds one: `null`, a scalar, or a
    /// pointer to a live object.
    unsafe fn of(types: &Types, value: Value) -> Self {
        let object = value.payload;
        // SAFETY: the caller's contract: an enum's box is a header and the
        // variant's position.
        unsafe {
            match value.id() {
                TAG_NULL => Self::Null,
                TAG_BOOL => Self::Bool(object & 0xFF != 0),
                TAG_INT => Self::Int(object as i64),
                TAG_FLOAT => Self::Float(f64::from_bits(object as u64)),
                string if string < STRING_TAGS_END => Self::String(object),
                TAG_BIGINT => Self::Bigint(object),
                TAG_ARRAY => Self::Array(object),
                TAG_MAP => Self::Object(object),
                other => match types.of_type_id(other) {
                    Some(described) if described.is_enum() => described
                        .variant(load(object + BOX_VALUE))
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

/// Whether the map at `map` has string keys, as far as its keys say: an
/// empty one has whatever keys are asked of it. Every key of a map is of
/// one kind, so the first says it.
///
/// # Safety
///
/// `map` is a live map.
pub(super) unsafe fn string_keyed(map: usize) -> bool {
    let mut first = None;
    // SAFETY: the caller's contract.
    unsafe { map::for_each(map, |key, _| first = first.or(Some(key))) };
    first.is_none_or(|key| key.is_pointer() && key.id() < STRING_TAGS_END)
}

/// The value under `key` in the string-keyed map at `map`.
///
/// # Safety
///
/// `map` is a live map of string keys.
unsafe fn object_get(map: usize, key: &str) -> Option<Value> {
    let mut found = None;
    // SAFETY: the caller's contract. The keys are compared as text: a
    // probe wants a string object, and making one would allocate.
    unsafe {
        if map::len(map) == 0 {
            return None;
        }
        map::for_each(map, |held, value| {
            if found.is_none() && crate::string::text(held.payload) == key {
                found = Some(value);
            }
        });
    }
    found
}

impl NativeBamlJson for Natives {
    // Reading text and decoding recurse as deep as the value nests, which
    // is more than a task's stack keeps for the runtime.
    fn parse<'s>(scope: &mut Scope<'s>, s: Root<'s, Str>) -> Result<Value, Thrown<'s>> {
        scope.with_room(|scope| parse_rooted(scope, s))
    }

    async fn _decode<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 1],
        s: Root<'s, Str>,
        aliased: bool,
    ) -> Result<Value, Thrown<'s>> {
        let json = scope.with_room(|scope| parse_rooted(scope, s))?;
        // SAFETY: a `json` value just built.
        let json = unsafe { scope.elem(json) };
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
        let indent = match encode::indent_text(indent.value(scope)) {
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
                    value.value(scope),
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
    ) -> Result<Value, Thrown<'s>> {
        to_json_value(scope, value_type, value, aliased, omit_nulls).await
    }

    async fn _to<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 1],
        j: Elem<'s>,
        aliased: bool,
    ) -> Result<Value, Thrown<'s>> {
        decode::to(scope, types[0], j, aliased, false).await
    }

    fn _path<'s>(
        scope: &mut Scope<'s>,
        j: Elem<'s>,
        selector: Root<'s, Str>,
    ) -> Result<Value, Thrown<'s>> {
        let types = Types::of(scope);
        // SAFETY: a `json` value, and nothing is allocated while it is
        // walked.
        let found = unsafe { text::select(&types, j.value(scope), selector.get(scope).as_str()) };
        found.map_err(|message| {
            let selector = selector.get(scope).as_str().to_owned();
            path_error(scope, &message, &selector)
        })
    }

    fn field(j: Value, key: StrRef<'_>) -> Value {
        let map = j.payload;
        // SAFETY: a value whose tag is a map's refers to a live one, and
        // a map found in a `json` value has string keys.
        unsafe {
            if j.id() != TAG_MAP || map::len(map) == 0 || !string_keyed(map) {
                return Value::NULL;
            }
            map::get(map, Value::reference(super::View::raw(key)))
                .map_or(Value::NULL, |slot| Value::load(slot))
        }
    }
}

/// The values under one being written whose class has a `to_json` of its
/// own, and after them what the `to_json` of each returned: `count`
/// elements and `count` more.
struct Held<'s> {
    array: Root<'s, Arr<Slot>>,
    count: usize,
}

impl Held<'_> {
    /// How the walk writes the value now. The address is good until the
    /// next allocation.
    fn splice(&self, scope: &Scope<'_>) -> encode::Overrides {
        encode::Overrides::Splice {
            held: self.array.raw(scope),
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
    unsafe { encode::overriding(&types, value.value(scope), options, |_| count += 1) };
    if count == 0 {
        return Ok(None);
    }
    let reservation = scope.reserve(array_size(2 * count));
    let array = reservation.array::<Slot>(2 * count);
    let mut found = 0;
    // SAFETY: as above: nothing moves while the reservation is open, and
    // each object handed out is a live instance.
    unsafe {
        let value = value.value(reservation.scope);
        encode::overriding(&types, value, options, |object| {
            array.set(found, Value::reference(object));
            found += 1;
        });
    }
    let kept = array.keep();
    let array = reservation.finish(kept);
    for index in 0..count {
        let object = array.get(scope).get(index).unwrap_or(Value::NULL);
        // SAFETY: a live instance, found to have one.
        let Some(to_json) = (unsafe { encode::own_to_json(&types, object.payload) }) else {
            continue;
        };
        // SAFETY: as above.
        let instance = unsafe { scope.elem(object) };
        let returned = scope.call_baml(to_json, &[instance]).await?;
        // SAFETY: the array has `2 * count` elements, and neither slot is
        // used again.
        unsafe {
            Value::store(
                crate::array::element(array.raw(scope), count + index),
                returned.value(scope),
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
) -> Result<Value, Thrown<'s>> {
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
                value.value(scope),
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
            Ok(built.map_or(Value::NULL, |(root, _)| root))
        }
        Err(refused) => Err(serialization_error(scope, &refused)),
    }
}

/// Parse the text of `s` into a `json` value.
pub(super) fn parse_rooted<'s>(
    scope: &mut Scope<'s>,
    s: Root<'s, Str>,
) -> Result<Value, Thrown<'s>> {
    let counted = text::parse(s.get(scope).as_str(), Out::counting());
    match counted {
        Ok((_, out)) => {
            let bytes = out.bytes();
            let reservation = scope.reserve(bytes);
            let base = reservation.take(bytes);
            // The text may have moved when the room was made.
            let text = reservation.get(s).as_str();
            let built = text::parse(text, Out::building(base, bytes));
            Ok(built.map_or(Value::NULL, |(root, _)| root))
        }
        Err(stop) => {
            let message = text::syntax_message(s.get(scope).as_str(), &stop);
            Err(parse_error(scope, &message))
        }
    }
}
