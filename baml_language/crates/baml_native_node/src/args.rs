//! Building a call's arguments from JavaScript values, by each parameter's
//! declared type, the way today's TypeScript SDK reads them
//! (`bridge_encode`'s planner with its JavaScript rules):
//!
//! - a class parameter takes an instance of that generated class or a plain
//!   object; only own properties are read, an unknown key is refused, and a
//!   missing nullable field is `null`;
//! - a map takes a plain object (not a `Map`), an `int` a whole number or a
//!   `bigint` in range;
//! - a union ranks its members by how the value stands to each (the value's
//!   own type, its structural type, then a conversion), builds every member
//!   of the best rank that takes it, and breaks a tie by the value's natural
//!   type, then by the fewest fields read as `null`; past that the value is
//!   ambiguous;
//! - nesting deeper than [`MAX_DEPTH`] is refused, so a cyclic value is
//!   refused, never followed.

use std::fmt::Write as _;

use baml_native_host::{BamlValue, SdkError, Session, Type};
use napi_sys::ValueType;

use crate::{
    copy::{Plans, TypePlan, user_name},
    js::{self, Env, Value},
};

/// The deepest an argument or a result may nest, in classes, lists and maps.
pub(crate) const MAX_DEPTH: usize = 200;

/// Why an argument could not be built.
pub(crate) enum ArgError {
    /// JavaScript threw (a getter): what it threw.
    Js(Value),
    /// The value does not fit; the message names where.
    Unfit(String),
    /// Nested past [`MAX_DEPTH`]: final, a union does not try further.
    TooDeep(String),
}

impl From<js::Thrown> for ArgError {
    fn from(thrown: js::Thrown) -> Self {
        Self::Js(thrown.0)
    }
}

/// One step of the path from an argument to where a value sits.
enum Step<'p> {
    Argument(&'p str),
    Field(&'p str),
    Index(u32),
    Key(String),
}

/// What a JavaScript value is, as today's encoder classifies it.
#[derive(Clone, Copy)]
enum Kind {
    Null,
    Bool,
    /// Whole when the second is true.
    Number(f64, bool),
    BigInt,
    String,
    Array,
    PlainObject,
    /// An instance of the generated class of interface type `n`.
    Instance(u32),
    Other,
}

/// How a value stands to a union member before it is built: the order a
/// union prefers (as `bridge_encode`'s `Rank`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    Identity,
    Natural,
    Conversion,
}

/// Names Object.prototype gives every plain object: a field so named is read
/// only when it is the object's own.
const INHERITED: [&str; 12] = [
    "constructor",
    "toString",
    "toLocaleString",
    "valueOf",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "__proto__",
    "__defineGetter__",
    "__defineSetter__",
    "__lookupGetter__",
    "__lookupSetter__",
];

pub(crate) fn is_inherited_name(name: &str) -> bool {
    INHERITED.contains(&name)
}

/// One argument walk.
pub(crate) struct Walk<'p, 's> {
    plans: &'p Plans,
    env: Env,
    session: &'s Session,
    buffer: Vec<u8>,
    path: Vec<Step<'p>>,
    depth: usize,
    /// Declared fields read as `null` because the value left them out.
    omitted: usize,
}

impl<'p, 's> Walk<'p, 's> {
    pub(crate) fn new(plans: &'p Plans, env: Env, session: &'s Session) -> Self {
        Self {
            plans,
            env,
            session,
            buffer: Vec::new(),
            path: Vec::new(),
            depth: 0,
            omitted: 0,
        }
    }

