//! Writing BAML values as JSON: `baml.json.from`, `baml.json.stringify`, and
//! the `baml.ToJson` interface default body.
//!
//! All three render a value honoring `baml.ToJson` overrides at every depth:
//! the value's own override (if any) wins, and any *nested* value whose
//! runtime class overrides `to_json` is rendered by that override rather than
//! structurally. The json analog of `string.from` / `baml.ToString` in
//! `root.rs`.
//!
//! One walk does the work, keeping the containers it is inside on a stack of
//! its own, so a deep value never deepens the Rust stack. It writes text
//! straight from the VM values (`stringify`) or builds the `json` value
//! directly (`from`). Most values hold no override, so the walk first runs
//! as one pass that checks each value for an override as it goes. If it meets
//! one, or an error, it falls back to the three passes overrides have always
//! had, so overrides run, and errors surface, in the order they always have:
//!   1. collect — pre-order, every sub-value whose runtime class overrides
//!      `baml.ToJson`, not descending into it;
//!   2. one `YieldToCall` per override, dispatching its `to_json`;
//!   3. the walk again, splicing each override's result in by pre-order
//!      position.
//!
//! `WriteOptions` (`naming`, `omit_nulls`) shape only the structural walk. An
//! override's result is spliced in as it came back, so the collect pass must
//! skip exactly the fields the walk skips, or the pre-order positions of the
//! two drift apart.
//!
//! JSON text has no NaN or infinity, so `stringify` refuses a float that is
//! not finite, wherever it is. A `json` value is in memory and holds one as
//! it is, so `from` keeps it.

use bex_heap::TlabHolder;
use bex_vm_types::{
    HeapPtr, MapData, Object, RealizedTy, RootHaver, RootVisitor, TypeHead, Value, ValueKind,
    types::{Array, Map},
};
use rustc_hash::FxHashMap;
use sys_types::JsonNaming;

use super::{OpenSet, STRING_MAP_REQUIRED, raise_serialize};
use crate::{
    BexVm,
    errors::{VmInternalError, VmRustFnError},
    package_baml::{Continuation, NativeCallResult, make_to_json_callee},
};

/// How the structural walk writes a value: the names fields and variants go
/// by, and whether a class field holding `null` is written. A `baml.ToJson`
/// override never sees these: it owns its whole subtree.
#[derive(Clone, Copy)]
pub(super) struct WriteOptions {
    pub(super) naming: JsonNaming,
    pub(super) omit_nulls: bool,
}

impl WriteOptions {
    /// What the `baml.ToJson` default body writes: declared names, every field.
    pub(super) const DEFAULT: Self = Self {
        naming: JsonNaming::Declared,
        omit_nulls: false,
    };

    /// Whether the walk writes the class field `field` holding `value`.
    fn writes_field(self, field: &bex_vm_types::ClassField, value: Value) -> bool {
        self.naming.shows(field.skip) && !(self.omit_nulls && value.is_null())
    }
}

/// What the walk produces.
pub(super) enum Finish {
    /// A `json` value.
    Json,
    /// JSON text. `indent` is written once per nesting level; empty is compact.
    Text { indent: String },
}

/// The message for a value that contains itself, which no JSON can hold.
const CYCLIC_VALUE: &str = "value contains itself, so it has no JSON form";

/// Render `value` as `finish` says, honoring overrides.
pub(super) fn render_json(
    vm: &mut BexVm,
    value: Value,
    options: WriteOptions,
    finish: Finish,
) -> NativeCallResult {
    let mut cache = ToJsonCache::default();
    let stopped = match write(
        vm,
        value,
        options,
        &finish,
        &mut Overrides::Probe(&mut cache),
    ) {
        Ok(done) => return NativeCallResult::Done(done),
        Err(stopped) => stopped,
    };
    let pending = match (collect(vm, value, options, &mut cache), stopped) {
        (Err(error), _) => return NativeCallResult::Error(error.into()),
        // Every override runs before a structural error surfaces, so the
        // error stands only when there is no override at all.
        (Ok(pending), Stopped::Error(error)) if pending.is_empty() => {
            return NativeCallResult::Error(error);
        }
        (Ok(pending), _) => pending,
    };
    ToJsonWalkContinuation {
        root: value,
        options,
        finish,
        pending,
        results: Vec::new(),
    }
    .next(vm)
}

/// Structural JSON text for `value`, ignoring `baml.ToJson` overrides: the
/// fallback a CSV markdown cell uses for a nested value. A float that is not
/// finite is written `null`, as a cell always has.
pub(in crate::package_baml) fn structural_json_text(
    vm: &mut BexVm,
    value: Value,
) -> Result<String, VmRustFnError> {
    let mut sink = TextSink::new("", NonFinite::Null);
    match walk(
        vm,
        value,
        WriteOptions::DEFAULT,
        &mut Overrides::Ignore,
        &mut sink,
    ) {
        Ok(()) => Ok(sink.into_string()),
        Err(Stopped::Error(error)) => Err(error),
        Err(Stopped::Override) => unreachable!("overrides are ignored"),
    }
}

