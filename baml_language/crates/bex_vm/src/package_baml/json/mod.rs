//! Native handlers for the `baml.json` namespace: `parse`, `field`, and the
//! `_`-prefixed natives behind `decode<T>`, `from`, `stringify`, `to<T>`, and
//! the structural decode that `OpenAI` structured outputs use.
//!
//! No path goes through a `serde_json::Value`: `text` reads JSON text into
//! VM values, `decode_text` reads it straight into a declared type,
//! `decode` decodes `json` values into one, `encode` writes VM values
//! as text or as `json` values, and `path` walks a `path<T>` selector.
//!
//! The generic natives read their type-arg `T` from
//! `vm.current_call_type_args()`, populated by the call-instruction handler
//! from the leading `LoadType` operand. See `BexVm::pending_call_type_args`.

mod decode;
mod decode_text;
mod encode;
mod path;
mod plan;
mod serde_bridge;
mod text;
mod union;

use std::sync::Arc;

use baml_type::MediaKind;
use bex_heap::TlabHolder;
use bex_vm_types::{
    BexStr, HeapPtr, RealizedTy, TypeHead, Value, ValueKind,
    types::{Array, Map, Object},
};
use decode::{Decoder, Root};
use encode::{Finish, WriteOptions};
pub(super) use serde_bridge::{json_value_to_serde, serde_to_value};
use sys_types::JsonNaming;

use super::{BamlNamespaceJson, NativeCallResult, PackageBamlImpl};
use crate::{
    BexVm,
    errors::{VmInternalError, VmRustFnError},
};

/// The runtime type of an untyped `json` value: the recursive `baml.json.json`
/// alias (`null | bool | int | bigint | float | string | json[] | map<string, json>`).
/// Recursive aliases stay opaque in `RealizedTy`, so this is the most precise
/// element/value type available for containers parsed from untyped JSON.
pub(super) fn json_alias_ty(vm: &BexVm) -> RealizedTy {
    let head = vm
        .stdlib_heads()
        .json_alias()
        .unwrap_or_else(|| unreachable!("`baml.json.json` is declared by the stdlib"));
    RealizedTy::TypeAlias(head)
}

const STRING_MAP_REQUIRED: &str = "JSON requires maps with string keys";

/// The error for a JSON object or array that contains itself: the walks
/// refuse it where they meet it again instead of descending forever.
const CYCLIC_JSON: &str = "cyclic json value: the document contains itself";

const JSON_PARSE_ERROR: baml_base::LangItem = crate::lang_items::BAML_JSON_PARSE_ERROR;
const JSON_DECODE_ERROR: baml_base::LangItem = crate::lang_items::BAML_JSON_DECODE_ERROR;
const JSON_SERIALIZATION_ERROR: baml_base::LangItem =
    crate::lang_items::BAML_JSON_SERIALIZATION_ERROR;

// ─── Trait implementation ─────────────────────────────────────────────────────

/// The call's type argument `T`.
fn type_arg(vm: &BexVm, native: &str) -> Result<RealizedTy, VmRustFnError> {
    vm.current_call_type_args().first().cloned().ok_or_else(|| {
        VmRustFnError::InternalError(VmInternalError::MissingNativeFunction {
            name: format!("baml.json.{native}: missing type argument"),
        })
    })
}

impl BamlNamespaceJson for PackageBamlImpl {
    fn parse(vm: &mut BexVm, s: &bex_str::BexStr) -> Result<Value, VmRustFnError> {
        let s = s.as_str();
        text::parse(vm, s).map_err(|text::Stop| text::syntax_error(vm, s))
    }

    fn _decode(vm: &mut BexVm, s: &bex_str::BexStr, aliased: bool) -> NativeCallResult {
        let ty = match type_arg(vm, "_decode") {
            Ok(ty) => ty,
            Err(error) => return NativeCallResult::Error(error),
        };
        let s = s.as_str();
        let mut decoder = Decoder::new(JsonNaming::new(aliased));
        // With no override in reach, `T` reads the text in one pass.
        if !decoder.lookups.reaches_override(vm, &ty)
            && let Some(value) = decode_text::decode(vm, s, &ty, &mut decoder.lookups.plans)
        {
            return NativeCallResult::Done(value);
        }
        // `to<T>(parse(s))`: the text decode failed, or an override can run.
        match text::parse(vm, s) {
            Ok(json) => decoder.decode(vm, json, &ty, Root::Dispatch),
            Err(text::Stop) => NativeCallResult::Error(text::syntax_error(vm, s)),
        }
    }

