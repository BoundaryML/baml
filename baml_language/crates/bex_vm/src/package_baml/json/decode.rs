//! Decoding a `json` value into a declared type: `to<T>`, the slow path of
//! `decode<T>`, and the body of every implicit `baml.FromJson`.
//!
//! `baml.json.to<T>(j)` is the decode counterpart of `baml.json.from` /
//! `baml.ToJson`, and the mirror of `==`: `baml.ops.equals_equals` lets a
//! provided `Equals.eq` win and compares everything else structurally,
//! recursively. Here the target type drives the walk; wherever it meets a
//! declaration — class, interface, or enum, at the root or any depth, under an
//! optional, in a list, in a map — a `baml.FromJson` rule that provides
//! `from_json` builds the value, and anything else decodes structurally, its
//! children by the same rule. The one place shape decides is a union: its
//! member is chosen by the first structural match, because a provided
//! `from_json` that throws unwinds the whole walk and cannot be tried and
//! fallen through; the chosen member then decodes by the rule above.
//!
//! The walk suspends only to call a provided `from_json`. It keeps the
//! containers it is inside on a stack of its own ([`Decoder`]), so its depth is
//! neither a Rust recursion nor a chain of VM frames, and it resumes exactly
//! where it yielded. A JSON value that contains itself is refused when the
//! walk meets a container it already has open.
//!
//! What the walk decodes in place — primitives, enums, media, aliases, and
//! `json` itself — it decodes with [`Structural`], which reads the VM values
//! directly. It runs quietly first, and only when that fails runs again to
//! name the error and where it is.

#![allow(clippy::ptr_arg)]

use std::{collections::HashMap, sync::Arc};

use baml_type::MediaKind;
use bex_heap::TlabHolder;
use bex_vm_types::{
    BexStr, HeapPtr, Object, RealizedTy, TypeHead, Value, ValueKind,
    types::{Array, Instance},
};
use indexmap::IndexMap;
use rustc_hash::FxHashMap;
use sys_types::JsonNaming;

use super::{
    CYCLIC_JSON, MediaFields, OpenSet, STRING_MAP_REQUIRED,
    plan::{ClassPlan, Plans},
    raise_decode,
};
use crate::{
    BexVm,
    errors::VmRustFnError,
    package_baml::{Continuation, NativeCallResult},
};

/// How many containers deep the in-place decode follows a value before it
/// refuses: it is a Rust recursion, so it stops well before the stack does.
/// `parse` nests no deeper, so only a value built in memory can reach it.
const MAX_STRUCTURAL_DEPTH: usize = 128;

fn too_deep_message() -> String {
    format!("value nests more than {MAX_STRUCTURAL_DEPTH} levels deep")
}

/// How the ROOT value is decoded. Everything below it is dispatched.
#[derive(Clone, Copy)]
pub(super) enum Root {
    /// A `baml.FromJson` rule providing `from_json` for the root type builds
    /// it.
    Dispatch,
    /// The root decodes structurally whatever its rule provides: this walk is
    /// the implicit implementation's own body.
    Structural,
}

/// What decoding one value as one type comes to.
enum Decoded {
    /// The value, decoded in place.
    Value(Value),
    /// A container: its children decode next, one at a time.
    Open(Container),
    /// A `baml.FromJson` rule provides `from_json` for the type: the method
    /// builds the value, and the walk resumes with what it returns.
    Provided {
        from_json: HeapPtr,
        frame: Vec<RealizedTy>,
        json: Value,
    },
}

/// A container the walk is inside: the JSON object or array it was opened
/// over, what is left to decode, and what it has decoded so far. The next
/// child is the one after the last result.
enum Container {
    Class {
        source: HeapPtr,
        plan: Arc<ClassPlan>,
        /// Each field's JSON value, or the value a held field holds, in slot
        /// order.
        fields: Vec<Field>,
        decoded: Vec<Value>,
    },
    List {
        source: HeapPtr,
        elements: Vec<Value>,
        element: RealizedTy,
        decoded: Vec<Value>,
    },
    Map {
        source: HeapPtr,
        entries: Vec<(BexStr, Value)>,
        key: RealizedTy,
        value: RealizedTy,
        decoded: IndexMap<BexStr, Value>,
    },
}

/// One field of a class the walk is inside.
enum Field {
    /// Decoded from this JSON value as the field's type.
    Read(Value),
    /// Not in JSON: a `@skip` field under aliased names holds this, its
    /// type's empty value.
    Held(Value),
}

/// The next child of a container.
enum Child<'c> {
    /// To decode from its JSON value as its type.
    Read(Value, &'c RealizedTy),
    /// Already a value: a [`Field::Held`] field.
    Held(Value),
}

impl Container {
    /// The JSON object or array this container was opened over.
    fn source(&self) -> HeapPtr {
        match self {
            Self::Class { source, .. } | Self::List { source, .. } | Self::Map { source, .. } => {
                *source
            }
        }
    }