/// One walk, finishing as `finish` says.
fn write(
    vm: &mut BexVm,
    value: Value,
    options: WriteOptions,
    finish: &Finish,
    overrides: &mut Overrides<'_>,
) -> Result<Value, Stopped> {
    match finish {
        Finish::Json => {
            let mut sink = ValueSink::new(super::json_alias_ty(vm));
            walk(vm, value, options, overrides, &mut sink)?;
            Ok(sink.root.unwrap_or(Value::NULL))
        }
        Finish::Text { indent } => {
            let mut sink = TextSink::new(indent, NonFinite::Refuse);
            walk(vm, value, options, overrides, &mut sink)?;
            Ok(Value::object(vm.alloc_string(sink.into_string())))
        }
    }
}

// ─── Overrides ───────────────────────────────────────────────────────────────

/// What the walk does at a value whose runtime class overrides `to_json`.
enum Overrides<'a> {
    /// Nothing: the walk is structural.
    Ignore,
    /// Stop: the caller runs the passes.
    Probe(&'a mut ToJsonCache),
    /// Splice the override's result in. `pending` is every override-bearing
    /// value in pre-order (pass 1), `results` what each `to_json` returned
    /// (pass 2); `next` is the next one to meet.
    Splice {
        pending: &'a [HeapPtr],
        results: &'a [Value],
        next: usize,
    },
}

/// Why a walk stopped.
enum Stopped {
    /// [`Overrides::Probe`] met an override.
    Override,
    Error(VmRustFnError),
}

impl From<VmRustFnError> for Stopped {
    fn from(error: VmRustFnError) -> Self {
        Self::Error(error)
    }
}

/// Whether each runtime type carries a `baml.ToJson` override, as one call
/// has found it. Resolution depends on the value's type (its class and type
/// arguments, or a container's element types) and on the calling package,
/// which is the same for the whole call.
#[derive(Default)]
struct ToJsonCache {
    found: FxHashMap<OverrideKey, bool>,
}

#[derive(PartialEq, Eq, Hash)]
enum OverrideKey {
    Float,
    String,
    Bigint,
    Uint8Array,
    Instance(TypeHead, Box<[RealizedTy]>),
    Variant(TypeHead),
    List(RealizedTy),
    Map(RealizedTy, RealizedTy),
}

impl ToJsonCache {
    /// Whether `value`'s runtime type carries a `baml.ToJson` override. A
    /// resolver error PROPAGATES (`shim_rule_method`'s own contract: never a
    /// silent structural fallback).
    fn has_override(&mut self, vm: &BexVm, value: Value) -> Result<bool, VmInternalError> {
        let Some(ptr) = value.as_object_ptr() else {
            return Ok(false);
        };
        let key = match vm.get_object(ptr) {
            Object::Float(_) => OverrideKey::Float,
            Object::String(_) => OverrideKey::String,
            Object::Bigint(_) => OverrideKey::Bigint,
            Object::Uint8Array(_) => OverrideKey::Uint8Array,
            Object::Array(array) => OverrideKey::List((*array.element_ty).clone()),
            Object::Map(map) => OverrideKey::Map((*map.key_ty).clone(), (*map.value_ty).clone()),
            Object::Instance(instance) => match vm.get_object(instance.class) {
                Object::Class(class) => OverrideKey::Instance(
                    TypeHead::new(instance.class, class.type_tag),
                    instance.class_type_args.clone(),
                ),
                _ => return has_to_json_override(vm, value),
            },
            Object::Variant(variant) => match vm.get_object(variant.enm) {
                Object::Enum(enm) => OverrideKey::Variant(TypeHead::new(variant.enm, enm.type_tag)),
                _ => return has_to_json_override(vm, value),
            },
            _ => return has_to_json_override(vm, value),
        };
        if let Some(&found) = self.found.get(&key) {
            return Ok(found);
        }
        let found = has_to_json_override(vm, value)?;
        self.found.insert(key, found);
        Ok(found)
    }
}

/// Whether `value`'s runtime type carries a `baml.ToJson` override. Shares
/// `make_to_json_callee`'s rule resolution but allocates nothing on the VM
/// heap.
fn has_to_json_override(vm: &BexVm, value: Value) -> Result<bool, VmInternalError> {
    Ok(matches!(
        super::super::shim_rule_method(vm, value, crate::lang_items::BAML_TO_JSON, "to_json")?,
        Some(resolved) if !resolved.is_default
    ))
}

/// Pass 1: every sub-value of `value` whose runtime class overrides
/// `baml.ToJson`, by heap pointer and in pre-order. An override node is
/// recorded and *not* descended into — its `to_json` owns its whole subtree.
/// Matches the walk's order (array elements, then map values, then the
/// instance fields `options` writes) so the two stay index-aligned. A media
/// instance is a leaf, as the walk writes its tagged form without descending
/// into its opaque `_data` field. A container met again inside itself is not
/// descended into again: the walk refuses it.
fn collect(
    vm: &BexVm,
    value: Value,
    options: WriteOptions,
    cache: &mut ToJsonCache,
) -> Result<Vec<HeapPtr>, VmInternalError> {
    let mut pending = Vec::new();
    // Each open container with its children and the next one to visit.
    let mut open: Vec<(HeapPtr, Vec<Value>, usize)> = Vec::new();
    let mut inside = OpenSet::default();
    let mut next = Some(value);
    loop {
        if let Some(value) = next.take()
            && let Some(ptr) = value.as_object_ptr()
        {
            if cache.has_override(vm, value)? {
                pending.push(ptr);
            } else if !inside.contains(ptr) {
                let children = children(vm, ptr, options);
                if !children.is_empty() {
                    inside.insert(ptr);
                    open.push((ptr, children, 0));
                }
            }
        }
        let Some((ptr, children, cursor)) = open.last_mut() else {
            return Ok(pending);
        };
        match children.get(*cursor) {
            Some(child) => {
                *cursor += 1;
                next = Some(*child);
            }
            None => {
                inside.remove(*ptr);
                open.pop();
            }
        }
    }
}

