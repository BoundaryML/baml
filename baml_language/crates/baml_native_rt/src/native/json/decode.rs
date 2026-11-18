//! Reading a `json` value into a value of a described type, by the
//! interpreter's rules: a shape check, not a coercion.

use std::fmt::Write as _;

use super::{
    MAX_DEPTH, Node, Refused, STRING_MAP_REQUIRED, decode_error,
    describe::{Desc, Types},
    encode, object_get,
    out::Out,
    string_keyed,
};
use crate::{
    heap::{load, store},
    json_layout::{
        KIND_ARRAY, KIND_BOOL, KIND_BOOL_IS, KIND_CLASS, KIND_ENUM, KIND_FLOAT, KIND_INT,
        KIND_INT_IS, KIND_MAP, KIND_NULL, KIND_OPTIONAL, KIND_STRING, KIND_STRING_IS, KIND_UNION,
        KIND_VARIANT, REPR_BOXED, REPR_PAIR, REPR_REF,
    },
    layout::{ARRAY_DATA, ARRAY_LENGTH, TYPE_BOX_BOOL, TYPE_BOX_FLOAT, TYPE_BOX_INT, WORD},
    map,
    native::{Elem, Scope, Thrown, TypeId},
};

/// A decoded value, before it is put in the form its place holds it in.
#[derive(Clone, Copy)]
enum Got {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    /// A variant of the enum whose boxes have this type id.
    Variant(usize, usize),
    /// A reference to an object.
    Ref(usize),
}

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
}

