//! Reading a `json` value into a value of a described type, by the
//! interpreter's rules: a shape check, not a coercion.
//!
//! A class with a `from_json` of its own is built by that, from the `json`
//! in its place. The decoder calls nothing. It runs up to three times over
//! the same value, and the native calls each `from_json` between two runs
//! ([`to`]), so the recursion here never stops halfway:
//!
//! - A first run counts the bytes of what it builds and notes, in the
//!   order it meets them, each `json` a `from_json` is to read. It stops
//!   at the first value that does not fit, as the interpreter does, having
//!   noted only what comes before.
//! - When there are any, a run like it leaves those values in an array,
//!   which is how the native holds them while each `from_json` runs.
//! - The last run builds, taking what each `from_json` returned in the
//!   same order.
//!
//! A member of a union is chosen by shape, with every class read by its
//! fields: a `from_json` that throws ends the decode, so no member can be
//! tried through one. The member chosen is then read again as any other
//! value, when a `from_json` was met inside it.

use std::fmt::Write as _;

use super::{
    MAX_DEPTH, Node, Refused, STRING_MAP_REQUIRED, decode_error, encode, object_get, out::Out,
    string_keyed,
};
use crate::{
    array,
    describe::{Desc, Types},
    heap::{Value, load},
    json_layout::{
        HOOK_FROM_JSON, KIND_ARRAY, KIND_BOOL, KIND_BOOL_IS, KIND_CLASS, KIND_ENUM, KIND_FLOAT,
        KIND_INT, KIND_INT_IS, KIND_MAP, KIND_NULL, KIND_OPTIONAL, KIND_STRING, KIND_STRING_IS,
        KIND_UNION, KIND_VARIANT,
    },
    layout::{ARRAY_LENGTH, VALUE},
    map,
    native::{
        Elem, Scope, Slot, Thrown, TypeRef, array_size,
        build::{Got, place},
        drive::BamlFn,
    },
};

/// Why a value does not fit.
struct Misfit {
    message: String,
    path: String,
    /// The value nests too deep to follow: no other member is tried.
    deep: bool,
}

/// The JSON kind of a value, which says what members of a union it can be.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Null,
    Bool,
    Number,
    String,
    Array,
    Object,
}

/// How well a member of a union suits a value.
#[derive(Clone, Copy)]
enum Fit {
    /// By what it is: the higher, the more specific.
    Rank(u64),
    /// By how many keys of the object it uses.
    Keys,
}

/// What the decoder does at a class with a `from_json` of its own.
enum Provided {
    /// Note its `from_json`, and when `into` is not zero leave the `json`
    /// it is to read there, one slot after the one before.
    Note { entries: Vec<BamlFn>, into: usize },
    /// Take what its `from_json` returned: the next of the slots at this
    /// address.
    Take(usize),
}

struct Decoder<'a> {
    types: &'a Types,
    aliased: bool,
    out: Out,
    /// The member each union met decoded as, in the order they were met:
    /// written by the counting pass and followed by the building one.
    choices: Vec<usize>,
    next_choice: usize,
    path: String,
    depth: usize,
    /// How many unions' members are being tried. The interpreter words
    /// two misfits differently there.
    trying: usize,
    provided: Provided,
    /// Whether the value being decoded next is read by its fields whatever
    /// `from_json` its class has: the root of a default `from_json`.
    by_fields: bool,
    /// Whether a class with a `from_json` of its own was read by its
    /// fields, while a member of a union was tried.
    tried_provided: bool,
}

/// What a decode is asked for: the `json` value to read, the type a call
/// named to read it as, and how.
#[derive(Clone, Copy)]
struct Asked<'s> {
    ty: TypeRef,
    json: Elem<'s>,
    aliased: bool,
    by_fields: bool,
}

/// What a run that went to the end made: the value, the room it took, and
/// the member each union decoded as.
type Made = (Value, Out, Vec<usize>);