/// The children of the value at `ptr` the walk descends into, in its order.
fn children(vm: &BexVm, ptr: HeapPtr, options: WriteOptions) -> Vec<Value> {
    match vm.get_object(ptr) {
        Object::Array(values) => values.to_vec(),
        Object::Map(map) => map
            .snapshot_entries()
            .into_iter()
            .map(|(_, value)| value)
            .collect(),
        Object::Instance(instance) => match vm.get_object(instance.class) {
            Object::Class(class)
                if vm
                    .stdlib_heads()
                    .media_kind(TypeHead::new(instance.class, class.type_tag))
                    .is_none() =>
            {
                instance
                    .field_values()
                    .zip(&class.fields)
                    .filter(|(value, field)| options.writes_field(field, *value))
                    .map(|(value, _)| value)
                    .collect()
            }
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Drives passes 2 and 3: accumulates each override's `to_json` result,
/// dispatches the next, and once all are in, walks the value splicing them
/// in. The number of results gathered so far IS the index of the next
/// override to dispatch.
struct ToJsonWalkContinuation {
    /// The value being rendered.
    root: Value,
    /// How the structural walk writes it.
    options: WriteOptions,
    /// What the walk produces.
    finish: Finish,
    /// Override-bearing sub-values, in pre-order (pass 1).
    pending: Vec<HeapPtr>,
    /// What each override returned so far: `json` values, rooted while the
    /// next override runs.
    results: Vec<Value>,
}

impl ToJsonWalkContinuation {
    /// Dispatch the next override, or walk once every result is in. Every
    /// pending pointer was collected as an override by pass 1, so a pass-2
    /// miss is a skew between the two passes, not a fallback case.
    fn next(self, vm: &mut BexVm) -> NativeCallResult {
        if let Some(&ptr) = self.pending.get(self.results.len()) {
            return match make_to_json_callee(vm, Value::object(ptr)) {
                Err(error) => NativeCallResult::Error(error.into()),
                Ok(Some(callee)) => NativeCallResult::YieldToCall {
                    callee,
                    args: vec![],
                    type_args: vec![],
                    continuation: Box::new(self),
                },
                Ok(None) => NativeCallResult::Error(
                    VmInternalError::OverrideWalkSkew {
                        interface: "ToJson",
                    }
                    .into(),
                ),
            };
        }
        let mut overrides = Overrides::Splice {
            pending: &self.pending,
            results: &self.results,
            next: 0,
        };
        match write(vm, self.root, self.options, &self.finish, &mut overrides) {
            Ok(done) => NativeCallResult::Done(done),
            Err(Stopped::Error(error)) => NativeCallResult::Error(error),
            Err(Stopped::Override) => unreachable!("a splicing walk does not stop at overrides"),
        }
    }
}

impl RootHaver for ToJsonWalkContinuation {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        let Self {
            root,
            options: _,
            finish: _,
            pending,
            results,
        } = self;
        root.visit_roots(visitor);
        pending.visit_roots(visitor);
        results.visit_roots(visitor);
    }
}

impl Continuation for ToJsonWalkContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        self.results.push(value);
        self.next(vm)
    }
}

// ─── The walk ────────────────────────────────────────────────────────────────

/// What a sink does with a float that is not finite.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NonFinite {
    /// Holds it as it is: a `json` value can.
    Keep,
    /// Writes `null` instead.
    Null,
    /// Refuses it: JSON text has no NaN or infinity.
    Refuse,
}