    /// Argument `name` of type `ty`.
    pub(crate) fn argument(
        &mut self,
        name: &'p str,
        ty: &'p Type,
        value: Value,
    ) -> Result<BamlValue<'s>, ArgError> {
        self.path.clear();
        self.path.push(Step::Argument(name));
        self.depth = 0;
        self.value(ty, value)
    }

    fn where_(&self) -> String {
        let mut out = String::new();
        for step in &self.path {
            match step {
                Step::Argument(name) => {
                    let _ = write!(out, "argument `{name}`");
                }
                Step::Field(name) => {
                    let _ = write!(out, ".{name}");
                }
                Step::Index(index) => {
                    let _ = write!(out, "[{index}]");
                }
                Step::Key(key) => {
                    let _ = write!(out, "[{key:?}]");
                }
            }
        }
        out
    }

    fn unfit(&self, why: &str) -> ArgError {
        ArgError::Unfit(format!("{}: {why}", self.where_()))
    }

    fn mismatch(&self, ty: &Type, value: Value) -> ArgError {
        self.unfit(&format!(
            "expected {}, found {}",
            self.plans.render(ty),
            self.found(value)
        ))
    }

    fn refused(&self, error: &SdkError) -> ArgError {
        self.unfit(&error.to_string())
    }

    /// One level deeper: a class, a list or a map.
    fn enter(&mut self) -> Result<(), ArgError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(ArgError::TooDeep(format!(
                "{}: the value is nested too deeply for the boundary, which carries {MAX_DEPTH} \
                 levels",
                self.where_()
            )));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    fn kind(&self, value: Value) -> Result<Kind, ArgError> {
        let env = self.env;
        Ok(match js::type_of(env, value) {
            ValueType::napi_undefined | ValueType::napi_null => Kind::Null,
            ValueType::napi_boolean => Kind::Bool,
            ValueType::napi_number => {
                let n = js::get_f64(env, value);
                Kind::Number(n, n.fract() == 0.0 && n.abs() <= 9.223_372_036_854_775e18)
            }
            ValueType::napi_bigint => Kind::BigInt,
            ValueType::napi_string => Kind::String,
            ValueType::napi_object if js::is_array(env, value) => Kind::Array,
            ValueType::napi_object => self.plans.object_kind(env, value)?,
            _ => Kind::Other,
        })
    }

    fn found(&self, value: Value) -> String {
        self.plans.describe(self.env, value)
    }

    fn value(&mut self, ty: &'p Type, value: Value) -> Result<BamlValue<'s>, ArgError> {
        let env = self.env;
        let kind = self.kind(value)?;
        match ty {
            Type::Int => match kind {
                Kind::Number(n, true) =>
                {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "a whole number within the range of an int: exact"
                    )]
                    Ok(self.session.int(n as i64))
                }
                Kind::BigInt => match js::get_bigint_i64(env, value) {
                    Some(n) => Ok(self.session.int(n)),
                    None => Err(self.mismatch(ty, value)),
                },
                _ => Err(self.mismatch(ty, value)),
            },
            Type::Float => match kind {
                Kind::Number(n, _) => Ok(self.session.float(n)),
                _ => Err(self.mismatch(ty, value)),
            },
            Type::Bool => match kind {
                Kind::Bool => Ok(self.session.bool(js::get_bool(env, value))),
                _ => Err(self.mismatch(ty, value)),
            },
            Type::Null => match kind {
                Kind::Null => Ok(self.session.null()),
                _ => Err(self.mismatch(ty, value)),
            },
            Type::String => match kind {
                Kind::String => {
                    let text = js::get_string(env, value, &mut self.buffer);
                    self.session.string(text).map_err(|e| self.refused(&e))
                }
                _ => Err(self.mismatch(ty, value)),
            },
            Type::LiteralString(expected) => {
                if matches!(kind, Kind::String)
                    && js::get_string(env, value, &mut self.buffer) == expected
                {
                    return self.session.string(expected).map_err(|e| self.refused(&e));
                }
                Err(self.literal_mismatch(ty, value))
            }
            Type::LiteralInt(expected) => match kind {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "compared only when whole and within the range of an int"
                )]
                Kind::Number(n, true) if n as i64 == *expected => Ok(self.session.int(*expected)),
                Kind::BigInt if js::get_bigint_i64(env, value) == Some(*expected) => {
                    Ok(self.session.int(*expected))
                }
                _ => Err(self.literal_mismatch(ty, value)),
            },
            Type::LiteralBool(expected) => {
                if matches!(kind, Kind::Bool) && js::get_bool(env, value) == *expected {
                    return Ok(self.session.bool(*expected));
                }
                Err(self.literal_mismatch(ty, value))
            }
            Type::Enum(of) | Type::EnumVariant { of, .. } => self.enumeration(ty, *of, kind, value),
            Type::List(element) => {
                if !matches!(kind, Kind::Array) {
                    return Err(self.mismatch(ty, value));
                }
                self.enter()?;
                let length = js::array_length(env, value);
                let mut elements = Vec::with_capacity(length as usize);
                for i in 0..length {
                    let item = js::get_element(env, value, i)?;
                    self.path.push(Step::Index(i));
                    let built = self.value(element, item);
                    self.path.pop();
                    elements.push(built?);
                }
                self.leave();
                self.session.array(elements).map_err(|e| self.refused(&e))
            }
            Type::Map(item_type) => {
                if !matches!(kind, Kind::PlainObject) {
                    return Err(self.mismatch(ty, value));
                }
                self.enter()?;
                let keys = js::keys(env, value)?;
                let length = js::array_length(env, keys);
                let mut entries = Vec::with_capacity(length as usize);
                for i in 0..length {
                    let key = js::get_element(env, keys, i)?;
                    let item = js::get_property(env, value, key)?;
                    let text = js::get_string(env, key, &mut self.buffer).to_owned();
                    let key = self.session.string(&text).map_err(|e| self.refused(&e))?;
                    self.path.push(Step::Key(text));
                    let built = self.value(item_type, item);
                    self.path.pop();
                    entries.push((key, built?));
                }
                self.leave();
                self.session.map(entries).map_err(|e| self.refused(&e))
            }
            Type::Class(class) => self.class(ty, *class, kind, value),
            Type::Union(members) => self.union(ty, members, kind, value),
        }
    }

    fn literal_mismatch(&self, ty: &Type, value: Value) -> ArgError {
        self.unfit(&format!(
            "{} does not fit the literal type {}",
            self.found(value),
            self.plans.render(ty)
        ))
    }

    fn enumeration(
        &mut self,
        ty: &Type,
        of: u32,
        kind: Kind,
        value: Value,
    ) -> Result<BamlValue<'s>, ArgError> {
        let Some(TypePlan::Enum { name, index, .. }) = self.plans.types.get(of as usize) else {
            return Err(self.unfit(&format!("type {of} is not an enum")));
        };
        if !matches!(kind, Kind::String) {
            return Err(self.mismatch(ty, value));
        }
        let text = js::get_string(self.env, value, &mut self.buffer);
        let Some(&position) = index.get(text) else {
            let why = format!("enum {} has no variant `{text}`", user_name(name));
            return Err(self.unfit(&why));
        };
        if let Type::EnumVariant { index: wanted, .. } = ty
            && position != *wanted
        {
            return Err(self.literal_mismatch(ty, value));
        }
        self.session
            .variant(of, position)
            .map_err(|e| self.refused(&e))
    }

    fn class(
        &mut self,
        ty: &Type,
        class: u32,
        kind: Kind,
        value: Value,
    ) -> Result<BamlValue<'s>, ArgError> {
        let env = self.env;
        let plans = self.plans;
        let Some(TypePlan::Class {
            name,
            fields,
            field_index,
            ..
        }) = plans.types.get(class as usize)
        else {
            return Err(self.unfit(&format!("type {class} is not a class")));
        };
        let instance = match kind {
            Kind::PlainObject => false,
            Kind::Instance(of) if of == class => true,
            _ => return Err(self.mismatch(ty, value)),
        };
        self.enter()?;
        let mut built = Vec::with_capacity(fields.len());
        let mut present = 0_u32;
        for field in fields {
            let key = js::deref(env, field.key);
            let item = if field.inherited_name && !js::has_own(env, value, key)? {
                js::undefined(env)
            } else {
                js::get_property(env, value, key)?
            };
            if js::type_of(env, item) == ValueType::napi_undefined {
                if field.nullable {
                    if !js::has_own(env, value, key)? {
                        self.omitted += 1;
                    }
                    built.push(self.session.null());
                    continue;
                }
                return Err(self.unfit(&format!(
                    "class {} requires field `{}`",
                    user_name(name),
                    field.name
                )));
            }
            present += 1;
            self.path.push(Step::Field(&field.name));
            let item = self.value(&field.ty, item);
            self.path.pop();
            built.push(item?);
        }
        // An own key that is no field is refused. Counting first keeps the
        // usual value, every key a field, to one call.
        let keys = js::keys(env, value)?;
        let length = js::array_length(env, keys);
        if length > present {
            for i in 0..length {
                let key = js::get_element(env, keys, i)?;
                let text = js::get_string(env, key, &mut self.buffer);
                if !(field_index.contains_key(text) || instance && text == "$types") {
                    let why = format!("class {} has no field `{text}`", user_name(name));
                    return Err(self.unfit(&why));
                }
            }
        }
        self.leave();
        self.session
            .instance(class, built)
            .map_err(|e| self.refused(&e))
    }

    // ---- unions ----------------------------------------------------------

    fn union(
        &mut self,
        ty: &'p Type,
        members: &'p [Type],
        kind: Kind,
        value: Value,
    ) -> Result<BamlValue<'s>, ArgError> {
        let mut flat: Vec<&'p Type> = Vec::new();
        flatten(members, &mut flat);
        let mut ranked: Vec<(Rank, &'p Type)> = flat
            .iter()
            .filter_map(|member| rank(kind, member).map(|rank| (rank, *member)))
            .collect();
        ranked.sort_by_key(|(rank, _)| *rank);
        let tried = ranked.len();
        let mut last_failure = None;
        let mut index = 0;
        while index < ranked.len() {
            let rank = ranked[index].0;
            let mut fits: Vec<(&'p Type, BamlValue<'s>, usize)> = Vec::new();
            while index < ranked.len() && ranked[index].0 == rank {
                let member = ranked[index].1;
                index += 1;
                let (omitted, depth, path) = (self.omitted, self.depth, self.path.len());
                match self.value(member, value) {
                    Ok(built) => fits.push((member, built, self.omitted - omitted)),
                    Err(ArgError::Unfit(why)) => last_failure = Some(why),
                    Err(final_error) => return Err(final_error),
                }
                self.omitted = omitted;
                self.depth = depth;
                self.path.truncate(path);
            }
            if !fits.is_empty() {
                let (chosen, omitted) = self.tiebreak(fits, kind, value)?;
                self.omitted += omitted;
                return Ok(chosen);
            }
        }
        // A union with one member that could take the value's shape reports
        // that member's own reason.
        match last_failure {
            Some(why) if tried == 1 => Err(ArgError::Unfit(why)),
            _ => Err(self.unfit(&format!(
                "expected one of {}, found {}",
                self.plans.render(ty),
                self.found(value)
            ))),
        }
    }

    /// The one place a union member is chosen among those that took the
    /// value at the best rank: the value's natural type, then the fewest
    /// fields read as `null`, else ambiguous. The chosen value, and how many
    /// fields it read as `null`.
    fn tiebreak(
        &mut self,
        mut fits: Vec<(&'p Type, BamlValue<'s>, usize)>,
        kind: Kind,
        value: Value,
    ) -> Result<(BamlValue<'s>, usize), ArgError> {
        if fits.len() == 1 {
            let (_, chosen, omitted) = fits.remove(0);
            return Ok((chosen, omitted));
        }
        if let Some(natural) = self.natural(kind, value, 0)? {
            let matching: Vec<usize> = fits
                .iter()
                .enumerate()
                .filter(|(_, (member, _, _))| equivalent(member, &natural))
                .map(|(i, _)| i)
                .collect();
            if let [only] = matching.as_slice() {
                omit_others(&mut fits, *only);
                let (_, chosen, omitted) = fits.remove(0);
                return Ok((chosen, omitted));
            }
        }
        let cheapest = fits
            .iter()
            .map(|(_, _, omitted)| *omitted)
            .min()
            .unwrap_or(0);
        let cheapest_fits: Vec<usize> = fits
            .iter()
            .enumerate()
            .filter(|(_, (_, _, omitted))| *omitted == cheapest)
            .map(|(i, _)| i)
            .collect();
        if let [only] = cheapest_fits.as_slice() {
            omit_others(&mut fits, *only);
            let (_, chosen, omitted) = fits.remove(0);
            return Ok((chosen, omitted));
        }
        let members: Vec<String> = cheapest_fits
            .iter()
            .map(|i| self.plans.render(fits[*i].0))
            .collect();
        Err(self.unfit(&format!(
            "{} fits more than one of {} and the host cannot yet say which member is meant",
            self.found(value),
            members.join(" | ")
        )))
    }

    /// The type the value has by itself, as the compiler would type it:
    /// `None` when it has none to prefer (an empty list, a `bigint`).
    fn natural(
        &mut self,
        kind: Kind,
        value: Value,
        depth: usize,
    ) -> Result<Option<Type>, ArgError> {
        if depth > MAX_DEPTH {
            return Ok(None);
        }
        let env = self.env;
        Ok(match kind {
            Kind::Null => Some(Type::Null),
            Kind::Bool => Some(Type::Bool),
            Kind::Number(_, true) => Some(Type::Int),
            Kind::Number(_, false) => Some(Type::Float),
            Kind::String => Some(Type::String),
            Kind::Instance(class) => Some(Type::Class(class)),
            Kind::BigInt | Kind::Other => None,
            Kind::Array => {
                let length = js::array_length(env, value);
                let mut items = Vec::new();
                for i in 0..length {
                    let item = js::get_element(env, value, i)?;
                    let kind = self.kind(item)?;
                    items.push(self.natural(kind, item, depth + 1)?);
                }
                unify(items).map(|element| Type::List(Box::new(element)))
            }
            Kind::PlainObject => {
                let keys = js::keys(env, value)?;
                let length = js::array_length(env, keys);
                let mut items = Vec::new();
                for i in 0..length {
                    let key = js::get_element(env, keys, i)?;
                    let item = js::get_property(env, value, key)?;
                    let kind = self.kind(item)?;
                    items.push(self.natural(kind, item, depth + 1)?);
                }
                unify(items).map(|item| Type::Map(Box::new(item)))
            }
        })
    }
}