    /// The next child — its JSON value and its type, or a held field's
    /// value — or `None` once every child is decoded.
    fn next(&self) -> Option<Child<'_>> {
        match self {
            Self::Class {
                plan,
                fields,
                decoded,
                ..
            } => fields.get(decoded.len()).map(|field| match field {
                Field::Read(json) => Child::Read(*json, &plan.fields[decoded.len()].ty),
                Field::Held(value) => Child::Held(*value),
            }),
            Self::List {
                elements,
                element,
                decoded,
                ..
            } => elements
                .get(decoded.len())
                .map(|json| Child::Read(*json, element)),
            Self::Map {
                entries,
                value,
                decoded,
                ..
            } => entries
                .get(decoded.len())
                .map(|(_, json)| Child::Read(*json, value)),
        }
    }

    /// Append the path segment of the next child — `.key`, `[index]`, or
    /// `["key"]`, the notation `DecodeError.path` uses, with a field named by
    /// its JSON key — if there is one left.
    fn segment(&self, path: &mut String) {
        use std::fmt::Write;
        match self {
            Self::Class { plan, decoded, .. } => {
                if let Some(field) = plan.fields.get(decoded.len()) {
                    let _ = write!(path, ".{}", field.key);
                }
            }
            Self::List {
                elements, decoded, ..
            } => {
                if decoded.len() < elements.len() {
                    let _ = write!(path, "[{}]", decoded.len());
                }
            }
            Self::Map {
                entries, decoded, ..
            } => {
                if let Some((key, _)) = entries.get(decoded.len()) {
                    let _ = write!(path, "[{:?}]", key.as_str());
                }
            }
        }
    }

    /// Record the next child's decoded value.
    fn push(&mut self, child: Value) {
        match self {
            Self::Class { decoded, .. } | Self::List { decoded, .. } => decoded.push(child),
            Self::Map {
                entries, decoded, ..
            } => {
                // A JSON object's keys are distinct, so the next entry is the
                // one after the last result.
                let key = entries[decoded.len()].0.clone();
                decoded.insert(key, child);
            }
        }
    }

    /// The container's value, once every child is decoded.
    fn finish(self, vm: &mut BexVm) -> Value {
        Value::object(match self {
            Self::Class { plan, decoded, .. } => vm.alloc(Object::Instance(Instance::new(
                plan.class,
                plan.type_args.clone(),
                decoded,
            ))),
            // Type-directed: the list carries the element type it decoded.
            Self::List {
                element, decoded, ..
            } => vm.alloc(Object::Array(Array::new(element, decoded))),
            // Type-directed: the declared key type (string-keyed, checked at
            // opening) and the value type it decoded.
            Self::Map {
                key,
                value,
                decoded,
                ..
            } => vm.alloc_map(key, value, decoded),
        })
    }

    /// Every value and declaration this container holds across a yield.
    fn roots(&self, roots: &mut Vec<HeapPtr>) {
        roots.push(self.source());
        let mut value = |value: &Value| roots.extend(value.as_object_ptr());
        match self {
            Self::Class {
                fields, decoded, ..
            } => fields
                .iter()
                .map(|field| match field {
                    Field::Read(json) | Field::Held(json) => json,
                })
                .chain(decoded)
                .for_each(&mut value),
            Self::List {
                elements, decoded, ..
            } => elements.iter().chain(decoded).for_each(&mut value),
            Self::Map {
                entries, decoded, ..
            } => entries
                .iter()
                .map(|(_, json)| json)
                .chain(decoded.values())
                .for_each(&mut value),
        }
        let mut ty = |ty: &RealizedTy| {
            ty.visit_heads(&mut |head| {
                if head.is_resolved() {
                    roots.push(head.ptr());
                }
            });
        };
        match self {
            Self::Class { plan, .. } => plan.roots(roots),
            Self::List { element, .. } => ty(element),
            Self::Map { key, value, .. } => {
                ty(key);
                ty(value);
            }
        }
    }

    /// Repoint everything [`Self::roots`] reported after a collection.
    fn forward(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        let forward_value = |value: &mut Value| {
            if let Some(moved) = value.as_object_ptr().and_then(|ptr| forwarding.get(&ptr)) {
                *value = Value::object(*moved);
            }
        };
        let forward_ty = |ty: &mut RealizedTy| {
            ty.visit_heads_mut(&mut |head| {
                if head.is_resolved()
                    && let Some(moved) = forwarding.get(&head.ptr())
                {
                    head.forward_to(*moved);
                }
            });
        };
        match self {
            Self::Class { source, .. } | Self::List { source, .. } | Self::Map { source, .. } => {
                if let Some(moved) = forwarding.get(source) {
                    *source = *moved;
                }
            }
        }
        match self {
            Self::Class {
                plan,
                fields,
                decoded,
                ..
            } => {
                Arc::make_mut(plan).forward(forwarding);
                fields
                    .iter_mut()
                    .map(|field| match field {
                        Field::Read(json) | Field::Held(json) => json,
                    })
                    .chain(decoded)
                    .for_each(forward_value);
            }
            Self::List {
                elements,
                element,
                decoded,
                ..
            } => {
                forward_ty(element);
                elements.iter_mut().chain(decoded).for_each(forward_value);
            }
            Self::Map {
                entries,
                key,
                value,
                decoded,
                ..
            } => {
                forward_ty(key);
                forward_ty(value);
                entries
                    .iter_mut()
                    .map(|(_, json)| json)
                    .chain(decoded.values_mut())
                    .for_each(forward_value);
            }
        }
    }
}

/// What one decode call has looked up, kept for the rest of the call.
pub(super) struct Lookups {
    pub(super) plans: Plans,
    /// The `from_json` a `baml.FromJson` rule provides for a type, if any.
    provided: FxHashMap<RealizedTy, Option<(HeapPtr, Vec<RealizedTy>)>>,
    /// Whether decoding a type can reach a provided `from_json`.
    reaches: FxHashMap<RealizedTy, bool>,
}

/// Distinct types [`Lookups::reaches_override`] follows before it assumes an
/// override can be reached (a generic class can nest without end).
const MAX_REACH_TYPES: usize = 256;

impl Lookups {
    pub(super) fn new(naming: JsonNaming) -> Self {
        Self {
            plans: Plans::new(naming),
            provided: FxHashMap::default(),
            reaches: FxHashMap::default(),
        }
    }

    /// The `from_json` a `baml.FromJson` rule provides for `ty`, the class,
    /// interface, or enum type at `head`, with the rule's realized frame.
    fn provided(
        &mut self,
        vm: &BexVm,
        head: TypeHead,
        ty: &RealizedTy,
    ) -> Option<Result<(HeapPtr, Vec<RealizedTy>), VmRustFnError>> {
        if let Some(provided) = self.provided.get(ty) {
            return provided.clone().map(Ok);
        }
        let provided = provided_from_json(vm, head, ty);
        match &provided {
            Some(Err(_)) => {}
            Some(Ok(found)) => {
                self.provided.insert(ty.clone(), Some(found.clone()));
            }
            None => {
                self.provided.insert(ty.clone(), None);
            }
        }
        provided
    }

    /// Whether decoding `ty` can reach a `from_json` some `baml.FromJson`
    /// rule provides, by the walk's own rules: through unions, lists, map
    /// values, and the shown fields of classes, but not into aliases or
    /// `json`, which decode in place.
    pub(super) fn reaches_override(&mut self, vm: &BexVm, ty: &RealizedTy) -> bool {
        if !matches!(
            ty,
            RealizedTy::Union(_)
                | RealizedTy::List(_)
                | RealizedTy::Map { .. }
                | RealizedTy::Class(..)
                | RealizedTy::Interface(..)
                | RealizedTy::Enum(_)
        ) {
            return false;
        }
        if let Some(&reaches) = self.reaches.get(ty) {
            return reaches;
        }
        if self.reaches.len() >= MAX_REACH_TYPES {
            return true;
        }
        // A type met again while it is being worked out adds nothing.
        self.reaches.insert(ty.clone(), false);
        let reaches = match ty {
            RealizedTy::Union(members) if members.iter().any(RealizedTy::is_null) => {
                self.reaches_override(vm, &ty.strip_null())
            }
            RealizedTy::Union(members) => members
                .iter()
                .any(|member| self.reaches_override(vm, member)),
            RealizedTy::List(element) => self.reaches_override(vm, element),
            RealizedTy::Map { value, .. } => self.reaches_override(vm, value),
            RealizedTy::Class(head, _) | RealizedTy::Interface(head, _, _)
                if vm.stdlib_heads().media_kind(*head).is_some() =>
            {
                false
            }
            RealizedTy::Class(head, type_args) | RealizedTy::Interface(head, type_args, _) => {
                self.provided(vm, *head, ty).is_some()
                    || match self.plans.class(vm, *head, type_args) {
                        Ok(plan) => plan
                            .fields
                            .iter()
                            .filter(|field| field.shown)
                            .any(|field| self.reaches_override(vm, &field.ty)),
                        Err(_) => false,
                    }
            }
            RealizedTy::Enum(head) => self.provided(vm, *head, ty).is_some(),
            _ => false,
        };
        self.reaches.insert(ty.clone(), reaches);
        reaches
    }

    fn roots(&self, roots: &mut Vec<HeapPtr>) {
        for (from_json, frame) in self.provided.values().flatten() {
            roots.push(*from_json);
            for ty in frame {
                ty.visit_heads(&mut |head| {
                    if head.is_resolved() {
                        roots.push(head.ptr());
                    }
                });
            }
        }
    }

    /// Forget everything: after a collection the pointers may be stale.
    fn clear(&mut self) {
        self.plans.clear();
        self.provided.clear();
        self.reaches.clear();
    }
}