/// Where a JSON value goes: text, or a `json` value.
trait Sink {
    /// What the sink does with a float that is not finite.
    fn non_finite(&self) -> NonFinite;
    fn null(&mut self, vm: &mut BexVm);
    fn bool(&mut self, vm: &mut BexVm, b: bool);
    fn int(&mut self, vm: &mut BexVm, i: i64);
    /// A float object: a finite one, or one the sink keeps.
    fn float(&mut self, vm: &mut BexVm, value: Value, f: f64);
    /// A bigint object.
    fn bigint(&mut self, vm: &mut BexVm, value: Value);
    /// A string object.
    fn string(&mut self, vm: &mut BexVm, value: Value);
    /// A string the walk spells itself.
    fn text(&mut self, vm: &mut BexVm, text: &str);
    fn begin_array(&mut self, vm: &mut BexVm);
    /// Before each element; `first` is whether it is the first.
    fn item(&mut self, vm: &mut BexVm, first: bool);
    fn end_array(&mut self, vm: &mut BexVm, empty: bool);
    fn begin_object(&mut self, vm: &mut BexVm);
    /// Before each value of an object.
    fn key(&mut self, vm: &mut BexVm, first: bool, key: Key<'_>);
    fn end_object(&mut self, vm: &mut BexVm, empty: bool);
}

/// An object key.
#[derive(Clone, Copy)]
enum Key<'k> {
    /// A map's key: a string object.
    Value(Value),
    /// A class field's JSON key: field `index` of `class`.
    Field {
        class: HeapPtr,
        index: usize,
        text: &'k str,
    },
    /// A key the walk spells itself.
    Text(&'static str),
}

/// A container the walk is inside.
struct Frame {
    /// The container, for the cycle check; null for a media object, which is
    /// written whole.
    ptr: HeapPtr,
    kind: FrameKind,
    children: Vec<(Option<Slot>, Value)>,
    next: usize,
    /// Inside an override's result, written as it came back.
    spliced: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    Array,
    Object,
}

/// Where a child sits in its container.
#[derive(Clone, Copy)]
enum Slot {
    /// A map key, a string object.
    Key(Value),
    /// Field `index` of the frame's class.
    Field(HeapPtr, usize),
}

/// What the walk knows about a class: each field's JSON key under the
/// naming, and whether two fields share one.
struct WritePlan {
    keys: Vec<String>,
    shared_keys: bool,
}

/// What the walk does with a heap value.
enum Shape {
    Float(f64),
    String,
    Bigint,
    Array(Vec<Value>),
    Map,
    Instance(HeapPtr),
    Variant(HeapPtr, usize),
    /// No JSON form: the message, and the `SerializationError.reason` tag.
    Unserializable(&'static str, &'static str),
}

/// What the walk does with a heap value inside an override's result.
enum JsonShape {
    Float(f64),
    String,
    Bigint,
    Array(Vec<Value>),
    Map,
    Variant(Option<String>),
    Null,
}

/// The walk's state.
struct Walker<'o, 'a> {
    options: WriteOptions,
    overrides: &'o mut Overrides<'a>,
    frames: Vec<Frame>,
    open: OpenSet,
    plans: FxHashMap<HeapPtr, WritePlan>,
}

/// Write `root` to `sink`.
fn walk(
    vm: &mut BexVm,
    root: Value,
    options: WriteOptions,
    overrides: &mut Overrides<'_>,
    sink: &mut impl Sink,
) -> Result<(), Stopped> {
    let mut walker = Walker {
        options,
        overrides,
        frames: Vec::new(),
        open: OpenSet::default(),
        plans: FxHashMap::default(),
    };
    walker.visit(vm, root, false, sink)?;
    loop {
        let Some(frame) = walker.frames.last_mut() else {
            return Ok(());
        };
        let Some(&(slot, child)) = frame.children.get(frame.next) else {
            let frame = walker
                .frames
                .pop()
                .unwrap_or_else(|| unreachable!("the frame was just read"));
            if !frame.ptr.is_null() {
                walker.open.remove(frame.ptr);
            }
            match frame.kind {
                FrameKind::Array => sink.end_array(vm, frame.children.is_empty()),
                FrameKind::Object => sink.end_object(vm, frame.children.is_empty()),
            }
            continue;
        };
        let first = frame.next == 0;
        frame.next += 1;
        let spliced = frame.spliced;
        match slot {
            None => sink.item(vm, first),
            Some(Slot::Key(key)) => sink.key(vm, first, Key::Value(key)),
            Some(Slot::Field(class, index)) => {
                let text = &walker.plans[&class].keys[index];
                sink.key(vm, first, Key::Field { class, index, text });
            }
        }
        walker.visit(vm, child, spliced, sink)?;
    }
}

impl Walker<'_, '_> {
    /// The path of the child the innermost container is on, in the notation
    /// `SerializationError.path` uses.
    fn path(&self, vm: &BexVm) -> String {
        use std::fmt::Write;
        let mut path = String::new();
        for frame in &self.frames {
            let Some((slot, _)) = frame
                .next
                .checked_sub(1)
                .and_then(|i| frame.children.get(i))
            else {
                continue;
            };
            let _ = match slot {
                None => write!(path, "[{}]", frame.next - 1),
                Some(Slot::Key(key)) => {
                    let key = vm
                        .as_string(key)
                        .map(bex_vm_types::BexStr::as_str)
                        .unwrap_or_default();
                    write!(path, "[{key:?}]")
                }
                Some(Slot::Field(class, index)) => {
                    write!(path, ".{}", self.plans[class].keys[*index])
                }
            };
        }
        path
    }

    fn refuse(&self, vm: &mut BexVm, message: impl Into<String>, reason: &str) -> Stopped {
        let path = self.path(vm);
        Stopped::Error(raise_serialize(vm, message, &path, reason))
    }