/// Free the members not chosen first, then keep the chosen one at the front.
fn omit_others<T>(fits: &mut Vec<T>, keep: usize) {
    let chosen = fits.swap_remove(keep);
    fits.clear();
    fits.push(chosen);
}

/// The natural element type of items: one type, or a union of the distinct
/// ones; `None` when there are none or one has no natural type.
fn unify(items: Vec<Option<Type>>) -> Option<Type> {
    let mut distinct: Vec<Type> = Vec::new();
    for item in items {
        let item = item?;
        if !distinct.iter().any(|seen| equivalent(seen, &item)) {
            distinct.push(item);
        }
    }
    match distinct.len() {
        0 => None,
        1 => distinct.pop(),
        _ => Some(Type::Union(distinct)),
    }
}

fn flatten<'t>(members: &'t [Type], out: &mut Vec<&'t Type>) {
    for member in members {
        match member {
            Type::Union(inner) => flatten(inner, out),
            other => {
                if !out.iter().any(|seen| equivalent(seen, other)) {
                    out.push(other);
                }
            }
        }
    }
}

/// Two types are the same type; unions compare as sets of their members.
fn equivalent(a: &Type, b: &Type) -> bool {
    match (a, b) {
        (Type::Union(_), _) | (_, Type::Union(_)) => {
            let (mut left, mut right) = (Vec::new(), Vec::new());
            flatten(std::slice::from_ref(a), &mut left);
            flatten(std::slice::from_ref(b), &mut right);
            left.len() == right.len()
                && left.iter().all(|l| {
                    right
                        .iter()
                        .any(|r| !matches!(r, Type::Union(_)) && equivalent(l, r))
                })
        }
        (Type::List(a), Type::List(b)) | (Type::Map(a), Type::Map(b)) => equivalent(a, b),
        _ => a == b,
    }
}