/// The walk's state: the containers it is inside, outermost first, and what
/// it has looked up. It is its own continuation — what a provided `from_json`
/// returns is the next child of the innermost container, or the whole result
/// when there is none.
pub(super) struct Decoder {
    open: Vec<Container>,
    /// The sources of `open`, to find a value that contains itself.
    inside: OpenSet,
    pub(super) lookups: Lookups,
}

impl Decoder {
    /// A walk that has opened nothing yet.
    pub(super) fn new(naming: JsonNaming) -> Self {
        Self {
            open: Vec::new(),
            inside: OpenSet::default(),
            lookups: Lookups::new(naming),
        }
    }

    /// Decode `json` as `ty` (the root, per `root`) inside the open
    /// containers, then keep walking.
    pub(super) fn decode(
        mut self,
        vm: &mut BexVm,
        json: Value,
        ty: &RealizedTy,
        root: Root,
    ) -> NativeCallResult {
        match decode_one(vm, json, ty, root, &mut self.lookups, &|| String::new()) {
            Ok(Decoded::Value(value)) => self.resume(vm, value),
            Ok(Decoded::Open(container)) => match self.open(vm, container, &|| String::new()) {
                Ok(()) => self.walk(vm),
                Err(error) => NativeCallResult::Error(error),
            },
            Ok(Decoded::Provided {
                from_json,
                frame,
                json,
            }) => NativeCallResult::YieldToCall {
                callee: from_json,
                args: vec![json],
                type_args: frame,
                continuation: Box::new(self),
            },
            Err(error) => NativeCallResult::Error(error),
        }
    }

    /// Enter `container`, found at `path`. A JSON object or array the walk
    /// already has open is a value that contains itself: decoding it as a type
    /// would never end.
    fn open(
        &mut self,
        vm: &mut BexVm,
        container: Container,
        path: &dyn Fn() -> String,
    ) -> Result<(), VmRustFnError> {
        let source = container.source();
        if self.inside.contains(source) {
            return Err(raise_decode(vm, CYCLIC_JSON, &path()));
        }
        self.inside.insert(source);
        self.open.push(container);
        Ok(())
    }

    /// `value` is the next child of the innermost container; with none open
    /// it is the result.
    fn resume(mut self, vm: &mut BexVm, value: Value) -> NativeCallResult {
        match self.open.last_mut() {
            Some(container) => {
                container.push(value);
                self.walk(vm)
            }
            None => NativeCallResult::Done(value),
        }
    }

    /// Decode children until one yields or the outermost container is done.
    fn walk(mut self, vm: &mut BexVm) -> NativeCallResult {
        loop {
            let Self { open, lookups, .. } = &mut self;
            let container = open
                .last()
                .unwrap_or_else(|| unreachable!("the walk runs inside an open container"));
            let decoded = match container.next() {
                Some(Child::Held(value)) => Ok(Decoded::Value(value)),
                Some(Child::Read(json, ty)) => {
                    // The path of the next child of the innermost container,
                    // from the root: each open container's segment for the
                    // child it is on. Only an error needs it.
                    let open = &*open;
                    let path = || {
                        let mut path = String::new();
                        for container in open {
                            container.segment(&mut path);
                        }
                        path
                    };
                    decode_one(vm, json, ty, Root::Dispatch, lookups, &path)
                }
                None => {
                    let container = self
                        .open
                        .pop()
                        .unwrap_or_else(|| unreachable!("the container was just read"));
                    self.inside.remove(container.source());
                    let finished = container.finish(vm);
                    match self.open.last_mut() {
                        Some(parent) => {
                            parent.push(finished);
                            continue;
                        }
                        None => return NativeCallResult::Done(finished),
                    }
                }
            };
            match decoded {
                Ok(Decoded::Value(value)) => {
                    if let Some(container) = self.open.last_mut() {
                        container.push(value);
                    }
                }
                Ok(Decoded::Open(child)) => {
                    let opened = {
                        let open = &self.open;
                        let path = || {
                            let mut path = String::new();
                            for container in open {
                                container.segment(&mut path);
                            }
                            path
                        };
                        if self.inside.contains(child.source()) {
                            Err(raise_decode(vm, CYCLIC_JSON, &path()))
                        } else {
                            Ok(())
                        }
                    };
                    match opened {
                        Ok(()) => {
                            self.inside.insert(child.source());
                            self.open.push(child);
                        }
                        Err(error) => return NativeCallResult::Error(error),
                    }
                }
                Ok(Decoded::Provided {
                    from_json,
                    frame,
                    json,
                }) => {
                    return NativeCallResult::YieldToCall {
                        callee: from_json,
                        args: vec![json],
                        type_args: frame,
                        continuation: Box::new(self),
                    };
                }
                Err(error) => return NativeCallResult::Error(error),
            }
        }
    }
}

impl Continuation for Decoder {
    fn call(self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        self.resume(vm, value)
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        let mut roots = Vec::new();
        for container in &self.open {
            container.roots(&mut roots);
        }
        self.lookups.roots(&mut roots);
        roots
    }

    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        for container in &mut self.open {
            container.forward(forwarding);
        }
        self.inside = OpenSet::default();
        for container in &self.open {
            self.inside.insert(container.source());
        }
        self.lookups.clear();
    }
}

/// Decode `json` as `ty`, found at `path`, one level: its value if it decodes
/// in place, the container to walk if it has children, or the `from_json`
/// that builds it.
///
/// - a nullable union: `null` is `null`, anything else decodes as the
///   non-null payload;
/// - any other union: the first member `json` matches structurally is the
///   one, and decodes by the rules below;
/// - a list or map opens, to decode each element or value;
/// - a class, interface, or enum (media aside): a `baml.FromJson` rule that
///   provides `from_json` wins — unless this is a [`Root::Structural`] root,
///   whose rule is the walk itself — else a class opens to decode each field
///   and an enum decodes in place;
/// - everything else (primitives, media, literals, type aliases) decodes
///   structurally — none can carry an override.
fn decode_one(
    vm: &mut BexVm,
    json: Value,
    ty: &RealizedTy,
    root: Root,
    lookups: &mut Lookups,
    path: &dyn Fn() -> String,
) -> Result<Decoded, VmRustFnError> {
    match ty {
        RealizedTy::Union(members) if members.iter().any(RealizedTy::is_null) => {
            if json.is_null() {
                Ok(Decoded::Value(Value::NULL))
            } else {
                decode_one(vm, json, &ty.strip_null(), Root::Dispatch, lookups, path)
            }
        }
        RealizedTy::Union(members) => {
            // Membership by shape: a provided `from_json` that throws unwinds
            // the whole walk, so a member cannot be tried through its rule
            // and fallen through to the next.
            validate(vm, json, path)?;
            for member in members {
                if !could_decode(vm, json, member) {
                    continue;
                }
                let mut probe = Structural::quiet(&mut lookups.plans);
                let value = match probe.decode(vm, json, member) {
                    Ok(value) => value,
                    Err(Failed::Deep) => return Err(raise_decode(vm, too_deep_message(), &path())),
                    Err(Failed::Quiet | Failed::Raised(_)) => continue,
                };
                // Without an override in reach, the member decodes to what
                // the probe built.
                if !lookups.reaches_override(vm, member) {
                    return Ok(Decoded::Value(value));
                }
                return decode_one(vm, json, &member.clone(), Root::Dispatch, lookups, path);
            }
            Err(raise_decode(vm, "no union member matched", &path()))
        }
        RealizedTy::List(element) => open_list(vm, json, element, path).map(Decoded::Open),
        RealizedTy::Map { key, value } => open_map(vm, json, key, value, path).map(Decoded::Open),
        RealizedTy::Class(head, _) | RealizedTy::Interface(head, _, _)
            if vm.stdlib_heads().media_kind(*head).is_some() =>
        {
            decode_leaf(vm, json, ty, lookups, path).map(Decoded::Value)
        }
        RealizedTy::Class(head, _) | RealizedTy::Interface(head, _, _) | RealizedTy::Enum(head) => {
            let provided = match root {
                Root::Dispatch => lookups.provided(vm, *head, ty),
                Root::Structural => None,
            };
            match (provided, ty) {
                (Some(provided), _) => provided.map(|(from_json, frame)| Decoded::Provided {
                    from_json,
                    frame,
                    json,
                }),
                (
                    None,
                    RealizedTy::Class(_, type_args) | RealizedTy::Interface(_, type_args, _),
                ) => open_class(vm, json, *head, type_args, lookups, path).map(Decoded::Open),
                (None, _) => decode_leaf(vm, json, ty, lookups, path).map(Decoded::Value),
            }
        }
        _ => decode_leaf(vm, json, ty, lookups, path).map(Decoded::Value),
    }
}