    /// Write `value`, or open it to write its children. `spliced` is whether
    /// it is (inside) an override's result, written as it came back.
    fn visit(
        &mut self,
        vm: &mut BexVm,
        value: Value,
        spliced: bool,
        sink: &mut impl Sink,
    ) -> Result<(), Stopped> {
        let ptr = match value.kind() {
            ValueKind::Null | ValueKind::OmittedArg => {
                sink.null(vm);
                return Ok(());
            }
            ValueKind::Bool(b) => {
                sink.bool(vm, b);
                return Ok(());
            }
            ValueKind::Int(i) => {
                sink.int(vm, i);
                return Ok(());
            }
            ValueKind::Object(ptr) => ptr,
        };
        if !spliced {
            match self.overrides {
                Overrides::Ignore => {}
                Overrides::Probe(cache) => {
                    if cache.has_override(vm, value).map_err(VmRustFnError::from)? {
                        return Err(Stopped::Override);
                    }
                }
                Overrides::Splice {
                    pending,
                    results,
                    next,
                } => {
                    // Override node: splice its `to_json` result in.
                    if pending.get(*next) == Some(&ptr) {
                        let result = results.get(*next).copied().unwrap_or(Value::NULL);
                        *next += 1;
                        return self.visit(vm, result, true, sink);
                    }
                }
            }
        }
        if spliced {
            return self.visit_json(vm, value, ptr, sink);
        }
        let shape = match vm.get_object(ptr) {
            Object::Float(f) => Shape::Float(*f),
            Object::String(_) => Shape::String,
            Object::Bigint(_) => Shape::Bigint,
            Object::Array(values) => Shape::Array(values.to_vec()),
            Object::Map(_) => Shape::Map,
            Object::Instance(instance) => Shape::Instance(instance.class),
            Object::Variant(variant) => Shape::Variant(variant.enm, variant.index),
            Object::Uint8Array(_) => Shape::Unserializable(
                "uint8array requires explicit encoding (use to_base64() or to_hex())",
                "uint8array",
            ),
            Object::Function(_)
            | Object::Closure(_)
            | Object::BoundMethod(_)
            | Object::GenericFunction(_)
            | Object::HostClosure(_) => {
                Shape::Unserializable("a function has no JSON form", "function")
            }
            Object::Future(_) => {
                Shape::Unserializable("a future has no JSON form (await it first)", "future")
            }
            // Its state lives in the runtime, which gives it no form outside.
            Object::RustInstance(_) => Shape::Unserializable(
                "a value whose state lives in the runtime has no JSON form",
                "unserializable",
            ),
            _ => Shape::Unserializable("value has no json representation", "unserializable"),
        };
        match shape {
            Shape::Float(f) => self.float(vm, value, f, sink)?,
            Shape::String => sink.string(vm, value),
            Shape::Bigint => sink.bigint(vm, value),
            Shape::Array(values) => self.open_array(vm, ptr, values, false, sink)?,
            Shape::Map => self.open_map(vm, ptr, false, sink)?,
            Shape::Instance(class) => self.open_instance(vm, value, ptr, class, sink)?,
            Shape::Variant(enm, index) => {
                let name = match vm.get_object(enm) {
                    Object::Enum(e) => match e.variants.get(index) {
                        Some(v) if self.options.naming.shows(v.skip) => Ok(self
                            .options
                            .naming
                            .key(&v.name, v.alias.as_deref())
                            .to_owned()),
                        Some(v) => Err(format!("{}.{}", e.name(), v.name)),
                        None => Ok(String::new()),
                    },
                    _ => {
                        return Err(self.refuse(
                            vm,
                            "enum variant points to non-enum object",
                            "enum",
                        ));
                    }
                };
                match name {
                    Ok(name) => sink.text(vm, &name),
                    Err(skipped) => {
                        return Err(self.refuse(
                            vm,
                            format!(
                                "enum variant `{skipped}` is `@skip`, so JSON has no name for it"
                            ),
                            "enum",
                        ));
                    }
                }
            }
            Shape::Unserializable(message, reason) => {
                return Err(self.refuse(vm, message, reason));
            }
        }
        Ok(())
    }

    /// Write the float object `value`, holding `f`, as the sink takes it.
    fn float(
        &self,
        vm: &mut BexVm,
        value: Value,
        f: f64,
        sink: &mut impl Sink,
    ) -> Result<(), Stopped> {
        if f.is_finite() {
            sink.float(vm, value, f);
            return Ok(());
        }
        match sink.non_finite() {
            NonFinite::Keep => sink.float(vm, value, f),
            NonFinite::Null => sink.null(vm),
            NonFinite::Refuse => {
                return Err(self.refuse(
                    vm,
                    format!("{f} has no JSON form: a JSON number is finite"),
                    "non_finite",
                ));
            }
        }
        Ok(())
    }

    /// Write `value` (at `ptr`), part of an override's result, as a `json`
    /// value: a value with no JSON form inside it is `null`, and an enum
    /// variant is its declared name.
    fn visit_json(
        &mut self,
        vm: &mut BexVm,
        value: Value,
        ptr: HeapPtr,
        sink: &mut impl Sink,
    ) -> Result<(), Stopped> {
        let shape = match vm.get_object(ptr) {
            Object::Float(f) => JsonShape::Float(*f),
            Object::String(_) => JsonShape::String,
            Object::Bigint(_) => JsonShape::Bigint,
            Object::Array(values) => JsonShape::Array(values.to_vec()),
            Object::Map(_) => JsonShape::Map,
            Object::Variant(variant) => JsonShape::Variant(match vm.get_object(variant.enm) {
                Object::Enum(e) => e.variants.get(variant.index).map(|v| v.name.clone()),
                _ => None,
            }),
            Object::Tombstone => Object::tombstone_reached(),
            _ => JsonShape::Null,
        };
        match shape {
            JsonShape::Float(f) => self.float(vm, value, f, sink)?,
            JsonShape::Null | JsonShape::Variant(None) => sink.null(vm),
            JsonShape::String => sink.string(vm, value),
            JsonShape::Bigint => sink.bigint(vm, value),
            JsonShape::Array(values) => self.open_array(vm, ptr, values, true, sink)?,
            JsonShape::Map => self.open_map(vm, ptr, true, sink)?,
            JsonShape::Variant(Some(name)) => sink.text(vm, &name),
        }
        Ok(())
    }

    /// Enter the container at `ptr`. A container the walk is already inside
    /// is a value that contains itself.
    fn push(
        &mut self,
        vm: &mut BexVm,
        ptr: HeapPtr,
        kind: FrameKind,
        children: Vec<(Option<Slot>, Value)>,
        spliced: bool,
    ) -> Result<(), Stopped> {
        if self.open.contains(ptr) {
            return Err(self.refuse(vm, CYCLIC_VALUE, "cycle"));
        }
        self.open.insert(ptr);
        self.frames.push(Frame {
            ptr,
            kind,
            children,
            next: 0,
            spliced,
        });
        Ok(())
    }