/// How a value of kind `kind` stands to `member`, or `None` when it can never
/// take it.
fn rank(kind: Kind, member: &Type) -> Option<Rank> {
    match (kind, member) {
        (Kind::Null, Type::Null)
        | (Kind::Bool, Type::Bool | Type::LiteralBool(_))
        | (Kind::String, Type::String | Type::LiteralString(_))
        | (Kind::Number(_, true), Type::Int | Type::LiteralInt(_))
        | (Kind::Number(_, false), Type::Float) => Some(Rank::Identity),
        (Kind::Instance(of), Type::Class(class)) if of == *class => Some(Rank::Identity),
        (Kind::Array, Type::List(_)) | (Kind::PlainObject, Type::Map(_)) => Some(Rank::Natural),
        (Kind::Number(_, true), Type::Float)
        | (Kind::BigInt, Type::Int | Type::LiteralInt(_))
        | (Kind::String, Type::Enum(_) | Type::EnumVariant { .. })
        | (Kind::PlainObject, Type::Class(_)) => Some(Rank::Conversion),
        _ => None,
    }
}

impl Plans {
    /// What an object is: a plain object (its prototype is `Object.prototype`,
    /// another realm's, or `null`), an instance of a generated class of the
    /// interface (its prototype carries the class tag `bind` set), or another
    /// kind of object.
    fn object_kind(&self, env: Env, value: Value) -> Result<Kind, ArgError> {
        let prototype = js::prototype(env, value)?;
        if js::type_of(env, prototype) == ValueType::napi_null
            || js::strict_equals(env, prototype, js::deref(env, self.object_prototype))
        {
            return Ok(Kind::PlainObject);
        }
        let tag = js::get_property(env, value, js::deref(env, self.class_tag))?;
        if js::type_of(env, tag) == ValueType::napi_number {
            if let Some(class) = js::whole(env, tag).and_then(|n| u32::try_from(n).ok()) {
                return Ok(Kind::Instance(class));
            }
        }
        // Another realm's `Object.prototype`: the root of its chain, whose
        // constructor is that realm's `Object`.
        let grand = js::prototype(env, prototype)?;
        if js::type_of(env, grand) == ValueType::napi_null {
            let constructor = js::get_named(env, prototype, c"constructor")?;
            if js::type_of(env, constructor) == ValueType::napi_function {
                let name = js::get_named(env, constructor, c"name")?;
                if js::type_of(env, name) == ValueType::napi_string {
                    let mut buffer = Vec::new();
                    if js::get_string(env, name, &mut buffer) == "Object" {
                        return Ok(Kind::PlainObject);
                    }
                }
            }
        }
        Ok(Kind::Other)
    }
}