    fn _from(vm: &mut BexVm, value: &Value, aliased: bool, omit_nulls: bool) -> NativeCallResult {
        let options = WriteOptions {
            naming: JsonNaming::new(aliased),
            omit_nulls,
        };
        encode::render_json(vm, *value, options, Finish::Json)
    }

    fn _stringify(
        vm: &mut BexVm,
        value: &Value,
        aliased: bool,
        indent: &Value,
        omit_nulls: bool,
    ) -> NativeCallResult {
        let indent = match indent_text(vm, *indent) {
            Ok(indent) => indent,
            Err(e) => return NativeCallResult::Error(e),
        };
        let options = WriteOptions {
            naming: JsonNaming::new(aliased),
            omit_nulls,
        };
        encode::render_json(vm, *value, options, Finish::Text { indent })
    }

    fn _to(vm: &mut BexVm, j: &Value, aliased: bool) -> NativeCallResult {
        match type_arg(vm, "_to") {
            Ok(ty) => Decoder::new(JsonNaming::new(aliased)).decode(vm, *j, &ty, Root::Dispatch),
            Err(error) => NativeCallResult::Error(error),
        }
    }

    fn _path(
        vm: &mut BexVm,
        j: &Value,
        selector: &bex_str::BexStr,
    ) -> Result<Value, VmRustFnError> {
        path::select(vm, *j, selector.as_str())
    }

    fn field(vm: &mut BexVm, j: &Value, key: &bex_str::BexStr) -> Value {
        // The public throws-never API accepts `json`, not arbitrary maps.
        let Some(ptr) = j.as_object_ptr() else {
            return Value::NULL;
        };
        match vm.get_object(ptr) {
            Object::Map(map) => {
                assert!(
                    string_keyed(vm, map),
                    "baml.json.field received invalid json: {STRING_MAP_REQUIRED}"
                );
                let hash = bex_vm_types::map_string_hash(key.as_str());
                object_get(vm, ptr, hash, key.as_str()).unwrap_or(Value::NULL)
            }
            _ => Value::NULL,
        }
    }
}

/// Entry point for `baml._to_json_default`: `from` with the default options.
pub(super) fn render_to_json_honoring_overrides(vm: &mut BexVm, value: Value) -> NativeCallResult {
    encode::render_json(vm, value, WriteOptions::DEFAULT, Finish::Json)
}

pub(super) use encode::structural_json_text;

/// Reads the target type `T` from the call's type-args and decodes `j` as it,
/// the root structurally — the `baml._from_json_structural_default` native,
/// the body every implicit `baml.FromJson` implementation runs. The root's
/// own rule is not consulted (it IS this body); its fields, elements and
/// values honor theirs.
pub(super) fn json_to_structural_default(vm: &mut BexVm, j: Value) -> NativeCallResult {
    match type_arg(vm, "to") {
        Ok(ty) => Decoder::new(JsonNaming::Declared).decode(vm, j, &ty, Root::Structural),
        Err(error) => NativeCallResult::Error(error),
    }
}

/// The widest indent `stringify` takes, in spaces or characters per level:
/// JavaScript's `JSON.stringify` stops at the same width. Without a cap, a huge
/// count would ask the allocator for the whole indent string up front.
const MAX_INDENT: usize = 10;