/// Decode `json` at `path` as `ty` in place, structurally.
fn decode_leaf(
    vm: &mut BexVm,
    json: Value,
    ty: &RealizedTy,
    lookups: &mut Lookups,
    path: &dyn Fn() -> String,
) -> Result<Value, VmRustFnError> {
    // `json` and `unknown` copy the value, and refuse a bad one as they go.
    if !matches!(ty, RealizedTy::Unknown)
        && !matches!(ty, RealizedTy::TypeAlias(head) if vm.stdlib_heads().is_json_alias(*head))
    {
        validate(vm, json, path)?;
    }
    if let Ok(value) = Structural::quiet(&mut lookups.plans).decode(vm, json, ty) {
        return Ok(value);
    }
    Structural::reporting(&mut lookups.plans, path()).decode_reported(vm, json, ty)
}

/// Whether a value of `json`'s JSON kind could decode as `member` at all: a
/// cheap check before a probe.
fn could_decode(vm: &BexVm, json: Value, member: &RealizedTy) -> bool {
    let kind = match json.kind() {
        ValueKind::Null | ValueKind::OmittedArg => return true,
        ValueKind::Bool(_) => Kind::Bool,
        ValueKind::Int(_) => Kind::Number,
        ValueKind::Object(ptr) => match vm.get_object(ptr) {
            Object::Float(_) | Object::Bigint(_) => Kind::Number,
            Object::String(_) | Object::Variant(_) => Kind::String,
            Object::Array(_) => Kind::Array,
            Object::Map(_) => Kind::Object,
            _ => return true,
        },
    };
    match member {
        RealizedTy::Bool => kind == Kind::Bool,
        RealizedTy::Int | RealizedTy::Float => kind == Kind::Number,
        RealizedTy::Bigint => matches!(kind, Kind::Number | Kind::String),
        RealizedTy::String | RealizedTy::Enum(_) | RealizedTy::EnumVariant(..) => {
            kind == Kind::String
        }
        RealizedTy::List(_) => kind == Kind::Array,
        RealizedTy::Map { .. } | RealizedTy::Class(..) | RealizedTy::Media(_) => {
            kind == Kind::Object
        }
        RealizedTy::Null => false,
        _ => true,
    }
}

#[derive(PartialEq, Eq)]
enum Kind {
    Bool,
    Number,
    String,
    Array,
    Object,
}

/// The entries of the JSON object `json` at `path`: a string-keyed map and
/// nothing else — `expected` names what was wanted when it is not an object.
fn json_object(
    vm: &mut BexVm,
    json: Value,
    expected: impl FnOnce() -> String,
    path: &dyn Fn() -> String,
) -> Result<HeapPtr, VmRustFnError> {
    let refused = match json.as_object_ptr().map(|ptr| (ptr, vm.get_object(ptr))) {
        Some((ptr, Object::Map(map))) => {
            if super::string_keyed(vm, map) {
                return Ok(ptr);
            }
            STRING_MAP_REQUIRED.to_owned()
        }
        Some(_) | None => expected(),
    };
    Err(raise_decode(vm, refused, &path()))
}

/// The class at `head` (instantiated with `type_args`), opened over the JSON
/// object `json`: each declared field's JSON value, under its key by the
/// naming, beside its realized type. A field the naming leaves out of JSON
/// (`@skip` under aliased names) holds its type's empty value. A field that
/// can be `null` may be absent and decodes to `null`; an absent required one
/// is an error.
fn open_class(
    vm: &mut BexVm,
    json: Value,
    head: TypeHead,
    type_args: &[RealizedTy],
    lookups: &mut Lookups,
    path: &dyn Fn() -> String,
) -> Result<Container, VmRustFnError> {
    let source = json_object(
        vm,
        json,
        || {
            format!(
                "expected JSON object for class `{}`",
                baml_type::HeadDisplay::head_display_name(&head)
            )
        },
        path,
    )?;
    let plan = match lookups.plans.class(vm, head, type_args) {
        Ok(plan) => plan,
        Err(named) => {
            return Err(raise_decode(
                vm,
                format!("`{named}` is not a class"),
                &path(),
            ));
        }
    };
    let mut fields: Vec<Field> = Vec::with_capacity(plan.fields.len());
    for (index, field) in plan.fields.iter().enumerate() {
        if !field.shown {
            let Some(empty) = super::super::sap::empty_value(vm, &field.ty, &mut Vec::new())?
            else {
                let message = skipped_without_empty(vm, &plan, index);
                return Err(raise_decode(vm, message, &path()));
            };
            fields.push(Field::Held(empty));
            continue;
        }
        let field_json = match super::object_get(vm, source, field.hash(), &field.key) {
            Some(value) => value,
            None if field.may_be_absent => Value::NULL,
            None => {
                return Err(raise_decode(
                    vm,
                    format!("missing required field `{}`", field.key),
                    &path(),
                ));
            }
        };
        fields.push(Field::Read(field_json));
    }
    Ok(Container::Class {
        source,
        decoded: Vec::with_capacity(fields.len()),
        fields,
        plan,
    })
}

/// The message for a `@skip` field, field `index` of `plan`, whose type has
/// no empty value to hold.
fn skipped_without_empty(vm: &BexVm, plan: &ClassPlan, index: usize) -> String {
    format!(
        "field `{}.{}` is `@skip`, so JSON leaves it out, and its type has no empty value to \
         hold",
        plan.named(),
        plan.field_name(vm, index)
    )
}