/// One run of the decoder: what it made, and what it noted or has left to
/// take of what each `from_json` is for.
fn run(
    scope: &Scope<'_>,
    asked: Asked<'_>,
    out: Out,
    choices: Vec<usize>,
    provided: Provided,
) -> (Result<Made, Misfit>, Provided) {
    let Asked {
        ty,
        json,
        aliased,
        by_fields,
    } = asked;
    let types = Types::of(scope);
    let mut decoder = Decoder {
        types: &types,
        aliased,
        out,
        choices,
        next_choice: 0,
        path: String::new(),
        depth: 0,
        trying: 0,
        provided,
        by_fields,
        tried_provided: false,
    };
    // SAFETY: a `json` value, read before anything is allocated or in
    // reserved room.
    let value = unsafe {
        match types.described(ty) {
            Some(described) => decoder.decode(json.value(scope), described).map(place),
            // A type the call does not describe is `unknown`.
            None => decoder.copy(json.value(scope)).map(place),
        }
    };
    (
        value.map(|value| (value, decoder.out, decoder.choices)),
        decoder.provided,
    )
}

/// Decode the `json` value `json` as the type a call named by `ty`, and
/// return it as a value. With `by_fields` the
/// value itself is read by its fields whatever `from_json` its class has.
///
/// Each `from_json` is called here, between runs of the decoder. A run
/// recurses as deep as the value nests, so it is made where there is room
/// for that, which is nowhere a call of compiled code can be made from.
pub(in crate::native) async fn to<'s>(
    scope: &mut Scope<'s>,
    ty: TypeRef,
    json: Elem<'s>,
    aliased: bool,
    by_fields: bool,
) -> Result<Value, Thrown<'s>> {
    let asked = Asked {
        ty,
        json,
        aliased,
        by_fields,
    };
    let note = |into| Provided::Note {
        entries: Vec::new(),
        into,
    };
    let (counted, provided) =
        scope.with_room(|scope| run(scope, asked, Out::counting(), Vec::new(), note(0)));
    let Provided::Note { entries, .. } = provided else {
        unreachable!("a run hands back what it was handed");
    };
    // What each `from_json` is to read, then what each returned.
    let mut held = None;
    if !entries.is_empty() {
        let count = entries.len();
        let array = scope.with_room(|scope| {
            let reservation = scope.reserve(array_size(2 * count));
            let array = reservation.array::<Slot>(2 * count).keep();
            let array = reservation.finish(array);
            // Nothing is allocated between the array and the run that
            // fills it, which notes the same values in the same order and
            // ends as the first did.
            // SAFETY: a live array of `2 * count` elements.
            let into = unsafe { array::element(array.raw(scope), 0) };
            drop(run(scope, asked, Out::counting(), Vec::new(), note(into)));
            array
        });
        for (index, from_json) in entries.into_iter().enumerate() {
            // SAFETY: the array's elements are `json` values and what was
            // returned for them, and neither slot is used again.
            unsafe {
                let read = Value::load(array::element(array.raw(scope), index));
                let read = scope.elem(read);
                let made = scope.call_baml(from_json, &[read]).await?;
                Value::store(
                    array::element(array.raw(scope), count + index),
                    made.value(scope),
                );
                scope.release_last(2);
            }
        }
        held = Some((array, count));
    }
    match counted {
        Ok((_, out, choices)) => Ok(scope.with_room(|scope| {
            let bytes = out.bytes();
            let reservation = scope.reserve(bytes);
            let base = reservation.take(bytes);
            // SAFETY: a live array of `2 * count` elements, which may have
            // moved when the room was made.
            let made = held.map_or(0, |(array, count)| unsafe {
                array::element(array.raw(reservation.scope), count)
            });
            let (built, _) = run(
                reservation.scope,
                asked,
                Out::building(base, bytes),
                choices,
                Provided::Take(made),
            );
            built.map_or(Value::NULL, |(value, ..)| value)
        })),
        Err(misfit) => Err(decode_error(
            scope,
            &Refused {
                message: misfit.message,
                path: misfit.path,
                reason: "",
            },
        )),
    }
}