/// Decode the `json` value `json` as the type a call named by `ty`, and
/// return it as a union holds it.
pub(super) fn to<'s>(
    scope: &mut Scope<'s>,
    ty: TypeId,
    json: Elem<'s>,
    aliased: bool,
) -> Result<usize, Thrown<'s>> {
    let run = |scope: &Scope<'s>, out: Out, choices: Vec<usize>| {
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
        };
        // SAFETY: a `json` value, read before anything is allocated or in
        // reserved room.
        let value = unsafe {
            match types.described(ty) {
                Some(described) => decoder
                    .decode(json.get(scope), described)
                    .map(|got| decoder.place(got, REPR_BOXED).0),
                // A type the call does not describe is `unknown`.
                None => decoder
                    .copy(json.get(scope))
                    .map(|got| decoder.place(got, REPR_BOXED).0),
            }
        };
        value.map(|value| (value, decoder.out, decoder.choices))
    };
    match run(scope, Out::counting(), Vec::new()) {
        Ok((_, out, choices)) => {
            let bytes = out.bytes();
            let reservation = scope.reserve(bytes);
            let base = reservation.take(bytes);
            let built = run(reservation.scope, Out::building(base, bytes), choices);
            Ok(built.map_or(0, |(value, ..)| value))
        }
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

    /// The words a place of representation `repr` holds `got` as: the
    /// slot, and for an optional scalar whether there is a value.
    fn place(&mut self, got: Got, repr: u32) -> (usize, usize) {
        let bits = match got {
            Got::Null => return (0, 0),
            Got::Bool(value) => usize::from(value),
            Got::Int(value) => value as usize,
            Got::Float(value) => value.to_bits() as usize,
            Got::Variant(_, index) => index,
            Got::Ref(object) => return (object, 1),
        };
        if repr != REPR_BOXED {
            return (bits, 1);
        }
        let type_id = match got {
            Got::Bool(_) => TYPE_BOX_BOOL,
            Got::Float(_) => TYPE_BOX_FLOAT,
            Got::Variant(type_id, _) => type_id,
            _ => TYPE_BOX_INT,
        };
        (self.out.boxed(type_id, bits), 1)
    }

    /// Whether elements or values of the type described are references.
    fn holds_refs(described: Desc) -> bool {
        matches!(described.repr(), REPR_REF | REPR_BOXED)
    }

    unsafe fn decode(&mut self, json: usize, described: Desc) -> Result<Got, Misfit> {
        if self.depth > MAX_DEPTH {
            return self.too_deep();
        }
        // SAFETY: the caller's contract, for everything reached from the
        // value.
        unsafe {
            let node = Node::of(self.types, json);
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
                    Node::Float(_) => {
                        self.fail("expected integer in the BAML int range [-2^62, 2^62 - 1]")
                    }
                    _ => self.fail("expected integer"),
                },
                KIND_FLOAT => match node {
                    Node::Float(value) => Ok(Got::Float(value)),
                    Node::Int(value) => Ok(Got::Float(value as f64)),
                    _ => self.fail("expected number"),
                },
                KIND_STRING => match node {
                    Node::String(string) => Ok(Got::Ref(string)),
                    Node::Variant(name) => Ok(Got::Ref(self.out.string(name))),
                    _ => self.fail("expected string"),
                },
                KIND_ARRAY => self.array(node, described.of_a()),
                KIND_MAP => self.map(node, described.of_a(), described.of_b()),
                KIND_CLASS => self.class(node, described),
                KIND_ENUM => {
                    let Some(written) = node.text() else {
                        return self.fail("expected enum variant string");
                    };
                    let aliased = self.aliased;
                    let found = described.variants().position(|variant| {
                        variant.shown(aliased) && variant.key(aliased) == written
                    });
                    match found {
                        Some(index) => Ok(Got::Variant(described.a(), index)),
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
                        Ok(Got::Variant(of.a(), described.b()))
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
                    if json == 0 {
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
    unsafe fn copy(&mut self, json: usize) -> Result<Got, Misfit> {
        let options = encode::Options {
            aliased: false,
            omit_nulls: false,
        };
        let out = std::mem::replace(&mut self.out, Out::counting());
        let mut sink = encode::JsonSink::new(out);
        // SAFETY: the caller's contract.
        let walked = unsafe { encode::walk(self.types, json, None, options, true, &mut sink) };
        let (copied, out) = sink.finish();
        self.out = out;
        match walked {
            Ok(()) => Ok(if copied == 0 {
                Got::Null
            } else {
                Got::Ref(copied)
            }),
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
        // SAFETY: a live `json` array: its elements are references.
        unsafe {
            let length = load(array + ARRAY_LENGTH);
            let mut items = Vec::with_capacity(length);
            for index in 0..length {
                let item = load(load(array + ARRAY_DATA) + index * WORD);
                let got = self.within(
                    |path| {
                        let _ = write!(path, "[{index}]");
                    },
                    |this| this.decode(item, element),
                )?;
                items.push(self.place(got, element.repr()).0);
            }
            Ok(Got::Ref(self.out.array(
                length,
                Self::holds_refs(element),
                &items,
            )))
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
        // SAFETY: a live map whose values are references.
        unsafe {
            if !string_keyed(object) {
                return self.fail(STRING_MAP_REQUIRED);
            }
            let mut entries = Vec::new();
            if map::len(object) != 0 {
                map::for_each(object, |key, value| entries.push((key, value)));
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
                *value = self.place(got, values.repr()).0;
            }
            Ok(Got::Ref(self.out.map(
                entries.len(),
                Self::holds_refs(values),
                &entries,
            )))
        }
    }

    unsafe fn class(&mut self, node: Node, described: Desc) -> Result<Got, Misfit> {
        let Node::Object(object) = node else {
            return self.fail(format!(
                "expected JSON object for class `{}`",
                described.name()
            ));
        };
        // SAFETY: a live map whose values are references, and an instance
        // with the fields its description names.
        unsafe {
            if !string_keyed(object) {
                return self.fail(STRING_MAP_REQUIRED);
            }
            let type_id = described.a();
            let mut slots = Vec::with_capacity(described.count());
            for field in described.fields() {
                let Some(ty) = field.ty else { continue };
                let got = if field.shown(self.aliased) {
                    let key = field.key(self.aliased);
                    let held = match object_get(object, key) {
                        Some(held) => held,
                        None if field.may_be_absent => 0,
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
                slots.push((field.offset, ty.repr(), self.place(got, ty.repr())));
            }
            let instance = self
                .out
                .instance(type_id, self.types.instance_size(type_id));
            if instance != 0 {
                for (offset, repr, (word, present)) in slots {
                    store(instance + offset, word);
                    if repr == REPR_PAIR {
                        store(instance + offset + WORD, present);
                    }
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
            KIND_ARRAY => Got::Ref(self.out.array(0, Self::holds_refs(described.of_a()), &[])),
            KIND_MAP => Got::Ref(self.out.map(0, Self::holds_refs(described.of_b()), &[])),
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

    unsafe fn union(&mut self, json: usize, node: Node, described: Desc) -> Result<Got, Misfit> {
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
            Node::Int(_) | Node::Float(_) => Kind::Number,
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
        for (index, _) in &ranked {
            self.choices[choice] = *index;
            // SAFETY: the caller's contract.
            match unsafe { self.decode(json, members[*index]) } {
                Ok(got) => {
                    self.path = path;
                    self.trying -= 1;
                    return Ok(got);
                }
                Err(misfit) if misfit.deep => {
                    self.path = path;
                    self.trying -= 1;
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
                    return self.too_deep();
                }
                misfits.push((member.name(), misfit));
            }
            self.out.rewind(bytes);
            self.choices.truncate(choice + 1);
        }
        self.path = path;
        self.trying -= 1;
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