    fn open_array(
        &mut self,
        vm: &mut BexVm,
        ptr: HeapPtr,
        values: Vec<Value>,
        spliced: bool,
        sink: &mut impl Sink,
    ) -> Result<(), Stopped> {
        let children = values.into_iter().map(|value| (None, value)).collect();
        self.push(vm, ptr, FrameKind::Array, children, spliced)?;
        sink.begin_array(vm);
        Ok(())
    }

    fn open_map(
        &mut self,
        vm: &mut BexVm,
        ptr: HeapPtr,
        spliced: bool,
        sink: &mut impl Sink,
    ) -> Result<(), Stopped> {
        let Object::Map(map) = vm.get_object(ptr) else {
            unreachable!("the value was just read as a map")
        };
        if !super::string_keyed(vm, map) {
            return Err(self.refuse(vm, STRING_MAP_REQUIRED, "map_key"));
        }
        let children = map
            .snapshot_entries()
            .into_iter()
            .map(|(key, value)| (Some(Slot::Key(key)), value))
            .collect();
        self.push(vm, ptr, FrameKind::Object, children, spliced)?;
        sink.begin_object(vm);
        Ok(())
    }

    fn open_instance(
        &mut self,
        vm: &mut BexVm,
        value: Value,
        ptr: HeapPtr,
        class: HeapPtr,
        sink: &mut impl Sink,
    ) -> Result<(), Stopped> {
        let (head, plan) = match vm.get_object(class) {
            Object::Class(declaration) => (
                TypeHead::new(class, declaration.type_tag),
                (!self.plans.contains_key(&class)).then(|| {
                    let keys: Vec<String> = declaration
                        .fields
                        .iter()
                        .map(|field| {
                            self.options
                                .naming
                                .key(&field.name, field.alias.as_deref())
                                .to_owned()
                        })
                        .collect();
                    let shared_keys = declaration.fields.iter().enumerate().any(|(i, field)| {
                        self.options.naming.shows(field.skip)
                            && declaration.fields[..i]
                                .iter()
                                .enumerate()
                                .any(|(j, other)| {
                                    self.options.naming.shows(other.skip) && keys[i] == keys[j]
                                })
                    });
                    WritePlan { keys, shared_keys }
                }),
            ),
            _ => {
                return Err(self.refuse(vm, "instance class pointer is not a class", "class"));
            }
        };
        // Media instances write their tagged form, not a field map.
        if let Some(kind) = vm.stdlib_heads().media_kind(head) {
            return self.write_media(vm, value, kind, sink);
        }
        if let Some(plan) = plan {
            self.plans.insert(class, plan);
        }
        let (Object::Instance(instance), Object::Class(declaration)) =
            (vm.get_object(ptr), vm.get_object(class))
        else {
            unreachable!("the instance and its class were just read")
        };
        let mut children: Vec<(Option<Slot>, Value)> = instance
            .field_values()
            .zip(&declaration.fields)
            .enumerate()
            .filter(|(_, (value, field))| self.options.writes_field(field, *value))
            .map(|(index, (value, _))| (Some(Slot::Field(class, index)), value))
            .collect();
        let plan = &self.plans[&class];
        if plan.shared_keys {
            // Two fields under one key write one entry, where the first sits,
            // holding the last value: what a JSON object can say.
            let mut unique: Vec<(Option<Slot>, Value)> = Vec::with_capacity(children.len());
            for (slot, value) in children {
                let Some(Slot::Field(_, index)) = slot else {
                    unreachable!("instance children are fields")
                };
                let seen = unique.iter_mut().find(|(seen, _)| {
                    matches!(seen, Some(Slot::Field(_, i)) if plan.keys[*i] == plan.keys[index])
                });
                match seen {
                    Some(entry) => entry.1 = value,
                    None => unique.push((slot, value)),
                }
            }
            children = unique;
        }
        self.push(vm, ptr, FrameKind::Object, children, false)?;
        sink.begin_object(vm);
        Ok(())
    }

    /// The tagged object for a media value: `{ "kind": "image"|..., "source":
    /// "url"|"base64", "value": <data>, "mime": <mime_type-or-null> }`, with
    /// `"name": <name>` when the value has one. `value` is the URL or the
    /// base64 payload depending on `source`.
    fn write_media(
        &mut self,
        vm: &mut BexVm,
        value: Value,
        kind: baml_type::MediaKind,
        sink: &mut impl Sink,
    ) -> Result<(), Stopped> {
        let Some(media) = super::read_media_value(vm, value) else {
            return Err(self.refuse(vm, "expected media instance", "media"));
        };
        let (source, payload) = match media.url() {
            Some(url) => ("url", url),
            None => ("base64", media.base64().as_str().to_owned()),
        };
        sink.begin_object(vm);
        sink.key(vm, true, Key::Text("kind"));
        sink.text(vm, kind.tag_str());
        sink.key(vm, false, Key::Text("source"));
        sink.text(vm, source);
        sink.key(vm, false, Key::Text("value"));
        sink.text(vm, &payload);
        sink.key(vm, false, Key::Text("mime"));
        match media.mime_type() {
            Some(mime) => sink.text(vm, &mime),
            None => sink.null(vm),
        }
        if let Some(name) = media.name() {
            sink.key(vm, false, Key::Text("name"));
            sink.text(vm, &name);
        }
        sink.end_object(vm, false);
        Ok(())
    }
}

// ─── Text ────────────────────────────────────────────────────────────────────

/// Writes JSON text: compact, or each nesting level indented by `indent`,
/// exactly as `serde_json`'s compact and pretty formatters do.
struct TextSink<'i> {
    out: Vec<u8>,
    indent: &'i [u8],
    level: usize,
    non_finite: NonFinite,
}