/// The text `baml.json.stringify` writes for one nesting level, from its
/// `indent` argument: that many spaces for an `int`, the string itself for a
/// `string`. A negative count, anything wider than [`MAX_INDENT`], or a string
/// holding anything other than JSON whitespace (which would make the text
/// invalid JSON), is refused.
fn indent_text(vm: &mut BexVm, indent: Value) -> Result<String, VmRustFnError> {
    let refused = match indent.kind() {
        ValueKind::Int(count) => match usize::try_from(count) {
            Ok(count) if count <= MAX_INDENT => return Ok(" ".repeat(count)),
            Ok(_) => format!("indent must be at most {MAX_INDENT} spaces, got {count}"),
            Err(_) => format!("indent must not be negative, got {count}"),
        },
        ValueKind::Object(ptr) => match vm.get_object(ptr) {
            Object::String(text) => {
                let width = text.as_str().chars().count();
                if width > MAX_INDENT {
                    format!("indent must be at most {MAX_INDENT} characters, got {width}")
                } else if text
                    .as_str()
                    .chars()
                    .all(|c| matches!(c, ' ' | '\t' | '\n' | '\r'))
                {
                    return Ok(text.to_string());
                } else {
                    format!(
                        "indent must hold only spaces, tabs, and line breaks, got {:?}",
                        text.as_str()
                    )
                }
            }
            _ => unreachable!("`indent` is statically `int | string`"),
        },
        _ => unreachable!("`indent` is statically `int | string`"),
    };
    Err(raise_serialize(vm, refused, "", "indent"))
}

// ─── JSON objects ────────────────────────────────────────────────────────────

/// Whether `map` has string keys, the only keys a JSON object has.
fn string_keyed(vm: &BexVm, map: &Map) -> bool {
    matches!(*map.key_ty, RealizedTy::String)
        || baml_type::normalize::is_string_key(map.key_ty.as_ty(), vm)
}

/// The value under `key` (whose map hash is `hash`) in the string-keyed map
/// at `ptr`.
fn object_get(vm: &BexVm, ptr: HeapPtr, hash: u64, key: &str) -> Option<Value> {
    let Object::Map(map) = vm.get_object(ptr) else {
        return None;
    };
    map.lock().get_hashed(hash, |candidate| {
        vm.as_string(candidate)
            .is_ok_and(|candidate| candidate.as_str() == key)
    })
}

/// The entries of the string-keyed map at `ptr`, in order.
fn string_entries(vm: &BexVm, ptr: HeapPtr) -> Vec<(BexStr, Value)> {
    let Object::Map(map) = vm.get_object(ptr) else {
        return Vec::new();
    };
    map.snapshot_entries()
        .into_iter()
        .filter_map(|(key, value)| Some((vm.as_string(&key).ok()?.clone(), value)))
        .collect()
}

/// Whether the JSON key `written` is a value of the map key type `key`.
fn key_fits(vm: &BexVm, written: &str, key: &RealizedTy) -> bool {
    baml_type::normalize::is_subtype(
        &baml_type::Ty::Literal(
            baml_type::Literal::String(written.to_owned()),
            baml_type::Freshness::Regular,
        ),
        key.as_ty(),
        vm,
    )
}