/// A list of `element`, opened over the JSON array `json`.
fn open_list(
    vm: &mut BexVm,
    json: Value,
    element: &RealizedTy,
    path: &dyn Fn() -> String,
) -> Result<Container, VmRustFnError> {
    match json.as_object_ptr().map(|ptr| (ptr, vm.get_object(ptr))) {
        Some((source, Object::Array(array))) => {
            let elements = array.to_vec();
            Ok(Container::List {
                source,
                decoded: Vec::with_capacity(elements.len()),
                elements,
                element: element.clone(),
            })
        }
        Some(_) | None => Err(raise_decode(vm, "expected JSON array", &path())),
    }
}

/// A map from `key` to `value`, opened over the JSON object `json`. JSON
/// objects have string keys, so `key` must be a string type and every key
/// written must be one of it.
fn open_map(
    vm: &mut BexVm,
    json: Value,
    key: &RealizedTy,
    value: &RealizedTy,
    path: &dyn Fn() -> String,
) -> Result<Container, VmRustFnError> {
    if !baml_type::normalize::is_string_key(key.as_ty(), vm) {
        return Err(raise_decode(vm, STRING_MAP_REQUIRED, &path()));
    }
    let source = json_object(vm, json, || "expected JSON object".to_string(), path)?;
    let entries = super::string_entries(vm, source);
    if !matches!(key, RealizedTy::String)
        && entries
            .iter()
            .any(|(written, _)| !super::key_fits(vm, written, key))
    {
        return Err(raise_decode(
            vm,
            "map key does not match declared key type",
            &path(),
        ));
    }
    Ok(Container::Map {
        source,
        decoded: IndexMap::with_capacity(entries.len()),
        entries,
        key: key.clone(),
        value: value.clone(),
    })
}

/// The `from_json` a `baml.FromJson` rule PROVIDES for `ty`, the class,
/// interface, or enum type at `head`, with the rule's realized frame — the
/// deserialize counterpart of the `ToJson` shim (`make_to_json_callee`),
/// resolved through the impl rules like it (so blanket and out-of-body impls
/// and runtime-declared classes all reach their provided method). `None` for a
/// type whose rule is absent or adopts a default body, which the walk decodes
/// structurally.
fn provided_from_json(
    vm: &BexVm,
    head: TypeHead,
    ty: &RealizedTy,
) -> Option<Result<(HeapPtr, Vec<RealizedTy>), VmRustFnError>> {
    // Type-directed dispatch: there is no receiver value, so the resolver
    // roots in the world that owns the target declaration (its package for a
    // runtime class; the lexical frame's world for static declarations).
    let from_json_head = vm.lang_head(crate::lang_items::BAML_FROM_JSON)?;
    let owner = match vm.get_object(head.ptr()) {
        Object::Class(class) => class
            .owner
            .package()
            .and_then(|owner| vm.runtime_owner(owner)),
        Object::Enum(enum_def) => enum_def
            .owner
            .package()
            .and_then(|owner| vm.runtime_owner(owner)),
        // An interface head carries the same owner edge; rooting it the
        // same way keeps runtime-declared interfaces symmetric with
        // runtime classes instead of falling to the lexical world.
        Object::Interface(iface) => vm.runtime_owner(iface.owner),
        _ => None,
    };
    let resolver = match owner {
        Some(owner) => super::super::resolve::ImplResolver::for_package(vm, owner),
        None => super::super::resolve::ImplResolver::new(vm),
    };
    let (rule, bound_args) = resolver.resolve_implements_rule(ty, from_json_head, &[])?;
    let resolved = match resolver.rule_method_impl(&rule, "from_json") {
        Ok(resolved) => resolved,
        Err(error) => return Some(Err(error.into())),
    };
    if resolved.is_default {
        // The rule adopts the structural default body: the walk decodes the
        // type structurally itself.
        return None;
    }
    Some(
        resolver
            .realize_frame(&resolved.method.frame, &bound_args)
            .map(|frame| (resolved.method.fqn, frame))
            .map_err(Into::into),
    )
}

// ─── Checking a value before a structural read ──────────────────────────────

/// Refuse `json` at `path` if it holds a map whose keys are not strings, or
/// contains itself: what reading it structurally has always refused first,
/// before matching it against any type.
fn validate(vm: &mut BexVm, json: Value, path: &dyn Fn() -> String) -> Result<(), VmRustFnError> {
    match first_invalid(vm, json) {
        None => Ok(()),
        Some((message, inner)) => Err(raise_decode(vm, message, &format!("{}{inner}", path()))),
    }
}

/// The first map with non-string keys, or the first container met again
/// inside itself, in depth-first order: its message and its path inside
/// `json`.
fn first_invalid(vm: &BexVm, json: Value) -> Option<(&'static str, String)> {
    let container = |value: Value| {
        value
            .as_object_ptr()
            .filter(|&ptr| matches!(vm.get_object(ptr), Object::Array(_) | Object::Map(_)))
    };
    let root = container(json)?;
    // Each open container with its children and the next one to visit.
    let mut open: Vec<Visiting> = Vec::new();
    let mut inside = OpenSet::default();
    let mut next = Some(root);
    loop {
        if let Some(ptr) = next.take() {
            if inside.contains(ptr) {
                return Some((CYCLIC_JSON, render_segments(&open)));
            }
            let children = match vm.get_object(ptr) {
                Object::Array(array) => array
                    .to_vec()
                    .into_iter()
                    .enumerate()
                    .map(|(i, value)| (Segment::Index(i), value))
                    .collect(),
                Object::Map(map) => {
                    if !super::string_keyed(vm, map) {
                        return Some((STRING_MAP_REQUIRED, render_segments(&open)));
                    }
                    super::string_entries(vm, ptr)
                        .into_iter()
                        .map(|(key, value)| (Segment::Key(key), value))
                        .collect()
                }
                _ => unreachable!("only containers are visited"),
            };
            inside.insert(ptr);
            open.push((ptr, children, 0));
        }
        let (ptr, children, cursor) = open.last_mut()?;
        match children.get(*cursor) {
            Some((_, child)) => {
                *cursor += 1;
                next = container(*child);
            }
            None => {
                inside.remove(*ptr);
                open.pop();
                if open.is_empty() {
                    return None;
                }
            }
        }
    }
}

/// A container [`first_invalid`] is inside: its children and the next one.
type Visiting = (HeapPtr, Vec<(Segment, Value)>, usize);

/// One step into a container, as `DecodeError.path` writes it.
enum Segment {
    Index(usize),
    Key(BexStr),
}

/// The path to the child each open container is on (its cursor has moved
/// past it).
fn render_segments(open: &[Visiting]) -> String {
    use std::fmt::Write;
    let mut path = String::new();
    for (_, children, cursor) in open {
        match cursor.checked_sub(1).and_then(|i| children.get(i)) {
            Some((Segment::Index(i), _)) => {
                let _ = write!(path, "[{i}]");
            }
            Some((Segment::Key(key), _)) => {
                let _ = write!(path, "[{:?}]", key.as_str());
            }
            None => {}
        }
    }
    path
}

// ─── Structural decode of a `json` value ─────────────────────────────────────

/// Why a structural decode stopped.
pub(super) enum Failed {
    /// Quietly: the caller only needs to know it did not fit.
    Quiet,
    /// Quietly, past [`MAX_STRUCTURAL_DEPTH`]: no other member of a union can
    /// do better, so a union does not try the next.
    Deep,
    /// With the error a `DecodeError` reports.
    Raised(VmRustFnError),
}

impl From<VmRustFnError> for Failed {
    fn from(error: VmRustFnError) -> Self {
        Self::Raised(error)
    }
}