impl<'i> TextSink<'i> {
    fn new(indent: &'i str, non_finite: NonFinite) -> Self {
        Self {
            out: Vec::new(),
            indent: indent.as_bytes(),
            level: 0,
            non_finite,
        }
    }

    fn into_string(self) -> String {
        String::from_utf8(self.out).unwrap_or_else(|e| unreachable!("JSON text is UTF-8: {e}"))
    }

    fn newline(&mut self) {
        self.out.push(b'\n');
        for _ in 0..self.level {
            self.out.extend_from_slice(self.indent);
        }
    }

    fn separator(&mut self, first: bool) {
        if !first {
            self.out.push(b',');
        }
        if !self.indent.is_empty() {
            self.newline();
        }
    }

    fn close(&mut self, empty: bool, bracket: u8) {
        self.level -= 1;
        if !self.indent.is_empty() && !empty {
            self.newline();
        }
        self.out.push(bracket);
    }
}

impl Sink for TextSink<'_> {
    fn non_finite(&self) -> NonFinite {
        self.non_finite
    }

    fn null(&mut self, _: &mut BexVm) {
        self.out.extend_from_slice(b"null");
    }

    fn bool(&mut self, _: &mut BexVm, b: bool) {
        self.out
            .extend_from_slice(if b { b"true" } else { b"false" });
    }

    fn int(&mut self, _: &mut BexVm, i: i64) {
        self.out
            .extend_from_slice(itoa::Buffer::new().format(i).as_bytes());
    }

    fn float(&mut self, _: &mut BexVm, _: Value, f: f64) {
        self.out
            .extend_from_slice(zmij::Buffer::new().format_finite(f).as_bytes());
    }

    fn bigint(&mut self, vm: &mut BexVm, value: Value) {
        if let Some(Object::Bigint(b)) = value.as_object_ptr().map(|ptr| vm.get_object(ptr)) {
            self.out.extend_from_slice(b.to_string().as_bytes());
        }
    }

    fn string(&mut self, vm: &mut BexVm, value: Value) {
        if let Ok(text) = vm.as_string(&value) {
            write_escaped(&mut self.out, text.as_str());
        }
    }

    fn text(&mut self, _: &mut BexVm, text: &str) {
        write_escaped(&mut self.out, text);
    }

    fn begin_array(&mut self, _: &mut BexVm) {
        self.level += 1;
        self.out.push(b'[');
    }

    fn item(&mut self, _: &mut BexVm, first: bool) {
        self.separator(first);
    }

    fn end_array(&mut self, _: &mut BexVm, empty: bool) {
        self.close(empty, b']');
    }

    fn begin_object(&mut self, _: &mut BexVm) {
        self.level += 1;
        self.out.push(b'{');
    }

    fn key(&mut self, vm: &mut BexVm, first: bool, key: Key<'_>) {
        self.separator(first);
        match key {
            Key::Value(value) => self.string(vm, value),
            Key::Field { text, .. } => write_escaped(&mut self.out, text),
            Key::Text(text) => write_escaped(&mut self.out, text),
        }
        self.out
            .extend_from_slice(if self.indent.is_empty() { b":" } else { b": " });
    }

    fn end_object(&mut self, _: &mut BexVm, empty: bool) {
        self.close(empty, b'}');
    }
}

/// `text` as a JSON string, escaped as `serde_json` escapes it: `"`, `\`, and
/// the control characters, with `\b`, `\f`, `\n`, `\r`, `\t` short and the
/// rest as `\u00XX`.
fn write_escaped(out: &mut Vec<u8>, text: &str) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push(b'"');
    let bytes = text.as_bytes();
    let mut start = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        let short = match byte {
            b'"' => b'"',
            b'\\' => b'\\',
            0x08 => b'b',
            0x0c => b'f',
            b'\n' => b'n',
            b'\r' => b'r',
            b'\t' => b't',
            0x00..=0x1f => b'u',
            _ => continue,
        };
        out.extend_from_slice(&bytes[start..i]);
        out.push(b'\\');
        out.push(short);
        if short == b'u' {
            out.extend_from_slice(b"00");
            out.push(HEX[usize::from(byte >> 4)]);
            out.push(HEX[usize::from(byte & 0xf)]);
        }
        start = i + 1;
    }
    out.extend_from_slice(&bytes[start..]);
    out.push(b'"');
}

// ─── json values ─────────────────────────────────────────────────────────────

/// Builds the `json` value. Leaves that are already JSON — strings, floats,
/// big bigints — are shared, not copied; every array and map is new.
struct ValueSink {
    json: RealizedTy,
    building: Vec<Building>,
    root: Option<Value>,
    /// Each class field's key, as a string object, made once per call.
    field_keys: FxHashMap<(HeapPtr, usize), Value>,
}

enum Building {
    Array(Vec<Value>),
    /// The entries so far, and the key of the next one.
    Object(MapData, Option<(u64, Value)>),
}