/// `json` copied as a `json` value, as it has always been copied into a
/// `json` or `unknown` target: every array and map is new, leaves are shared
/// (a float that is not finite too, as `from` keeps it), a bigint that fits
/// `int` is an `int`, an enum variant is its declared name, and a value with
/// no JSON form is `null`. A container met again inside itself, or a map
/// whose keys are not strings, is refused: its message and its path inside
/// `json`.
fn copy_json(vm: &mut BexVm, json: Value) -> Result<Value, (&'static str, String)> {
    /// A container being copied: what it holds and what it has so far.
    struct Copying {
        ptr: HeapPtr,
        /// Each child, with its key in a map.
        children: Vec<(Option<(u64, Value)>, Value)>,
        copied: Vec<Value>,
        is_map: bool,
    }
    let path = |open: &[Copying], vm: &BexVm| {
        use std::fmt::Write;
        let mut path = String::new();
        for copying in open {
            match copying.children.get(copying.copied.len()) {
                Some((Some((_, key)), _)) => {
                    let key = vm
                        .as_string(key)
                        .map(bex_vm_types::BexStr::as_str)
                        .unwrap_or_default();
                    let _ = write!(path, "[{key:?}]");
                }
                Some((None, _)) => {
                    let _ = write!(path, "[{}]", copying.copied.len());
                }
                None => {}
            }
        }
        path
    };
    let json_ty = json_alias_ty(vm);
    let mut open: Vec<Copying> = Vec::new();
    let mut inside = OpenSet::default();
    let mut next = Some(json);
    loop {
        if let Some(value) = next.take() {
            let copied = match value.kind() {
                ValueKind::Null | ValueKind::OmittedArg => Some(Value::NULL),
                ValueKind::Bool(_) | ValueKind::Int(_) => Some(value),
                ValueKind::Object(ptr) => {
                    let children = match vm.get_object(ptr) {
                        Object::Array(array) => Some((
                            array
                                .to_vec()
                                .into_iter()
                                .map(|v| (None, v))
                                .collect::<Vec<_>>(),
                            false,
                        )),
                        Object::Map(map) => {
                            if inside.contains(ptr) {
                                return Err((CYCLIC_JSON, path(&open, vm)));
                            }
                            if !string_keyed(vm, map) {
                                return Err((STRING_MAP_REQUIRED, path(&open, vm)));
                            }
                            Some((
                                map.snapshot_entries()
                                    .into_iter()
                                    .map(|(key, v)| {
                                        let hash = vm.as_string(&key).map_or(0, |key| {
                                            bex_vm_types::map_string_hash(key.as_str())
                                        });
                                        (Some((hash, key)), v)
                                    })
                                    .collect(),
                                true,
                            ))
                        }
                        _ => None,
                    };
                    match children {
                        Some((children, is_map)) => {
                            if inside.contains(ptr) {
                                return Err((CYCLIC_JSON, path(&open, vm)));
                            }
                            inside.insert(ptr);
                            open.push(Copying {
                                ptr,
                                copied: Vec::with_capacity(children.len()),
                                children,
                                is_map,
                            });
                            None
                        }
                        None => Some(copy_leaf(vm, value, ptr)),
                    }
                }
            };
            if let Some(copied) = copied {
                match open.last_mut() {
                    Some(parent) => parent.copied.push(copied),
                    None => return Ok(copied),
                }
            }
        }
        let Some(copying) = open.last() else {
            unreachable!("the copy is inside a container until it returns")
        };
        if let Some((_, child)) = copying.children.get(copying.copied.len()) {
            next = Some(*child);
            continue;
        }
        let copying = open
            .pop()
            .unwrap_or_else(|| unreachable!("the container was just read"));
        inside.remove(copying.ptr);
        let copied = if copying.is_map {
            let mut data = bex_vm_types::MapData::new();
            for ((key, _), value) in copying.children.into_iter().zip(copying.copied) {
                let (hash, key) = key.unwrap_or_else(|| unreachable!("map children have keys"));
                data.insert_hashed(hash, key, value);
            }
            vm.alloc(Object::Map(Map::new(
                RealizedTy::String,
                json_ty.clone(),
                data,
            )))
        } else {
            vm.alloc(Object::Array(Array::new(json_ty.clone(), copying.copied)))
        };
        match open.last_mut() {
            Some(parent) => parent.copied.push(Value::object(copied)),
            None => return Ok(Value::object(copied)),
        }
    }
}

/// A heap leaf of a `json` value, copied by [`copy_json`]'s rules.
fn copy_leaf(vm: &mut BexVm, value: Value, ptr: HeapPtr) -> Value {
    let name = match vm.get_object(ptr) {
        Object::Float(_) => return value,
        Object::String(_) => return value,
        Object::Bigint(b) => {
            return i64::try_from(&**b)
                .ok()
                .and_then(Value::try_int)
                .unwrap_or(value);
        }
        Object::Variant(variant) => match vm.get_object(variant.enm) {
            Object::Enum(e) => e.variants.get(variant.index).map(|v| v.name.clone()),
            _ => None,
        },
        Object::Tombstone => Object::tombstone_reached(),
        _ => None,
    };
    match name {
        Some(name) => Value::object(vm.alloc_string(name)),
        None => Value::NULL,
    }
}

/// The containers a walk is inside, by pointer: scanned while few, hashed
/// once deep, so a deep value costs no more per level than a shallow one.
#[derive(Default)]
pub(super) struct OpenSet {
    few: Vec<HeapPtr>,
    many: Option<rustc_hash::FxHashSet<HeapPtr>>,
}

/// Containers [`OpenSet`] scans before it hashes.
const OPEN_SCANNED: usize = 32;

impl OpenSet {
    pub(super) fn contains(&self, ptr: HeapPtr) -> bool {
        match &self.many {
            Some(many) => many.contains(&ptr),
            None => self.few.contains(&ptr),
        }
    }