/// A `json` value read as a JSON document: what a JSON node is, whatever VM
/// value holds it.
enum Node {
    Null,
    Bool(bool),
    Int(i64),
    Bigint(Arc<num_bigint::BigInt>),
    Float(f64),
    /// A string object.
    String(Value),
    /// An enum variant reads as the string of its declared name.
    Variant(String),
    Array(HeapPtr),
    Object(HeapPtr),
}

fn node_of(vm: &BexVm, json: Value) -> Node {
    match json.kind() {
        ValueKind::Null | ValueKind::OmittedArg => Node::Null,
        ValueKind::Bool(b) => Node::Bool(b),
        ValueKind::Int(i) => Node::Int(i),
        ValueKind::Object(ptr) => match vm.get_object(ptr) {
            Object::Float(f) => Node::Float(*f),
            Object::String(_) => Node::String(json),
            Object::Bigint(b) => Node::Bigint(Arc::clone(b)),
            Object::Array(_) => Node::Array(ptr),
            Object::Map(_) => Node::Object(ptr),
            Object::Variant(variant) => match vm.get_object(variant.enm) {
                Object::Enum(enm) => enm
                    .variants
                    .get(variant.index)
                    .map_or(Node::Null, |v| Node::Variant(v.name.clone())),
                _ => Node::Null,
            },
            // Anything else in a `json` value reads as `null`, as it always has.
            _ => Node::Null,
        },
    }
}

impl Node {
    fn is_number(&self) -> bool {
        matches!(self, Node::Int(_) | Node::Bigint(_) | Node::Float(_))
    }

    /// The number as an `i64`, when it is integral and fits.
    fn as_i64(&self) -> Option<i64> {
        match self {
            Node::Int(i) => Some(*i),
            Node::Bigint(b) => i64::try_from(&**b).ok(),
            _ => None,
        }
    }

    /// The number as an `f64`: a bigint past `f64` has none.
    fn as_f64(&self) -> Option<f64> {
        match self {
            #[allow(clippy::cast_precision_loss)]
            Node::Int(i) => Some(*i as f64),
            Node::Bigint(b) => b
                .to_string()
                .parse::<f64>()
                .ok()
                .filter(|float| float.is_finite()),
            Node::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// The text of a string node.
    fn text<'a>(&'a self, vm: &'a BexVm) -> Option<&'a str> {
        match self {
            Node::String(value) => vm.as_string(value).ok().map(BexStr::as_str),
            Node::Variant(name) => Some(name),
            _ => None,
        }
    }

    /// An integral number, or a string of decimal digits, as a bigint.
    fn as_bigint(&self, vm: &BexVm) -> Option<num_bigint::BigInt> {
        match self {
            Node::Int(i) => Some(num_bigint::BigInt::from(*i)),
            Node::Bigint(b) => Some((**b).clone()),
            Node::String(_) | Node::Variant(_) => super::parse_decimal_bigint(self.text(vm)?),
            _ => None,
        }
    }
}

/// Reads a `json` value as a type, structurally: no `baml.FromJson` rule is
/// consulted. Quiet, it only says whether the value fits; reporting, it
/// raises the `DecodeError` naming the first misfit and its path.
///
/// Reporting assumes the value passed [`validate`]: neither it nor its parts
/// contain themselves, and every map in it has string keys.
pub(super) struct Structural<'p> {
    plans: &'p mut Plans,
    /// The path of the node being read, kept only when reporting.
    path: Option<String>,
    /// How many containers and aliases deep the read is.
    depth: usize,
}

impl<'p> Structural<'p> {
    pub(super) fn quiet(plans: &'p mut Plans) -> Self {
        Self {
            plans,
            path: None,
            depth: 0,
        }
    }

    pub(super) fn reporting(plans: &'p mut Plans, path: String) -> Self {
        Self {
            plans,
            path: Some(path),
            depth: 0,
        }
    }

    fn naming(&self) -> JsonNaming {
        self.plans.naming
    }

    /// Decode, raising what a reporting decode found.
    pub(super) fn decode_reported(
        mut self,
        vm: &mut BexVm,
        json: Value,
        ty: &RealizedTy,
    ) -> Result<Value, VmRustFnError> {
        match self.decode(vm, json, ty) {
            Ok(value) => Ok(value),
            Err(Failed::Raised(error)) => Err(error),
            // Reporting raises every misfit it meets.
            Err(Failed::Quiet | Failed::Deep) => {
                let path = self.path.unwrap_or_default();
                Err(raise_decode(
                    vm,
                    format!("value does not fit `{ty}`"),
                    &path,
                ))
            }
        }
    }

    fn fail(&self, vm: &mut BexVm, message: impl FnOnce() -> String) -> Failed {
        match &self.path {
            Some(path) => Failed::Raised(raise_decode(vm, message(), path)),
            None => Failed::Quiet,
        }
    }

    /// Run `f` one level deeper, with `segment` appended to the path when
    /// there is one.
    fn within<R>(
        &mut self,
        segment: impl FnOnce(&mut String),
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let saved = self.path.as_mut().map(|path| {
            let len = path.len();
            segment(path);
            len
        });
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        if let (Some(path), Some(len)) = (self.path.as_mut(), saved) {
            path.truncate(len);
        }
        result
    }