impl ValueSink {
    fn new(json: RealizedTy) -> Self {
        Self {
            json,
            building: Vec::new(),
            root: None,
            field_keys: FxHashMap::default(),
        }
    }

    /// `value` is the next value of the innermost container, or the root.
    fn emit(&mut self, value: Value) {
        match self.building.last_mut() {
            None => self.root = Some(value),
            Some(Building::Array(items)) => items.push(value),
            Some(Building::Object(data, key)) => {
                let (hash, key) = key
                    .take()
                    .unwrap_or_else(|| unreachable!("an object value follows its key"));
                // The walk writes each key once.
                data.insert_hashed(hash, key, value);
            }
        }
    }
}

impl Sink for ValueSink {
    fn non_finite(&self) -> NonFinite {
        NonFinite::Keep
    }

    fn null(&mut self, _: &mut BexVm) {
        self.emit(Value::NULL);
    }

    fn bool(&mut self, _: &mut BexVm, b: bool) {
        self.emit(Value::bool(b));
    }

    fn int(&mut self, _: &mut BexVm, i: i64) {
        self.emit(Value::int(i));
    }

    fn float(&mut self, _: &mut BexVm, value: Value, _: f64) {
        self.emit(value);
    }

    /// A bigint that fits `int` is an `int` in `json`, as `parse` reads it.
    fn bigint(&mut self, vm: &mut BexVm, value: Value) {
        let small = match value.as_object_ptr().map(|ptr| vm.get_object(ptr)) {
            Some(Object::Bigint(b)) => i64::try_from(&**b).ok().and_then(Value::try_int),
            _ => None,
        };
        self.emit(small.unwrap_or(value));
    }

    fn string(&mut self, _: &mut BexVm, value: Value) {
        self.emit(value);
    }

    fn text(&mut self, vm: &mut BexVm, text: &str) {
        let value = Value::object(vm.alloc_string(text));
        self.emit(value);
    }

    fn begin_array(&mut self, _: &mut BexVm) {
        self.building.push(Building::Array(Vec::new()));
    }

    fn item(&mut self, _: &mut BexVm, _: bool) {}

    fn end_array(&mut self, vm: &mut BexVm, _: bool) {
        let Some(Building::Array(items)) = self.building.pop() else {
            unreachable!("an array ends what it began")
        };
        let array = vm.alloc(Object::Array(Array::new(self.json.clone(), items)));
        self.emit(Value::object(array));
    }

    fn begin_object(&mut self, _: &mut BexVm) {
        self.building.push(Building::Object(MapData::new(), None));
    }

    fn key(&mut self, vm: &mut BexVm, _: bool, key: Key<'_>) {
        let (hash, key) = match key {
            Key::Value(value) => {
                let hash = vm
                    .as_string(&value)
                    .map(|key| bex_vm_types::map_string_hash(key.as_str()))
                    .unwrap_or_default();
                (hash, value)
            }
            Key::Field { class, index, text } => {
                let value = *self
                    .field_keys
                    .entry((class, index))
                    .or_insert_with(|| Value::object(vm.alloc_string(text)));
                (bex_vm_types::map_string_hash(text), value)
            }
            Key::Text(text) => (
                bex_vm_types::map_string_hash(text),
                Value::object(vm.alloc_string(text)),
            ),
        };
        if let Some(Building::Object(_, next)) = self.building.last_mut() {
            *next = Some((hash, key));
        }
    }

    fn end_object(&mut self, vm: &mut BexVm, _: bool) {
        let Some(Building::Object(data, _)) = self.building.pop() else {
            unreachable!("an object ends what it began")
        };
        let map = vm.alloc(Object::Map(Map::new(
            RealizedTy::String,
            self.json.clone(),
            data,
        )));
        self.emit(Value::object(map));
    }
}

#[cfg(test)]
mod tests {
    use super::write_escaped;

    /// Strings are written as `serde_json` wrote them, escapes included.
    #[test]
    fn escapes_strings_as_serde_json_does() {
        let mut cases: Vec<String> = (0u32..0x80)
            .filter_map(char::from_u32)
            .map(|c| format!("a{c}b"))
            .collect();
        cases.extend(
            [
                "",
                "plain",
                "é",
                "日本語",
                "😀",
                "\u{2028}\u{2029}",
                "\"\\/",
                "\u{7f}\u{80}\u{ffff}",
            ]
            .map(str::to_owned),
        );
        for case in cases {
            let mut ours = Vec::new();
            write_escaped(&mut ours, &case);
            let serde = serde_json::to_string(&case).expect("a string serializes");
            assert_eq!(String::from_utf8(ours).expect("UTF-8"), serde, "{case:?}");
        }
    }

    /// Floats are written as `serde_json` wrote them.
    #[test]
    fn formats_floats_as_serde_json_does() {
        let floats = [
            0.0,
            -0.0,
            1.0,
            -1.0,
            0.1,
            1.5,
            1e20,
            1e21,
            1e-7,
            123_456.789,
            1.5e300,
            -2.5e-300,
            f64::MAX,
            f64::MIN_POSITIVE,
            5e-324,
        ];
        for float in floats {
            let serde = serde_json::to_string(
                &serde_json::Number::from_f64(float).expect("a finite float"),
            )
            .expect("a number serializes");
            assert_eq!(zmij::Buffer::new().format_finite(float), serde, "{float}");
        }
    }
}