    pub(super) fn insert(&mut self, ptr: HeapPtr) {
        match &mut self.many {
            Some(many) => {
                many.insert(ptr);
            }
            None if self.few.len() < OPEN_SCANNED => self.few.push(ptr),
            None => {
                let mut many: rustc_hash::FxHashSet<HeapPtr> = self.few.drain(..).collect();
                many.insert(ptr);
                self.many = Some(many);
            }
        }
    }

    pub(super) fn remove(&mut self, ptr: HeapPtr) {
        match &mut self.many {
            Some(many) => {
                many.remove(&ptr);
            }
            None => {
                if let Some(position) = self.few.iter().rposition(|open| *open == ptr) {
                    self.few.remove(position);
                }
            }
        }
    }
}

// ─── Shared decode rules ─────────────────────────────────────────────────────

/// Decode signed decimal text without passing through `f64`. Check the size
/// before parsing so untrusted JSON cannot allocate an unbounded integer.
fn parse_decimal_bigint(text: &str) -> Option<num_bigint::BigInt> {
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    if digits.is_empty()
        || digits.len() > baml_type::MAX_BIGINT_DECIMAL_DIGITS
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let magnitude = num_bigint::BigInt::parse_bytes(digits.as_bytes(), 10)?;
    let value = if negative { -magnitude } else { magnitude };
    (value.bits() <= baml_type::MAX_BIGINT_BITS).then_some(value)
}

/// The members a value of `ty` may decode as at one JSON node: with each
/// union flattened and each recursive alias other than `json` replaced by its
/// definition. An alias met again while unfolding contributes nothing: it took
/// no JSON to reach, so it is an unguarded member, which adds no values.
fn alias_members(
    vm: &BexVm,
    ty: &RealizedTy,
    unfolding: &mut Vec<HeapPtr>,
    out: &mut Vec<RealizedTy>,
) {
    match ty {
        RealizedTy::TypeAlias(head) if !vm.stdlib_heads().is_json_alias(*head) => {
            let alias = head.ptr();
            if unfolding.contains(&alias) {
                return;
            }
            let Object::TypeAlias(declaration) = vm.get_object(alias) else {
                unreachable!("a type alias's head is a type alias: {head:?}")
            };
            let definition = declaration.definition.clone();
            unfolding.push(alias);
            alias_members(vm, &definition, unfolding, out);
            unfolding.pop();
        }
        RealizedTy::Union(members) => {
            for member in members {
                alias_members(vm, member, unfolding, out);
            }
        }
        _ => out.push(ty.clone()),
    }
}

/// The variant of the enum at `head` that JSON writes as `written` under
/// `naming`, or `Err` when `head` is not an enum.
fn variant_named(
    vm: &BexVm,
    head: TypeHead,
    written: &str,
    naming: JsonNaming,
) -> Result<Option<usize>, ()> {
    match vm.get_object(head.ptr()) {
        Object::Enum(e) => Ok(e.variants.iter().position(|v| {
            naming.shows(v.skip) && naming.key(&v.name, v.alias.as_deref()) == written
        })),
        _ => Err(()),
    }
}

/// The variant JSON writes as `written`, when that is how it writes the
/// variant `name` of the enum at `head`.
fn variant_written_as(
    vm: &BexVm,
    head: TypeHead,
    name: &baml_type::Name,
    written: &str,
    naming: JsonNaming,
) -> Option<usize> {
    let Object::Enum(e) = vm.get_object(head.ptr()) else {
        return None;
    };
    let variant = e.variants.iter().find(|v| v.name == name.as_str())?;
    if !naming.shows(variant.skip) || naming.key(&variant.name, variant.alias.as_deref()) != written
    {
        return None;
    }
    variant_named(vm, head, written, naming).ok().flatten()
}

// ─── Media ───────────────────────────────────────────────────────────────────

/// The string fields of a tagged media object; a field that is absent or not
/// a string is `None`.
#[derive(Default)]
struct MediaFields {
    kind: Option<String>,
    source: Option<String>,
    value: Option<String>,
    mime: Option<String>,
    name: Option<String>,
}