    pub(super) fn decode(
        &mut self,
        vm: &mut BexVm,
        json: Value,
        ty: &RealizedTy,
    ) -> Result<Value, Failed> {
        if self.depth > MAX_STRUCTURAL_DEPTH {
            return Err(self.too_deep(vm));
        }
        let node = node_of(vm, json);
        match ty {
            RealizedTy::Null => match node {
                Node::Null => Ok(Value::NULL),
                _ => Err(self.fail(vm, || "expected null".into())),
            },
            RealizedTy::Bool => match node {
                Node::Bool(b) => Ok(Value::bool(b)),
                _ => Err(self.fail(vm, || "expected boolean".into())),
            },
            RealizedTy::Int => {
                if !node.is_number() {
                    return Err(self.fail(vm, || "expected integer".into()));
                }
                match node.as_i64().and_then(Value::try_int) {
                    Some(int) => Ok(int),
                    None => Err(self.fail(vm, || {
                        "expected integer in the BAML int range [-2^62, 2^62 - 1]".into()
                    })),
                }
            }
            RealizedTy::Bigint => {
                let bigint = match node {
                    Node::Bigint(b) => Some(b),
                    _ => node.as_bigint(vm).map(Arc::new),
                };
                match bigint {
                    Some(bigint) => Ok(vm.try_alloc_bigint(bigint).map_err(VmRustFnError::from)?),
                    None => Err(self.fail(vm, || {
                        "expected integral JSON number or numeric string".into()
                    })),
                }
            }
            RealizedTy::Float => match (&node, node.as_f64()) {
                // A float is itself: NaN and the infinities too.
                (Node::Float(_), _) => Ok(json),
                (_, Some(float)) => Ok(Value::object(vm.alloc_float(float))),
                _ => Err(self.fail(vm, || "expected number".into())),
            },
            RealizedTy::String => match node {
                Node::String(value) => Ok(value),
                Node::Variant(name) => Ok(Value::object(vm.alloc_string(name))),
                _ => Err(self.fail(vm, || "expected string".into())),
            },
            RealizedTy::List(element) => self.list(vm, &node, element),
            RealizedTy::Map { key, value } => self.map(vm, &node, key, value),
            RealizedTy::TypeAlias(head) if vm.stdlib_heads().is_json_alias(*head) => {
                self.copy(vm, json)
            }
            RealizedTy::TypeAlias(_) => self.alias(vm, json, ty),
            RealizedTy::Class(head, type_args) => {
                if let Some(kind) = vm.stdlib_heads().media_kind(*head) {
                    return self.media(vm, &node, kind, *head);
                }
                self.class(vm, &node, *head, type_args)
            }
            RealizedTy::Interface(head, type_args, _) => self.class(vm, &node, *head, type_args),
            RealizedTy::Enum(head) => self.enumeration(vm, &node, *head),
            RealizedTy::EnumVariant(head, name) => self.enum_variant(vm, &node, *head, name),
            RealizedTy::Media(kind) => {
                let Some(wrapper) = kind.wrapper_class() else {
                    return Err(self.fail(vm, || {
                        "cannot deserialize generic media - type must be concrete \
                         (image|audio|video|pdf)"
                            .into()
                    }));
                };
                match vm.lang_head(wrapper) {
                    Some(head) => self.media(vm, &node, *kind, head),
                    None => Err(self.fail(vm, || format!("media class `{wrapper}` not found"))),
                }
            }
            RealizedTy::Uint8Array => Err(self.fail(vm, || {
                "uint8array requires explicit encoding (use from_base64() or from_hex())".into()
            })),
            RealizedTy::Union(members) => self.union(vm, json, members),
            RealizedTy::Literal(literal, _) => self.literal(vm, &node, json, literal),
            RealizedTy::Resource | RealizedTy::PromptAst => {
                Err(self.fail(vm, || "cannot deserialize opaque type".into()))
            }
            // Every JSON value is a value of `unknown`.
            RealizedTy::Unknown => self.copy(vm, json),
            RealizedTy::Function { .. } | RealizedTy::Future(_, _) => {
                Err(self.fail(vm, || format!("no value of `{ty}` is decoded from JSON")))
            }
            // Type-level and opaque types are not valid decode targets:
            // reflection types (`Type`), opaque Rust state (`RustType`), and the
            // bottom type (`Never`).
            RealizedTy::Never | RealizedTy::RustType | RealizedTy::Type => {
                Err(self.fail(vm, || "cannot decode compiler-only type".into()))
            }
        }
    }

    /// Past [`MAX_STRUCTURAL_DEPTH`].
    fn too_deep(&self, vm: &mut BexVm) -> Failed {
        match &self.path {
            Some(path) => Failed::Raised(raise_decode(vm, too_deep_message(), path)),
            None => Failed::Deep,
        }
    }

    // The arms of [`Self::decode`] that recurse are functions of their own, so
    // each level of a deep value costs the stack only what it uses.

    fn list(&mut self, vm: &mut BexVm, node: &Node, element: &RealizedTy) -> Result<Value, Failed> {
        let Node::Array(ptr) = *node else {
            return Err(self.fail(vm, || "expected array".into()));
        };
        let elements = match vm.get_object(ptr) {
            Object::Array(array) => array.to_vec(),
            _ => unreachable!("an array node is an array"),
        };
        let mut items = Vec::with_capacity(elements.len());
        for (i, item) in elements.into_iter().enumerate() {
            let item = self.within(
                |path| {
                    use std::fmt::Write;
                    let _ = write!(path, "[{i}]");
                },
                |this| this.decode(vm, item, element),
            )?;
            items.push(item);
        }
        Ok(Value::object(
            vm.alloc(Object::Array(Array::new(element.clone(), items))),
        ))
    }

    fn map(
        &mut self,
        vm: &mut BexVm,
        node: &Node,
        key: &RealizedTy,
        value: &RealizedTy,
    ) -> Result<Value, Failed> {
        if !baml_type::normalize::is_string_key(key.as_ty(), vm) {
            return Err(self.fail(vm, || STRING_MAP_REQUIRED.into()));
        }
        let Node::Object(ptr) = *node else {
            return Err(self.fail(vm, || "expected object".into()));
        };
        if !object_string_keyed(vm, ptr) {
            return Err(self.fail(vm, || STRING_MAP_REQUIRED.into()));
        }
        let any_key = matches!(key, RealizedTy::String);
        let mut entries = IndexMap::new();
        for (written, item) in super::string_entries(vm, ptr) {
            if !any_key && !super::key_fits(vm, &written, key) {
                return Err(self.fail(vm, || "map key does not match declared key type".into()));
            }
            let item = self.within(
                |path| {
                    use std::fmt::Write;
                    let _ = write!(path, "[{:?}]", written.as_str());
                },
                |this| this.decode(vm, item, value),
            )?;
            entries.insert(written, item);
        }
        Ok(Value::object(vm.alloc_map(
            key.clone(),
            value.clone(),
            entries,
        )))
    }

    /// A recursive alias decodes as its definition. Unfolding it takes no
    /// JSON, so the members are gathered once, here.
    fn alias(&mut self, vm: &mut BexVm, json: Value, ty: &RealizedTy) -> Result<Value, Failed> {
        let mut members = Vec::new();
        super::alias_members(vm, ty, &mut Vec::new(), &mut members);
        match members.len() {
            0 => Err(self.fail(vm, || format!("no value has type `{ty}`"))),
            1 => self.decode(vm, json, &members[0]),
            _ => self.union(vm, json, &members),
        }
    }

    /// Try each member structurally; first match wins.
    fn union(
        &mut self,
        vm: &mut BexVm,
        json: Value,
        members: &[RealizedTy],
    ) -> Result<Value, Failed> {
        for member in members {
            let mut probe = Structural {
                plans: &mut *self.plans,
                path: None,
                depth: self.depth,
            };
            match probe.decode(vm, json, member) {
                Ok(value) => return Ok(value),
                Err(Failed::Deep) => return Err(self.too_deep(vm)),
                Err(Failed::Quiet | Failed::Raised(_)) => {}
            }
        }
        Err(self.fail(vm, || "no union member matched".into()))
    }

    fn literal(
        &mut self,
        vm: &mut BexVm,
        node: &Node,
        json: Value,
        literal: &baml_type::Literal,
    ) -> Result<Value, Failed> {
        match literal {
            baml_type::Literal::Bool(expected) => match node {
                Node::Bool(actual) if actual == expected => Ok(Value::bool(*actual)),
                _ => Err(self.fail(vm, || "literal mismatch".into())),
            },
            baml_type::Literal::String(expected) => {
                if node.text(vm) != Some(expected.as_str()) {
                    return Err(self.fail(vm, || "literal mismatch".into()));
                }
                match node {
                    Node::String(_) => Ok(json),
                    _ => Ok(Value::object(vm.alloc_string(expected.as_str()))),
                }
            }
            baml_type::Literal::Int(expected) if node.is_number() => match node.as_i64() {
                Some(actual) if actual == *expected => Ok(Value::int(actual)),
                _ => Err(self.fail(vm, || "literal int mismatch".into())),
            },
            baml_type::Literal::Float(expected) if node.is_number() => {
                match (expected.parse::<f64>(), node.as_f64()) {
                    (Ok(expected), Some(actual)) if (expected - actual).abs() < f64::EPSILON => {
                        Ok(Value::object(vm.alloc_float(actual)))
                    }
                    _ => Err(self.fail(vm, || "literal float mismatch".into())),
                }
            }
            baml_type::Literal::Bigint(expected) => {
                let actual = if node.is_number() || node.text(vm).is_some() {
                    node.as_bigint(vm)
                } else {
                    None
                };
                let Some(actual) = actual else {
                    return Err(self.fail(vm, || {
                        "expected integral JSON number or numeric string".into()
                    }));
                };
                if actual != *expected {
                    return Err(self.fail(vm, || "literal bigint mismatch".into()));
                }
                Ok(vm
                    .try_alloc_bigint(Arc::new(actual))
                    .map_err(VmRustFnError::from)?)
            }
            baml_type::Literal::Int(_) | baml_type::Literal::Float(_) => {
                Err(self.fail(vm, || "literal mismatch".into()))
            }
        }
    }