impl Decoder<'_> {
    fn fail<T>(&self, message: impl Into<String>) -> Result<T, Misfit> {
        Err(Misfit {
            message: message.into(),
            path: self.path.clone(),
            deep: false,
        })
    }

    fn too_deep<T>(&self) -> Result<T, Misfit> {
        Err(Misfit {
            message: format!("value nests more than {MAX_DEPTH} levels deep"),
            path: self.path.clone(),
            deep: true,
        })
    }

    /// Run `inside` one level down, at the path `segment` adds.
    fn within<R>(
        &mut self,
        segment: impl FnOnce(&mut String),
        inside: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let length = self.path.len();
        segment(&mut self.path);
        self.depth += 1;
        let result = inside(self);
        self.depth -= 1;
        self.path.truncate(length);
        result
    }

    unsafe fn decode(&mut self, json: Value, described: Desc) -> Result<Got, Misfit> {
        if self.depth > MAX_DEPTH {
            return self.too_deep();
        }
        let by_fields = std::mem::take(&mut self.by_fields);
        // SAFETY: the caller's contract, for everything reached from the
        // value.
        unsafe {
            let node = Node::of(json);
            if described.shown_only() {
                // A class or an enum `baml.json` has no form for.
                return self.fail("the type has no JSON form");
            }
            match described.kind() {
                KIND_NULL => match node {
                    Node::Null => Ok(Got::Null),
                    _ => self.fail("expected null"),
                },
                KIND_BOOL => match node {
                    Node::Bool(value) => Ok(Got::Bool(value)),
                    _ => self.fail("expected boolean"),
                },
                KIND_INT => match node {
                    Node::Int(value) => Ok(Got::Int(value)),
                    Node::Bigint(_) | Node::Float(_) => {
                        self.fail("expected integer in the BAML int range [-2^63, 2^63 - 1]")
                    }
                    _ => self.fail("expected integer"),
                },
                KIND_FLOAT => match node {
                    Node::Float(value) => Ok(Got::Float(value)),
                    Node::Int(value) => Ok(Got::Float(value as f64)),
                    // The nearest `float`, as its decimal text reads.
                    Node::Bigint(object) => Ok(Got::Float(
                        crate::bigint::value(object)
                            .to_string()
                            .parse()
                            .unwrap_or(f64::INFINITY),
                    )),
                    _ => self.fail("expected number"),
                },
                KIND_STRING => match node {
                    Node::String(string) => Ok(Got::Ref(string)),
                    Node::Variant(name) => Ok(Got::Ref(self.out.string(name))),
                    _ => self.fail("expected string"),
                },
                KIND_ARRAY => self.array(node, described.of_a()),
                KIND_MAP => self.map(node, described.of_a(), described.of_b()),
                KIND_CLASS => match described.hook(HOOK_FROM_JSON) {
                    Some(from_json) if !by_fields => {
                        self.provided(json, node, described, from_json)
                    }
                    _ => self.class(node, described),
                },
                KIND_ENUM => {
                    let Some(written) = node.text() else {
                        return self.fail("expected enum variant string");
                    };
                    let aliased = self.aliased;
                    let found = described.variants().position(|variant| {
                        variant.shown(aliased) && variant.key(aliased) == written
                    });
                    match found {
                        Some(index) => Ok(Got::Variant(described.class_object(), index)),
                        None => self.fail(format!(
                            "unknown variant `{written}` for enum `{}`",
                            described.name()
                        )),
                    }
                }
                KIND_VARIANT => {
                    let of = described.of_a();
                    let Some(variant) = of.variant(described.b()) else {
                        return self.fail(format!("`{}` is not an enum", of.name()));
                    };
                    if !variant.shown(self.aliased) {
                        return self.fail(format!(
                            "enum variant `{}.{}` is `@skip`, so JSON has no name for it",
                            of.name(),
                            variant.name
                        ));
                    }
                    let expected = variant.key(self.aliased);
                    if node.text() == Some(expected) {
                        Ok(Got::Variant(of.class_object(), described.b()))
                    } else {
                        self.fail(format!("expected enum variant `{expected}`"))
                    }
                }
                KIND_STRING_IS => {
                    if node.text() != Some(described.literal()) {
                        return self.fail("literal mismatch");
                    }
                    match node {
                        Node::String(string) => Ok(Got::Ref(string)),
                        _ => Ok(Got::Ref(self.out.string(described.literal()))),
                    }
                }
                KIND_INT_IS => {
                    let expected = (described.a() | described.b() << 32) as i64;
                    match node {
                        Node::Int(value) if value == expected => Ok(Got::Int(value)),
                        _ if node.is_number() => self.fail("literal int mismatch"),
                        _ => self.fail("literal mismatch"),
                    }
                }
                KIND_BOOL_IS => match node {
                    Node::Bool(value) if usize::from(value) == described.a() => {
                        Ok(Got::Bool(value))
                    }
                    _ => self.fail("literal mismatch"),
                },
                KIND_OPTIONAL => {
                    if json.is_null() {
                        Ok(Got::Null)
                    } else {
                        self.decode(json, described.of_a())
                    }
                }
                KIND_UNION => self.union(json, node, described),
                // `unknown` and `json` hold the value as it is.
                _ => self.copy(json),
            }
        }
    }

    /// A copy of the `json` value, as `unknown` and `json` hold one.
    unsafe fn copy(&mut self, json: Value) -> Result<Got, Misfit> {
        let options = encode::Options {
            aliased: false,
            omit_nulls: false,
        };
        let out = std::mem::replace(&mut self.out, Out::counting());
        let mut sink = encode::JsonSink::new(out);
        // SAFETY: the caller's contract.
        let walked = unsafe {
            encode::walk(
                self.types,
                json,
                None,
                options,
                true,
                &mut encode::Overrides::Count(0),
                &mut sink,
            )
        };
        let (copied, out) = sink.finish();
        self.out = out;
        match walked {
            Ok(()) => Ok(Got::of(copied)),
            Err(refused) => Err(Misfit {
                message: refused.message,
                path: format!("{}{}", self.path, refused.path),
                deep: false,
            }),
        }
    }

    unsafe fn array(&mut self, node: Node, element: Desc) -> Result<Got, Misfit> {
        let Node::Array(array) = node else {
            return self.fail(if self.trying == 0 {
                "expected JSON array"
            } else {
                "expected array"
            });
        };
        // SAFETY: a live array.
        unsafe {
            let length = load(array + ARRAY_LENGTH);
            let mut items = Vec::with_capacity(length);
            for index in 0..length {
                let item = Value::load(array::element(array, index));
                let got = self.within(
                    |path| {
                        let _ = write!(path, "[{index}]");
                    },
                    |this| this.decode(item, element),
                )?;
                items.push(place(got));
            }
            Ok(Got::Ref(self.out.array(items.into_iter())))
        }
    }

    unsafe fn map(&mut self, node: Node, keys: Desc, values: Desc) -> Result<Got, Misfit> {
        if !keys.is_string_key() {
            return self.fail(STRING_MAP_REQUIRED);
        }
        let Node::Object(object) = node else {
            return self.fail(if self.trying == 0 {
                "expected JSON object"
            } else {
                "expected object"
            });
        };
        // SAFETY: a live map.
        unsafe {
            if !string_keyed(object) {
                return self.fail(STRING_MAP_REQUIRED);
            }
            let mut entries = Vec::new();
            if map::len(object) != 0 {
                map::for_each(object, |key, value| entries.push((key.payload, value)));
            }
            let any_key = keys.kind() == KIND_STRING;
            let mut literals = Vec::new();
            keys.members(&mut literals);
            for (key, value) in &mut entries {
                let written = crate::string::text(*key);
                if !any_key && !literals.iter().any(|literal| literal.literal() == written) {
                    return self.fail("map key does not match declared key type");
                }
                let got = self.within(
                    |path| {
                        let _ = write!(path, "[{written:?}]");
                    },
                    |this| this.decode(*value, values),
                )?;
                *value = place(got);
            }
            Ok(Got::Ref(self.out.map(entries.into_iter())))
        }
    }

    /// A value of a class with a `from_json` of its own: what that makes
    /// of the `json` in `json`. While a member of a union is tried the
    /// class is read by its fields, like any other.
    unsafe fn provided(
        &mut self,
        json: Value,
        node: Node,
        described: Desc,
        from_json: BamlFn,
    ) -> Result<Got, Misfit> {
        if self.trying != 0 {
            self.tried_provided = true;
            // SAFETY: the caller's contract.
            return unsafe { self.class(node, described) };
        }
        match &mut self.provided {
            Provided::Note { entries, into } => {
                if *into != 0 {
                    // SAFETY: the native has room there for each value a
                    // run like this one noted.
                    unsafe { Value::store(*into + entries.len() * VALUE, json) };
                }
                entries.push(from_json);
                Ok(Got::Null)
            }
            Provided::Take(next) => {
                // SAFETY: the native has there what each `from_json`
                // returned, in the order a run like this one noted them.
                let made = unsafe { Value::load(*next) };
                *next += VALUE;
                Ok(Got::of(made))
            }
        }
    }

    unsafe fn class(&mut self, node: Node, described: Desc) -> Result<Got, Misfit> {
        let Node::Object(object) = node else {
            return self.fail(format!(
                "expected JSON object for class `{}`",
                described.name()
            ));
        };
        // SAFETY: a live map, and an instance with the fields its
        // description names.
        unsafe {
            if !string_keyed(object) {
                return self.fail(STRING_MAP_REQUIRED);
            }
            let mut slots = Vec::with_capacity(described.count());
            for field in described.fields() {
                let Some(ty) = field.ty else { continue };
                let got = if field.shown(self.aliased) {
                    let key = field.key(self.aliased);
                    let held = match object_get(object, key) {
                        Some(held) => held,
                        None if field.may_be_absent => Value::NULL,
                        None => return self.fail(format!("missing required field `{key}`")),
                    };
                    self.within(
                        |path| {
                            path.push('.');
                            path.push_str(key);
                        },
                        |this| this.decode(held, ty),
                    )?
                } else {
                    // A `@skip` field holds its type's empty value.
                    match self.empty(ty) {
                        Some(empty) => empty,
                        None => {
                            return self.fail(format!(
                                "field `{}.{}` is `@skip`, so JSON leaves it out, and its type \
                                 has no empty value to hold",
                                described.name(),
                                field.name
                            ));
                        }
                    }
                };
                slots.push((field.offset, place(got)));
            }
            let instance = self.out.instance(described.class_object());
            if instance != 0 {
                for (offset, placed) in slots {
                    Value::store(instance + offset, placed);
                }
            }
            Ok(Got::Ref(instance))
        }
    }

    /// The empty value of a type: what a `@skip` field holds.
    fn empty(&mut self, described: Desc) -> Option<Got> {
        Some(match described.kind() {
            KIND_NULL | KIND_OPTIONAL => Got::Null,
            KIND_BOOL => Got::Bool(false),
            KIND_INT => Got::Int(0),
            KIND_FLOAT => Got::Float(0.0),
            KIND_STRING => Got::Ref(self.out.string("")),
            KIND_ARRAY => Got::Ref(self.out.array(std::iter::empty())),
            KIND_MAP => Got::Ref(self.out.map(std::iter::empty())),
            _ if described.is_any() => Got::Null,
            _ => return None,
        })
    }

    /// How well a member of a union suits a value of JSON kind `kind`:
    /// nothing when it cannot be one.
    fn fit(member: Desc, kind: Kind) -> Option<Fit> {
        let rank = |fits: bool, rank: u64| fits.then_some(Fit::Rank(1 + rank));
        if member.is_any() || (member.kind() == KIND_MAP && member.of_b().is_any()) {
            return Some(Fit::Rank(0));
        }
        match member.kind() {
            KIND_NULL => rank(kind == Kind::Null, 0),
            KIND_BOOL => rank(kind == Kind::Bool, 0),
            KIND_BOOL_IS => rank(kind == Kind::Bool, 1),
            KIND_FLOAT => rank(kind == Kind::Number, 0),
            KIND_INT => rank(kind == Kind::Number, 2),
            KIND_INT_IS => rank(kind == Kind::Number, 3),
            KIND_STRING => rank(kind == Kind::String, 0),
            KIND_ENUM => rank(kind == Kind::String, 1),
            KIND_STRING_IS | KIND_VARIANT => rank(kind == Kind::String, 2),
            KIND_ARRAY => rank(kind == Kind::Array, 0),
            KIND_MAP | KIND_CLASS => (kind == Kind::Object).then_some(Fit::Keys),
            KIND_OPTIONAL => {
                let inner = Self::fit(member.of_a(), kind);
                if kind == Kind::Null {
                    Some(Fit::Rank(1))
                } else {
                    inner
                }
            }
            _ => None,
        }
    }

    /// How many keys of the object at `object` a member of a union uses.
    unsafe fn used_keys(&self, member: Desc, object: usize) -> u64 {
        // SAFETY: a live map of string keys.
        unsafe {
            match member.kind() {
                KIND_MAP => map::len(object) as u64,
                KIND_CLASS => {
                    let mut seen: Vec<&str> = Vec::new();
                    for field in member.fields() {
                        let key = field.key(self.aliased);
                        if field.shown(self.aliased)
                            && !seen.contains(&key)
                            && object_get(object, key).is_some()
                        {
                            seen.push(key);
                        }
                    }
                    seen.len() as u64
                }
                KIND_OPTIONAL => self.used_keys(member.of_a(), object),
                _ => 0,
            }
        }
    }

    unsafe fn union(&mut self, json: Value, node: Node, described: Desc) -> Result<Got, Misfit> {
        let mut members = Vec::new();
        described.members(&mut members);
        if self.out.is_building() {
            // The member the counting pass found.
            let chosen = self.choices.get(self.next_choice).copied().unwrap_or(0);
            self.next_choice += 1;
            return match members.get(chosen) {
                // SAFETY: the caller's contract.
                Some(member) => unsafe { self.decode(json, *member) },
                None => self.fail("no union member can come from JSON"),
            };
        }
        let kind = match node {
            Node::Null => Kind::Null,
            Node::Bool(_) => Kind::Bool,
            Node::Int(_) | Node::Bigint(_) | Node::Float(_) => Kind::Number,
            Node::String(_) | Node::Variant(_) => Kind::String,
            Node::Array(_) => Kind::Array,
            Node::Object(_) => Kind::Object,
        };
        let mut ranked: Vec<(usize, u64)> = Vec::new();
        for (index, member) in members.iter().enumerate() {
            let rank = match (Self::fit(*member, kind), node) {
                (Some(Fit::Rank(rank)), _) => rank,
                // SAFETY: an object node is a live map; its keys are
                // strings when it came from JSON.
                (Some(Fit::Keys), Node::Object(object)) if unsafe { string_keyed(object) } => {
                    1 + unsafe { self.used_keys(*member, object) }
                }
                (Some(Fit::Keys), _) => 1,
                (None, _) => continue,
            };
            ranked.push((index, rank));
        }
        // The most specific first; ties in declaration order.
        ranked.sort_by(|(_, a), (_, b)| b.cmp(a));
        let choice = self.choices.len();
        self.choices.push(0);
        let (bytes, path) = (self.out.bytes(), std::mem::take(&mut self.path));
        self.trying += 1;
        let tried_before = std::mem::take(&mut self.tried_provided);
        for (index, _) in &ranked {
            self.choices[choice] = *index;
            // SAFETY: the caller's contract.
            match unsafe { self.decode(json, members[*index]) } {
                Ok(got) => {
                    self.path = path;
                    self.trying -= 1;
                    let met = std::mem::replace(&mut self.tried_provided, tried_before);
                    self.tried_provided |= met;
                    if met && self.trying == 0 {
                        // The member is the one. It is read again as any
                        // other value is: its `from_json` builds it.
                        self.out.rewind(bytes);
                        self.choices.truncate(choice + 1);
                        self.tried_provided = tried_before;
                        // SAFETY: the caller's contract.
                        return unsafe { self.decode(json, members[*index]) };
                    }
                    return Ok(got);
                }
                Err(misfit) if misfit.deep => {
                    self.path = path;
                    self.trying -= 1;
                    self.tried_provided = tried_before;
                    return self.too_deep();
                }
                Err(_) => {
                    // Nothing of a member that did not fit is kept.
                    self.out.rewind(bytes);
                    self.choices.truncate(choice + 1);
                    self.path.clear();
                }
            }
        }
        // No member fits: say why each does not.
        let is_null = kind == Kind::Null;
        let mut misfits = Vec::new();
        for member in &members {
            if member.kind() == KIND_NULL && !is_null {
                continue;
            }
            self.path.clear();
            // SAFETY: the caller's contract.
            if let Err(misfit) = unsafe { self.decode(json, *member) } {
                if misfit.deep {
                    self.path = path;
                    self.trying -= 1;
                    self.tried_provided = tried_before;
                    return self.too_deep();
                }
                misfits.push((member.name(), misfit));
            }
            self.out.rewind(bytes);
            self.choices.truncate(choice + 1);
        }
        self.path = path;
        self.trying -= 1;
        self.tried_provided = tried_before;
        match misfits.as_slice() {
            [] => self.fail("no union member can come from JSON"),
            [(_, only)] => Err(Misfit {
                message: only.message.clone(),
                path: format!("{}{}", self.path, only.path),
                deep: false,
            }),
            misfits => {
                let named: Vec<String> = misfits
                    .iter()
                    .map(|(name, misfit)| {
                        if misfit.path.is_empty() {
                            format!("`{name}`: {}", misfit.message)
                        } else {
                            format!("`{name}`: {} at `{}`", misfit.message, misfit.path)
                        }
                    })
                    .collect();
                self.fail(format!("no union member matched: {}", named.join("; ")))
            }
        }
    }
}