/// The media value of kind `kind` (an instance of the class at `head`) a
/// tagged object describes, or the message saying what is wrong with it.
fn build_media(
    vm: &mut BexVm,
    fields: &MediaFields,
    kind: MediaKind,
    head: TypeHead,
) -> Result<Value, String> {
    let tagged_kind = fields
        .kind
        .as_deref()
        .ok_or("media object missing `kind`")?;
    if tagged_kind != kind.tag_str() {
        return Err(format!(
            "media kind mismatch: expected `{}`, got `{tagged_kind}`",
            kind.tag_str()
        ));
    }
    let source = fields
        .source
        .as_deref()
        .ok_or("media object missing `source`")?;
    let value = fields
        .value
        .as_deref()
        .ok_or("media object missing `value`")?;
    let mime = fields.mime.as_deref();
    let media: Arc<bex_vm_types::MediaValue> = match (source, fields.name.as_deref()) {
        ("url", _) => bex_vm_types::MediaValue::from_url(kind, value, mime),
        ("base64" | "inline", Some(name)) => {
            bex_vm_types::MediaValue::from_file_content(kind, name, value.into(), mime)
        }
        ("base64" | "inline", None) => {
            bex_vm_types::MediaValue::from_base64(kind, value.into(), mime)
        }
        // Reading a path out of data would let any JSON name a file to read.
        ("file", _) => {
            return Err(
                "media source `file` is not supported: a file is read when the value \
                        is built (use `from_file`), so JSON carries its content as `base64`"
                    .to_owned(),
            );
        }
        (other, _) => {
            return Err(format!(
                "unknown media source `{other}` (expected url|base64)"
            ));
        }
    };
    let data = Value::object(vm.alloc_rust_data(media));
    Ok(Value::object(vm.alloc_instance(head.ptr(), vec![data])))
}

pub(crate) fn read_media_value(vm: &BexVm, value: Value) -> Option<Arc<bex_vm_types::MediaValue>> {
    let ptr = value.as_object_ptr()?;
    let (class, data_value) = match vm.get_object(ptr) {
        Object::Instance(inst) => (inst.class, inst.fields.first()?.load()),
        _ => return None,
    };
    let class_head = match vm.get_object(class) {
        Object::Class(c) => TypeHead::new(class, c.type_tag),
        _ => return None,
    };
    vm.stdlib_heads().media_kind(class_head)?;

    let data_ptr = data_value.as_object_ptr()?;
    match vm.get_object(data_ptr) {
        Object::RustData(arc) => {
            use bex_vm_types::RustDataArc as _;
            arc.clone()
                .downcast_payload::<bex_vm_types::MediaValue>()
                .ok()
        }
        _ => None,
    }
}

// ─── Error throwers ───────────────────────────────────────────────────────────

fn throw_json_parse_error(vm: &mut BexVm, message: String) -> Value {
    let class_ptr = vm.lang_object(JSON_PARSE_ERROR);
    let message_val = Value::object(vm.alloc_string(message));
    Value::object(vm.alloc_instance(class_ptr, vec![message_val]))
}

fn throw_json_decode_error(vm: &mut BexVm, message: String, path: &str) -> Value {
    let class_ptr = vm.lang_object(JSON_DECODE_ERROR);
    let message_val = Value::object(vm.alloc_string(message));
    let path_val = Value::object(vm.alloc_string(path.to_string()));
    Value::object(vm.alloc_instance(class_ptr, vec![message_val, path_val]))
}

fn throw_json_serialization_error(
    vm: &mut BexVm,
    message: String,
    path: &str,
    reason: &str,
) -> Value {
    let class_ptr = vm.lang_object(JSON_SERIALIZATION_ERROR);
    let message_val = Value::object(vm.alloc_string(message));
    let path_val = Value::object(vm.alloc_string(path.to_string()));
    let reason_val = Value::object(vm.alloc_string(reason.to_string()));
    Value::object(vm.alloc_instance(class_ptr, vec![message_val, path_val, reason_val]))
}

fn raise_decode(vm: &mut BexVm, message: impl Into<String>, path: &str) -> VmRustFnError {
    VmRustFnError::thrown_fresh(throw_json_decode_error(vm, message.into(), path))
}

fn raise_serialize(
    vm: &mut BexVm,
    message: impl Into<String>,
    path: &str,
    reason: &str,
) -> VmRustFnError {
    VmRustFnError::thrown_fresh(throw_json_serialization_error(
        vm,
        message.into(),
        path,
        reason,
    ))
}