    fn enumeration(
        &mut self,
        vm: &mut BexVm,
        node: &Node,
        head: TypeHead,
    ) -> Result<Value, Failed> {
        let named = || baml_type::HeadDisplay::head_display_name(&head);
        let Some(variant) = node.text(vm) else {
            return Err(self.fail(vm, || "expected enum variant string".into()));
        };
        match super::variant_named(vm, head, variant, self.naming()) {
            Ok(Some(index)) => Ok(Value::object(vm.alloc_variant(head.ptr(), index))),
            Ok(None) => {
                let variant = variant.to_owned();
                Err(self.fail(vm, || {
                    format!("unknown variant `{variant}` for enum `{}`", named())
                }))
            }
            Err(()) => Err(self.fail(vm, || format!("`{}` is not an enum", named()))),
        }
    }

    fn enum_variant(
        &mut self,
        vm: &mut BexVm,
        node: &Node,
        head: TypeHead,
        name: &baml_type::Name,
    ) -> Result<Value, Failed> {
        let named = || baml_type::HeadDisplay::head_display_name(&head);
        let expected = match vm.get_object(head.ptr()) {
            Object::Enum(e) => e
                .variants
                .iter()
                .find(|v| v.name == name.as_str())
                .map(|v| {
                    (
                        self.naming().shows(v.skip),
                        self.naming().key(&v.name, v.alias.as_deref()).to_owned(),
                    )
                }),
            _ => return Err(self.fail(vm, || format!("`{}` is not an enum", named()))),
        };
        match expected {
            Some((true, expected)) if node.text(vm) == Some(expected.as_str()) => {
                match super::variant_named(vm, head, &expected, self.naming()) {
                    Ok(Some(index)) => Ok(Value::object(vm.alloc_variant(head.ptr(), index))),
                    _ => Err(self.fail(vm, || {
                        format!("unknown variant `{expected}` for enum `{}`", named())
                    })),
                }
            }
            Some((true, expected)) => {
                Err(self.fail(vm, || format!("expected enum variant `{expected}`")))
            }
            Some((false, _)) => Err(self.fail(vm, || {
                format!(
                    "enum variant `{}.{name}` is `@skip`, so JSON has no name for it",
                    named()
                )
            })),
            None => Err(self.fail(vm, || format!("enum `{}` has no variant `{name}`", named()))),
        }
    }

    fn class(
        &mut self,
        vm: &mut BexVm,
        node: &Node,
        head: TypeHead,
        type_args: &[RealizedTy],
    ) -> Result<Value, Failed> {
        let Node::Object(ptr) = *node else {
            return Err(self.fail(vm, || {
                format!(
                    "expected JSON object for class `{}`",
                    baml_type::HeadDisplay::head_display_name(&head)
                )
            }));
        };
        if !object_string_keyed(vm, ptr) {
            return Err(self.fail(vm, || STRING_MAP_REQUIRED.into()));
        }
        let plan = match self.plans.class(vm, head, type_args) {
            Ok(plan) => plan,
            Err(named) => return Err(self.fail(vm, || format!("`{named}` is not a class"))),
        };
        // The naming decides each field's key, and whether JSON carries the
        // field at all. A field JSON leaves out (`@skip` under aliased names)
        // holds its type's empty value, as it does when SAP reads a model's
        // answer. A field that can be `null` may be absent and decodes to
        // `null`; every other field must be present.
        let mut values = Vec::with_capacity(plan.fields.len());
        for (index, field) in plan.fields.iter().enumerate() {
            if !field.shown {
                let empty = match super::super::sap::empty_value(vm, &field.ty, &mut Vec::new()) {
                    Ok(Some(empty)) => empty,
                    Ok(None) => {
                        let message = skipped_without_empty(vm, &plan, index);
                        return Err(self.fail(vm, || message));
                    }
                    Err(error) if self.path.is_some() => return Err(Failed::Raised(error)),
                    Err(_) => return Err(Failed::Quiet),
                };
                values.push(empty);
                continue;
            }
            let value = self.within(
                |path| {
                    path.push('.');
                    path.push_str(&field.key);
                },
                |this| match super::object_get(vm, ptr, field.hash(), &field.key) {
                    Some(json) => this.decode(vm, json, &field.ty),
                    None if field.may_be_absent => this.decode(vm, Value::NULL, &field.ty),
                    None => {
                        Err(this.fail(vm, || format!("missing required field `{}`", field.key)))
                    }
                },
            )?;
            values.push(value);
        }
        Ok(Value::object(vm.alloc(Object::Instance(Instance::new(
            plan.class,
            plan.type_args.clone(),
            values,
        )))))
    }

    fn media(
        &mut self,
        vm: &mut BexVm,
        node: &Node,
        kind: MediaKind,
        head: TypeHead,
    ) -> Result<Value, Failed> {
        let Node::Object(ptr) = *node else {
            return Err(self.fail(vm, || {
                "expected tagged media object {kind, source, value, mime}".into()
            }));
        };
        if !object_string_keyed(vm, ptr) {
            return Err(self.fail(vm, || STRING_MAP_REQUIRED.into()));
        }
        let read = |vm: &BexVm, key: &str| {
            let value = super::object_get(vm, ptr, bex_vm_types::map_string_hash(key), key)?;
            node_of(vm, value).text(vm).map(str::to_owned)
        };
        let fields = MediaFields {
            kind: read(vm, "kind"),
            source: read(vm, "source"),
            value: read(vm, "value"),
            mime: read(vm, "mime"),
            name: read(vm, "name"),
        };
        super::build_media(vm, &fields, kind, head).map_err(|message| self.fail(vm, || message))
    }

    /// `json` copied as a `json` value (see [`super::copy_json`]).
    fn copy(&mut self, vm: &mut BexVm, json: Value) -> Result<Value, Failed> {
        match super::copy_json(vm, json) {
            Ok(value) => Ok(value),
            Err((message, inner)) => Err(match &self.path {
                Some(path) => Failed::Raised(raise_decode(vm, message, &format!("{path}{inner}"))),
                None => Failed::Quiet,
            }),
        }
    }
}

/// Whether the map at `ptr` has string keys.
fn object_string_keyed(vm: &BexVm, ptr: HeapPtr) -> bool {
    match vm.get_object(ptr) {
        Object::Map(map) => super::string_keyed(vm, map),
        _ => false,
    }
}
