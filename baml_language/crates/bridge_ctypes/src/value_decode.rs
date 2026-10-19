//! Decode C FFI / protobuf host values into `BexExternalValue`.
//!
//! The wire is the serialization of a concretely typed value graph. A scalar,
//! `null`, enum variant, media value, rendered prompt, reflected type or
//! handle implies its own type and carries no `value_type`. A list, map, class
//! or host callable node states the type it has in `value_type` — `list<E>`,
//! `map<K, V>`, the class with its type arguments, the callable's function type
//! — and the decoder writes that statement into the node it builds. A
//! statement where the type is implied, a missing statement, a statement of
//! another shape, or one that mentions a type variable is refused here: the
//! engine builds exactly what the wire says and fills no hole from a slot.

use std::{collections::HashMap, sync::Arc};

use bex_project::{
    BexExternalAdt, BexExternalValue, MediaKind, MediaValue, PromptAst, PromptAstSimple, RuntimeTy,
};
use indexmap::IndexMap;

use crate::{
    baml_bridge::cffi::{
        BamlHandleType, BamlValueMedia, BamlValuePromptAst, BamlValuePromptAstSimple,
        InboundClassValue, InboundEnumValue, InboundListValue, InboundMapEntry, InboundMapValue,
        InboundValue, MediaTypeEnum, baml_value_media,
        baml_value_prompt_ast::Value as PromptAstVariant,
        baml_value_prompt_ast_simple::Value as PromptAstSimpleVariant,
        inbound_value::Value as InboundValueVariant,
    },
    error::CtypesError,
    handle_table::CffiHandleTable,
};

/// Owns every remaining wire reference, including those after a malformed
/// sibling. Recursive decoding shares this inventory, so drained references
/// are never released a second time on an error. A key is one reference
/// however often it occurs: a host registers each shipped handle once, and
/// the release it owes is one per key. A key that occurs under two handle
/// kinds names two different things with one identity and is refused.
struct InboundOwners<'a> {
    table: &'a CffiHandleTable,
    handles: Vec<crate::baml_bridge::cffi::BamlHandle>,
    conflict: Option<u64>,
}
impl<'a> InboundOwners<'a> {
    fn new(table: &'a CffiHandleTable) -> Self {
        Self {
            table,
            handles: Vec::new(),
            conflict: None,
        }
    }
    /// Refuse the whole value when one key arrived under two handle kinds.
    fn consistent(&self) -> Result<(), CtypesError> {
        match self.conflict {
            Some(key) => Err(CtypesError::HandleKindConflict(key)),
            None => Ok(()),
        }
    }
    fn collect(&mut self, value: &InboundValue) {
        match &value.value {
            Some(InboundValueVariant::Handle(handle)) => {
                match self.handles.iter().find(|owned| owned.key == handle.key) {
                    None => self.handles.push(*handle),
                    Some(owned) if owned.handle_type == handle.handle_type => {}
                    Some(_) => self.conflict = Some(handle.key),
                }
            }
            Some(InboundValueVariant::ListValue(list)) => {
                for value in &list.values {
                    self.collect(value);
                }
            }
            Some(InboundValueVariant::MapValue(map)) => {
                for entry in &map.entries {
                    if let Some(value) = &entry.value {
                        self.collect(value);
                    }
                }
            }
            Some(InboundValueVariant::ClassValue(class)) => {
                for entry in &class.fields {
                    if let Some(value) = &entry.value {
                        self.collect(value);
                    }
                }
            }
            _ => {}
        }
    }
    fn take(&mut self, handle: &crate::baml_bridge::cffi::BamlHandle) {
        if let Some(index) = self
            .handles
            .iter()
            .position(|owned| owned.key == handle.key)
        {
            self.handles.swap_remove(index);
        }
    }
}
impl Drop for InboundOwners<'_> {
    fn drop(&mut self) {
        for handle in self.handles.drain(..) {
            let kind = match BamlHandleType::try_from(handle.handle_type) {
                Ok(BamlHandleType::HostValueCallable) => Some(bex_project::HostValueKind::Callable),
                Ok(BamlHandleType::HostValueOpaque) => Some(bex_project::HostValueKind::Opaque),
                _ => None,
            };
            if let Some(kind) = kind {
                drop(bex_project::HostValueArc::intern(handle.key, kind));
            } else {
                self.table.release(handle.key);
            }
        }
    }
}

/// Decode a protobuf `InboundValue` into a `BexExternalValue` for use by the BEX engine.
///
/// Handles are resolved via `handle_table`; an unknown key returns `InvalidHandleKey`.
pub fn inbound_to_external(
    value: InboundValue,
    handle_table: &CffiHandleTable,
) -> Result<BexExternalValue, CtypesError> {
    let mut owners = InboundOwners::new(handle_table);
    owners.collect(&value);
    owners.consistent()?;
    decode_inbound(value, handle_table, &mut owners)
}

fn decode_inbound(
    value: InboundValue,
    handle_table: &CffiHandleTable,
    owners: &mut InboundOwners<'_>,
) -> Result<BexExternalValue, CtypesError> {
    let stated = value
        .value_type
        .as_ref()
        .map(crate::ty_decode::proto_ty_to_runtime_ty)
        .transpose()?;
    // A type that mentions a type variable determines no value: the host
    // binds its generics before it sends.
    if let Some(stated) = &stated
        && let Some(var) = crate::ty_decode::first_type_var(stated)
    {
        return Err(CtypesError::ValueTypeNotRealized {
            stated: bounded(stated.to_string()),
            var: bounded(var.name().to_string()),
        });
    }
    let Some(variant) = value.value else {
        implied(stated.as_ref(), "null")?;
        return Ok(BexExternalValue::Null);
    };
    match variant {
        InboundValueVariant::StringValue(s) => {
            implied(stated.as_ref(), "string_value")?;
            Ok(BexExternalValue::String(s.into()))
        }
        InboundValueVariant::IntValue(i) => {
            implied(stated.as_ref(), "int_value")?;
            Ok(BexExternalValue::Int(i))
        }
        InboundValueVariant::BigintValue(s) => {
            // Pre-allocation cap: stop a multi-megabyte hex blob from
            // building a multi-megabyte `BigInt` before the VM's own
            // `MAX_BIGINT_BITS` guard fires. Sized to match that VM cap
            // (~268M bits ≈ 67M hex digits, +2 for sign and slack).
            const MAX_BIGINT_HEX_LEN: usize = (1 << 28) / 4 + 2;
            implied(stated.as_ref(), "bigint_value")?;
            // Hex / base sixteen on the wire, parsed strictly: `parse_bytes`
            // accepts only `[0-9a-fA-F]` plus an optional leading `-`/`+`
            // (no `0x` prefix, underscores, or surrounding whitespace),
            // matching the encoders' output and the Python/JS bridges.
            let len = s.len();
            if len > MAX_BIGINT_HEX_LEN {
                return Err(CtypesError::InvalidBigint { len });
            }
            let bi = num_bigint::BigInt::parse_bytes(s.as_bytes(), 16)
                .ok_or(CtypesError::InvalidBigint { len })?;
            Ok(BexExternalValue::Bigint(bi))
        }
        InboundValueVariant::FloatValue(f) => {
            implied(stated.as_ref(), "float_value")?;
            Ok(BexExternalValue::Float(f))
        }
        InboundValueVariant::JsNumberValue(_) => Err(CtypesError::JsNumberValueRetired),
        InboundValueVariant::BoolValue(b) => {
            implied(stated.as_ref(), "bool_value")?;
            Ok(BexExternalValue::Bool(b))
        }
        InboundValueVariant::ListValue(list) => {
            let element = match required(stated, "list_value", "a list type")? {
                RuntimeTy::List(element) => *element,
                other => return Err(shape("list_value", "a list type", &other)),
            };
            convert_list(list, element, handle_table, owners)
        }
        InboundValueVariant::MapValue(map) => {
            let (key, value) = match required(stated, "map_value", "a map type")? {
                RuntimeTy::Map { key, value } => (*key, *value),
                other => return Err(shape("map_value", "a map type", &other)),
            };
            convert_map(map, key, value, handle_table, owners)
        }
        InboundValueVariant::ClassValue(class) => {
            let (name, type_args) = match required(stated, "class_value", "a class type")? {
                RuntimeTy::Class(name, type_args) => (name, type_args),
                other => return Err(shape("class_value", "a class type", &other)),
            };
            convert_class(class, &name, type_args.into_vec(), handle_table, owners)
        }
        InboundValueVariant::EnumValue(e) => {
            implied(stated.as_ref(), "enum_value")?;
            Ok(convert_enum(e))
        }
        InboundValueVariant::Uint8arrayValue(bytes) => {
            implied(stated.as_ref(), "uint8array_value")?;
            Ok(BexExternalValue::Uint8Array(bytes))
        }
        // A reflected BAML type passed as an argument value.
        InboundValueVariant::TyValue(ty) => {
            implied(stated.as_ref(), "ty_value")?;
            crate::ty_decode::proto_ty_to_external(&ty)
        }
        InboundValueVariant::TyDefValue(ty) => {
            implied(stated.as_ref(), "ty_def_value")?;
            crate::ty_decode::proto_ty_def_to_external(&ty)
        }
        InboundValueVariant::MediaValue(media) => {
            implied(stated.as_ref(), "media_value")?;
            Ok(BexExternalValue::Adt(BexExternalAdt::Media(
                proto_media_to_bex_media(media)?,
            )))
        }
        InboundValueVariant::PromptAstValue(prompt) => {
            implied(stated.as_ref(), "prompt_ast_value")?;
            Ok(BexExternalValue::Adt(BexExternalAdt::PromptAst(Arc::new(
                proto_prompt_ast_to_bex_prompt_ast(prompt)?,
            ))))
        }
        InboundValueVariant::Handle(handle) => {
            // Key 0 names no slot in any table. A host handle carrying it
            // would reach the host as a callback frame whose callable identity
            // the frame decoder refuses, and the call would never complete.
            if handle.key == 0 {
                return Err(CtypesError::InvalidHandleKey(0));
            }
            // HOST_VALUE_* keys do NOT live in HANDLE_TABLE. The host
            // bridge owns the lookup; we construct the Arc stub here so
            // last-drop fires the registered HostReleaseFn.
            //
            // Intern by key so repeated decodes of the same wire key
            // (e.g. BAML returns a host callable, the host passes it
            // back in) share one Arc identity. Otherwise each decode
            // would mint an independent Arc with its own refcount and
            // the first last-drop would fire HostReleaseFn, tearing the
            // registry entry out from under the still-live other Arc.
            if handle.handle_type == BamlHandleType::HostValueCallable as i32 {
                // A callable has no type of its own on the wire: the host
                // states the function type it inhabits.
                let (params, ret, throws) =
                    match required(stated, "handle (callable)", "a function type")? {
                        RuntimeTy::Function {
                            params,
                            ret,
                            throws,
                        } => (params, ret, throws),
                        other => {
                            return Err(shape("handle (callable)", "a function type", &other));
                        }
                    };
                let arc = bex_project::HostValueArc::intern(
                    handle.key,
                    bex_project::HostValueKind::Callable,
                );
                owners.take(&handle);
                return Ok(BexExternalValue::HostCallable {
                    handle: arc,
                    params,
                    ret,
                    throws,
                });
            }
            implied(stated.as_ref(), "handle")?;
            if handle.handle_type == BamlHandleType::HostValueOpaque as i32 {
                let arc = bex_project::HostValueArc::intern(
                    handle.key,
                    bex_project::HostValueKind::Opaque,
                );
                owners.take(&handle);
                return Ok(BexExternalValue::HostValue(arc));
            }
            // Internal invocation capabilities are control-channel data,
            // not application values, even if the host forges their tag.
            let pinned = handle_table
                .resolve(handle.key)
                .ok_or(CtypesError::InvalidHandleKey(handle.key))?;
            if matches!(
                &*pinned,
                crate::CffiHandleTableEntry::InvocationState(_)
                    | crate::CffiHandleTableEntry::TraceReservation(_)
            ) {
                return Err(CtypesError::InternalError(
                    "invocation capabilities cannot be supplied as application values".into(),
                ));
            }
            let value = handle_table
                .drain(handle.key)
                .ok_or(CtypesError::InvalidHandleKey(handle.key))?;
            owners.take(&handle);
            Ok(BexExternalValue::from((*value).clone()))
        }
    }
}

/// A node whose type the wire implies carries no statement.
fn implied(stated: Option<&RuntimeTy>, node: &'static str) -> Result<(), CtypesError> {
    match stated {
        None => Ok(()),
        Some(_) => Err(CtypesError::UnexpectedValueType { node }),
    }
}

/// The statement a list, map, class or callable node has to carry.
fn required(
    stated: Option<RuntimeTy>,
    node: &'static str,
    expected: &'static str,
) -> Result<RuntimeTy, CtypesError> {
    stated.ok_or(CtypesError::MissingValueType { node, expected })
}

/// A statement of another shape than the node it is on.
fn shape(node: &'static str, expected: &'static str, stated: &RuntimeTy) -> CtypesError {
    CtypesError::ValueTypeShape {
        node,
        expected,
        stated: bounded(stated.to_string()),
    }
}

/// A type as a message carries it: the host's text, cut to a length a message
/// can hold.
/// A rendering for a refusal, cut so no host-chosen text is echoed in full.
fn bounded(text: String) -> String {
    const LIMIT: usize = 200;
    if text.chars().count() <= LIMIT {
        text
    } else {
        let head: String = text.chars().take(LIMIT).collect();
        format!("{head}…")
    }
}

fn proto_media_to_bex_media(media: BamlValueMedia) -> Result<Arc<MediaValue>, CtypesError> {
    let kind = match MediaTypeEnum::try_from(media.media) {
        Ok(MediaTypeEnum::Image) => MediaKind::Image,
        Ok(MediaTypeEnum::Audio) => MediaKind::Audio,
        Ok(MediaTypeEnum::Video) => MediaKind::Video,
        Ok(MediaTypeEnum::Pdf) => MediaKind::Pdf,
        Ok(MediaTypeEnum::Other) => MediaKind::Generic,
        Ok(MediaTypeEnum::MediaTypeUnspecified) | Err(_) => {
            return Err(CtypesError::InternalError(
                "portable media payload has no valid media kind".to_string(),
            ));
        }
    };
    let mime_type = media.mime_type.as_deref();
    match media.value {
        Some(baml_value_media::Value::Url(url)) => Ok(MediaValue::from_url(kind, &url, mime_type)),
        Some(baml_value_media::Value::Base64(base64)) => {
            Ok(MediaValue::from_base64(kind, base64.into(), mime_type))
        }
        Some(baml_value_media::Value::FileContent(content)) => Ok(MediaValue::from_file_content(
            kind,
            &content.name,
            content.base64.into(),
            mime_type,
        )),
        None => Err(CtypesError::InternalError(
            "portable media payload has no content".to_string(),
        )),
    }
}

fn proto_prompt_ast_to_bex_prompt_ast(
    prompt: BamlValuePromptAst,
) -> Result<PromptAst, CtypesError> {
    match prompt.value {
        Some(PromptAstVariant::Simple(simple)) => Ok(PromptAst::Simple(Arc::new(
            proto_prompt_ast_simple_to_bex_prompt_ast_simple(simple)?,
        ))),
        Some(PromptAstVariant::Message(message)) => {
            let content = message.content.ok_or_else(|| {
                CtypesError::InternalError("portable prompt message has no content".to_string())
            })?;
            let metadata = serde_json::from_str(&message.metadata_as_json).map_err(|error| {
                CtypesError::InternalError(format!(
                    "portable prompt message has invalid metadata JSON: {error}"
                ))
            })?;
            Ok(PromptAst::Message {
                role: message.role,
                content: Arc::new(proto_prompt_ast_simple_to_bex_prompt_ast_simple(content)?),
                metadata,
            })
        }
        Some(PromptAstVariant::Multiple(multiple)) => Ok(PromptAst::Vec(
            multiple
                .items
                .into_iter()
                .map(proto_prompt_ast_to_bex_prompt_ast)
                .map(|item| item.map(Arc::new))
                .collect::<Result<_, _>>()?,
        )),
        None => Err(CtypesError::InternalError(
            "portable prompt payload has no value".to_string(),
        )),
    }
}

fn proto_prompt_ast_simple_to_bex_prompt_ast_simple(
    simple: BamlValuePromptAstSimple,
) -> Result<PromptAstSimple, CtypesError> {
    match simple.value {
        Some(PromptAstSimpleVariant::String(string)) => Ok(PromptAstSimple::String(string)),
        Some(PromptAstSimpleVariant::Media(media)) => {
            Ok(PromptAstSimple::Media(proto_media_to_bex_media(media)?))
        }
        Some(PromptAstSimpleVariant::Multiple(multiple)) => Ok(PromptAstSimple::Multiple(
            multiple
                .items
                .into_iter()
                .map(proto_prompt_ast_simple_to_bex_prompt_ast_simple)
                .map(|item| item.map(Arc::new))
                .collect::<Result<_, _>>()?,
        )),
        None => Err(CtypesError::InternalError(
            "portable prompt content has no value".to_string(),
        )),
    }
}

fn convert_list(
    list: InboundListValue,
    element_type: RuntimeTy,
    handle_table: &CffiHandleTable,
    owners: &mut InboundOwners<'_>,
) -> Result<BexExternalValue, CtypesError> {
    let items: Result<Vec<BexExternalValue>, CtypesError> = list
        .values
        .into_iter()
        .map(|v| decode_inbound(v, handle_table, owners))
        .collect();
    Ok(BexExternalValue::Array {
        element_type,
        items: items?,
    })
}

fn convert_map(
    map: InboundMapValue,
    key_type: RuntimeTy,
    value_type: RuntimeTy,
    handle_table: &CffiHandleTable,
    owners: &mut InboundOwners<'_>,
) -> Result<BexExternalValue, CtypesError> {
    let mut entries = IndexMap::new();
    for entry in map.entries {
        let key = extract_string_key(&entry)?;
        let value = entry
            .value
            .map(|v| decode_inbound(v, handle_table, owners))
            .transpose()?
            .unwrap_or(BexExternalValue::Null);
        if entries.insert(key.clone(), value).is_some() {
            return Err(CtypesError::DuplicateKey(bounded(key)));
        }
    }
    Ok(BexExternalValue::Map {
        key_type,
        value_type,
        entries,
    })
}

fn convert_class(
    class: InboundClassValue,
    name: &baml_type::TypeName,
    type_args: Vec<RuntimeTy>,
    handle_table: &CffiHandleTable,
    owners: &mut InboundOwners<'_>,
) -> Result<BexExternalValue, CtypesError> {
    let mut fields = IndexMap::new();
    for entry in class.fields {
        let key = extract_string_key(&entry)?;
        let value = entry
            .value
            .map(|v| decode_inbound(v, handle_table, owners))
            .transpose()?
            .unwrap_or(BexExternalValue::Null);
        if fields.insert(key.clone(), value).is_some() {
            return Err(CtypesError::DuplicateKey(bounded(key)));
        }
    }
    Ok(BexExternalValue::Instance {
        class_name: name.to_string(),
        type_args,
        fields,
    })
}

fn convert_enum(e: InboundEnumValue) -> BexExternalValue {
    BexExternalValue::Variant {
        enum_name: e.name,
        variant_name: e.value,
    }
}

/// The key of a map entry, class field or keyword argument. The boundary
/// carries string keys only for now: a key of another kind is refused, never
/// spelled as a string (which would merge `1` with `"1"`). A typed key
/// (`map<Color, V>`, `map<int, V>`) is a new arm here when the engine takes
/// one.
fn extract_string_key(entry: &InboundMapEntry) -> Result<String, CtypesError> {
    use crate::baml_bridge::cffi::inbound_map_entry::Key;
    match &entry.key {
        Some(Key::StringKey(s)) => Ok(s.clone()),
        Some(Key::IntKey(_)) => Err(CtypesError::MapKeyNotString("int")),
        Some(Key::BoolKey(_)) => Err(CtypesError::MapKeyNotString("bool")),
        Some(Key::EnumKey(_)) => Err(CtypesError::MapKeyNotString("enum")),
        None => Err(CtypesError::MapEntryMissingKey),
    }
}

/// Decode protobuf kwargs into a `HashMap<String, BexExternalValue>` for engine call arguments.
pub fn kwargs_to_bex_values(
    kwargs: Vec<InboundMapEntry>,
    handle_table: &CffiHandleTable,
) -> Result<HashMap<String, BexExternalValue>, CtypesError> {
    let mut owners = InboundOwners::new(handle_table);
    for entry in &kwargs {
        if let Some(value) = &entry.value {
            owners.collect(value);
        }
    }
    owners.consistent()?;
    let mut result = HashMap::new();
    for entry in kwargs {
        let key = extract_string_key(&entry)?;
        let value = entry
            .value
            .map(|v| decode_inbound(v, handle_table, &mut owners))
            .transpose()?
            .unwrap_or(BexExternalValue::Null);
        if result.insert(key.clone(), value).is_some() {
            return Err(CtypesError::DuplicateKey(bounded(key)));
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{baml_bridge::cffi::BamlHandle, handle_table::CffiHandleTableEntry};

    fn typed_input(value_type: &RuntimeTy, value: InboundValueVariant) -> InboundValue {
        InboundValue {
            value_type: Some(crate::ty_encode::runtime_ty_to_proto_ty(value_type)),
            value: Some(value),
        }
    }

    fn owned_input(key: u64) -> InboundValue {
        InboundValue {
            value: Some(InboundValueVariant::Handle(BamlHandle {
                key,
                handle_type: BamlHandleType::FunctionRef as i32,
            })),
            ..Default::default()
        }
    }

    #[test]
    fn a_zero_handle_key_is_refused_for_every_handle_kind() {
        let table = CffiHandleTable::new();
        let callable_ty = RuntimeTy::Function {
            params: Box::new([]),
            ret: Box::new(RuntimeTy::int()),
            throws: Box::new(RuntimeTy::Never),
        };
        let callable = typed_input(
            &callable_ty,
            InboundValueVariant::Handle(BamlHandle {
                key: 0,
                handle_type: BamlHandleType::HostValueCallable as i32,
            }),
        );
        let opaque = InboundValue {
            value: Some(InboundValueVariant::Handle(BamlHandle {
                key: 0,
                handle_type: BamlHandleType::HostValueOpaque as i32,
            })),
            ..Default::default()
        };
        for input in [callable, opaque, owned_input(0)] {
            assert!(matches!(
                inbound_to_external(input, &table),
                Err(CtypesError::InvalidHandleKey(0))
            ));
        }
    }

    #[test]
    fn duplicate_keys_fields_and_kwargs_are_refused() {
        let table = CffiHandleTable::new();
        let int = |n: i64| InboundValue {
            value: Some(InboundValueVariant::IntValue(n)),
            ..Default::default()
        };
        let entry = |key: &str, value: InboundValue| InboundMapEntry {
            key: Some(crate::baml_bridge::cffi::inbound_map_entry::Key::StringKey(
                key.into(),
            )),
            value: Some(value),
        };
        let map = typed_input(
            &RuntimeTy::Map {
                key: Box::new(RuntimeTy::string()),
                value: Box::new(RuntimeTy::int()),
            },
            InboundValueVariant::MapValue(InboundMapValue {
                entries: vec![entry("a", int(1)), entry("b", int(2)), entry("a", int(3))],
            }),
        );
        assert!(matches!(
            inbound_to_external(map, &table),
            Err(CtypesError::DuplicateKey(key)) if key == "a"
        ));
        let class = typed_input(
            &RuntimeTy::Class(
                baml_type::TypeName::from_dotted_path("user.P"),
                Box::new([]),
            ),
            InboundValueVariant::ClassValue(InboundClassValue {
                fields: vec![entry("x", int(1)), entry("x", int(2))],
            }),
        );
        assert!(matches!(
            inbound_to_external(class, &table),
            Err(CtypesError::DuplicateKey(key)) if key == "x"
        ));
        assert!(matches!(
            kwargs_to_bex_values(vec![entry("n", int(1)), entry("n", int(2))], &table),
            Err(CtypesError::DuplicateKey(key)) if key == "n"
        ));
    }

    #[test]
    fn one_handle_key_under_two_kinds_is_refused() {
        let table = CffiHandleTable::new();
        let callable_ty = RuntimeTy::Function {
            params: Box::new([]),
            ret: Box::new(RuntimeTy::int()),
            throws: Box::new(RuntimeTy::Never),
        };
        let as_kind = |handle_type: BamlHandleType, stated: Option<&RuntimeTy>| InboundValue {
            value_type: stated.map(crate::ty_encode::runtime_ty_to_proto_ty),
            value: Some(InboundValueVariant::Handle(BamlHandle {
                key: 42,
                handle_type: handle_type as i32,
            })),
        };
        let input = typed_input(
            &RuntimeTy::list(RuntimeTy::unknown()),
            InboundValueVariant::ListValue(InboundListValue {
                values: vec![
                    as_kind(BamlHandleType::HostValueOpaque, None),
                    as_kind(BamlHandleType::HostValueCallable, Some(&callable_ty)),
                ],
            }),
        );
        assert!(matches!(
            inbound_to_external(input, &table),
            Err(CtypesError::HandleKindConflict(42))
        ));
    }

    #[test]
    fn malformed_list_releases_owners_before_and_after_bad_sibling() {
        let table = CffiHandleTable::new();
        let first = table.insert(CffiHandleTableEntry::FunctionRef { global_index: 1 });
        let last = table.insert(CffiHandleTableEntry::FunctionRef { global_index: 2 });
        let input = typed_input(
            &RuntimeTy::list(RuntimeTy::unknown()),
            InboundValueVariant::ListValue(InboundListValue {
                values: vec![
                    owned_input(first),
                    InboundValue {
                        value: Some(InboundValueVariant::BigintValue("invalid".into())),
                        ..Default::default()
                    },
                    owned_input(last),
                ],
            }),
        );
        assert!(inbound_to_external(input, &table).is_err());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn invalid_annotation_releases_nested_wire_owners() {
        let table = CffiHandleTable::new();
        let key = table.insert(CffiHandleTableEntry::FunctionRef { global_index: 1 });
        let input = typed_input(
            &RuntimeTy::Union(vec![RuntimeTy::int(), RuntimeTy::Null].into()),
            InboundValueVariant::ListValue(InboundListValue {
                values: vec![owned_input(key)],
            }),
        );
        assert!(inbound_to_external(input, &table).is_err());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn invalid_kwarg_releases_later_argument_owners() {
        let table = CffiHandleTable::new();
        let key = table.insert(CffiHandleTableEntry::FunctionRef { global_index: 1 });
        let inputs = vec![
            InboundMapEntry {
                key: None,
                value: Some(InboundValue::default()),
            },
            InboundMapEntry {
                key: Some(crate::baml_bridge::cffi::inbound_map_entry::Key::StringKey(
                    "later".into(),
                )),
                value: Some(owned_input(key)),
            },
        ];
        assert!(kwargs_to_bex_values(inputs, &table).is_err());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn a_scalar_implies_its_type_and_carries_no_statement() {
        let decoded = inbound_to_external(
            InboundValue {
                value_type: None,
                value: Some(InboundValueVariant::BoolValue(true)),
            },
            &CffiHandleTable::new(),
        )
        .unwrap();
        assert_eq!(decoded, BexExternalValue::Bool(true));

        let error = inbound_to_external(
            typed_input(&RuntimeTy::int(), InboundValueVariant::IntValue(7)),
            &CffiHandleTable::new(),
        )
        .unwrap_err();
        assert!(
            matches!(
                error,
                CtypesError::UnexpectedValueType { node: "int_value" }
            ),
            "{error}"
        );
        // A literal is a scalar in a type's clothing: no node states one.
        let literal = RuntimeTy::Literal(
            baml_type::Literal::String("draft".to_string()),
            baml_type::Freshness::Regular,
        );
        let error = inbound_to_external(
            typed_input(
                &literal,
                InboundValueVariant::StringValue("draft".to_string()),
            ),
            &CffiHandleTable::new(),
        )
        .unwrap_err();
        assert!(
            matches!(
                error,
                CtypesError::UnexpectedValueType {
                    node: "string_value"
                }
            ),
            "{error}"
        );
    }

    #[test]
    fn javascript_number_wire_variant_is_refused() {
        let error = inbound_to_external(
            InboundValue {
                value_type: None,
                value: Some(InboundValueVariant::JsNumberValue(7.0)),
            },
            &CffiHandleTable::new(),
        )
        .unwrap_err();
        assert!(
            matches!(error, CtypesError::JsNumberValueRetired),
            "{error}"
        );
    }

    #[test]
    fn portable_media_keeps_its_name_and_never_names_a_file() {
        let named = BamlValueMedia {
            media: MediaTypeEnum::Pdf as i32,
            mime_type: None,
            value: Some(baml_value_media::Value::FileContent(
                crate::baml_bridge::cffi::BamlValueMediaFileContent {
                    name: "q3.pdf".to_string(),
                    base64: "JVBERi0xLjc=".to_string(),
                },
            )),
        };
        let media = proto_media_to_bex_media(named).unwrap();
        assert_eq!(media.name().as_deref(), Some("q3.pdf"));
        assert_eq!(media.base64().as_str(), "JVBERi0xLjc=");
        // Content read from a file infers its MIME type from the name.
        assert_eq!(media.mime_type().as_deref(), Some("application/pdf"));
        assert_eq!(media.url(), None);

        let plain = BamlValueMedia {
            media: MediaTypeEnum::Image as i32,
            mime_type: None,
            value: Some(baml_value_media::Value::Base64("aW1hZ2U=".to_string())),
        };
        let media = proto_media_to_bex_media(plain).unwrap();
        assert_eq!((media.name(), media.mime_type()), (None, None));

        // Field 5 once carried a path. It is reserved: a payload that sets
        // only that has no content.
        let mut old_wire = Vec::new();
        prost::encoding::int32::encode(1, &(MediaTypeEnum::Image as i32), &mut old_wire);
        prost::encoding::string::encode(5, &"/etc/hosts".to_string(), &mut old_wire);
        let decoded = <BamlValueMedia as prost::Message>::decode(old_wire.as_slice()).unwrap();
        assert!(proto_media_to_bex_media(decoded).is_err());
    }

    #[test]
    fn portable_prompt_with_media_decodes_as_owned_adts() {
        use crate::baml_bridge::cffi::{
            BamlValuePromptAstMessage, BamlValuePromptAstMultiple, BamlValuePromptAstSimpleMultiple,
        };

        let image = BamlValueMedia {
            media: MediaTypeEnum::Image as i32,
            mime_type: Some("image/png".to_string()),
            value: Some(baml_value_media::Value::Base64("aW1hZ2U=".to_string())),
        };
        let content = BamlValuePromptAstSimple {
            value: Some(PromptAstSimpleVariant::Multiple(
                BamlValuePromptAstSimpleMultiple {
                    items: vec![
                        BamlValuePromptAstSimple {
                            value: Some(PromptAstSimpleVariant::String("look: ".to_string())),
                        },
                        BamlValuePromptAstSimple {
                            value: Some(PromptAstSimpleVariant::Media(image)),
                        },
                    ],
                },
            )),
        };
        let prompt = BamlValuePromptAst {
            value: Some(PromptAstVariant::Multiple(BamlValuePromptAstMultiple {
                items: vec![BamlValuePromptAst {
                    value: Some(PromptAstVariant::Message(BamlValuePromptAstMessage {
                        role: "user".to_string(),
                        content: Some(content),
                        metadata_as_json: "{\"cache\":true}".to_string(),
                    })),
                }],
            })),
        };

        for _ in 0..2 {
            let decoded = inbound_to_external(
                InboundValue {
                    value_type: None,
                    value: Some(InboundValueVariant::PromptAstValue(prompt.clone())),
                },
                &CffiHandleTable::new(),
            )
            .unwrap();
            let BexExternalValue::Adt(BexExternalAdt::PromptAst(ast)) = decoded else {
                panic!("expected a prompt AST payload")
            };
            let messages = ast.to_structured_messages();
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].0, "user");
            assert_eq!(messages[0].2["cache"], true);
            let PromptAstSimple::Multiple(parts) = messages[0].1.as_ref() else {
                panic!("expected ordered text and media parts")
            };
            let PromptAstSimple::Media(media) = parts[1].as_ref() else {
                panic!("expected media in the second prompt part")
            };
            assert_eq!(media.kind, MediaKind::Image);
            assert_eq!(media.mime_type().as_deref(), Some("image/png"));
            assert_eq!(media.base64().as_str(), "aW1hZ2U=");
        }
    }

    #[test]
    fn a_list_lands_with_its_stated_element_type() {
        let list_type = RuntimeTy::list(RuntimeTy::int());
        let decoded = inbound_to_external(
            typed_input(
                &list_type,
                InboundValueVariant::ListValue(InboundListValue { values: vec![] }),
            ),
            &CffiHandleTable::new(),
        )
        .unwrap();
        assert_eq!(
            decoded,
            BexExternalValue::Array {
                element_type: RuntimeTy::int(),
                items: vec![],
            }
        );
        // A union is a fine component type.
        let either = RuntimeTy::union([RuntimeTy::int(), RuntimeTy::string()]);
        let decoded = inbound_to_external(
            typed_input(
                &RuntimeTy::list(either.clone()),
                InboundValueVariant::ListValue(InboundListValue {
                    values: vec![InboundValue {
                        value_type: None,
                        value: Some(InboundValueVariant::IntValue(1)),
                    }],
                }),
            ),
            &CffiHandleTable::new(),
        )
        .unwrap();
        assert_eq!(
            decoded,
            BexExternalValue::Array {
                element_type: either,
                items: vec![BexExternalValue::Int(1)],
            }
        );
    }

    #[test]
    fn a_map_lands_with_its_stated_key_and_value_types() {
        use crate::baml_bridge::cffi::inbound_map_entry::Key;
        let map_type = RuntimeTy::Map {
            key: Box::new(RuntimeTy::string()),
            value: Box::new(RuntimeTy::float()),
        };
        let decoded = inbound_to_external(
            typed_input(
                &map_type,
                InboundValueVariant::MapValue(InboundMapValue {
                    entries: vec![InboundMapEntry {
                        key: Some(Key::StringKey("x".to_string())),
                        value: Some(InboundValue {
                            value_type: None,
                            value: Some(InboundValueVariant::FloatValue(1.5)),
                        }),
                    }],
                }),
            ),
            &CffiHandleTable::new(),
        )
        .unwrap();
        assert_eq!(
            decoded,
            BexExternalValue::Map {
                key_type: RuntimeTy::string(),
                value_type: RuntimeTy::float(),
                entries: IndexMap::from([("x".to_string(), BexExternalValue::Float(1.5))]),
            }
        );
    }

    #[test]
    fn a_container_without_a_statement_is_refused() {
        let table = CffiHandleTable::new();
        let list = InboundValue {
            value_type: None,
            value: Some(InboundValueVariant::ListValue(InboundListValue {
                values: vec![],
            })),
        };
        let error = inbound_to_external(list, &table).unwrap_err();
        assert!(
            matches!(
                error,
                CtypesError::MissingValueType {
                    node: "list_value",
                    ..
                }
            ),
            "{error}"
        );

        let map = InboundValue {
            value_type: None,
            value: Some(InboundValueVariant::MapValue(InboundMapValue {
                entries: vec![],
            })),
        };
        let error = inbound_to_external(map, &table).unwrap_err();
        assert!(
            matches!(
                error,
                CtypesError::MissingValueType {
                    node: "map_value",
                    ..
                }
            ),
            "{error}"
        );

        let class = InboundValue {
            value_type: None,
            value: Some(InboundValueVariant::ClassValue(InboundClassValue {
                fields: vec![],
            })),
        };
        let error = inbound_to_external(class, &table).unwrap_err();
        assert!(
            matches!(
                error,
                CtypesError::MissingValueType {
                    node: "class_value",
                    ..
                }
            ),
            "{error}"
        );
    }

    #[test]
    fn a_statement_of_another_shape_is_refused() {
        // A union, an optional, a scalar: none is the type of a list node.
        for stated in [
            RuntimeTy::union([RuntimeTy::list(RuntimeTy::int()), RuntimeTy::string()]),
            RuntimeTy::optional(RuntimeTy::list(RuntimeTy::int())),
            RuntimeTy::int(),
        ] {
            let error = inbound_to_external(
                typed_input(
                    &stated,
                    InboundValueVariant::ListValue(InboundListValue { values: vec![] }),
                ),
                &CffiHandleTable::new(),
            )
            .unwrap_err();
            assert!(
                matches!(
                    error,
                    CtypesError::ValueTypeShape {
                        node: "list_value",
                        expected: "a list type",
                        ..
                    }
                ),
                "{error}"
            );
        }
        let error = inbound_to_external(
            typed_input(
                &RuntimeTy::list(RuntimeTy::int()),
                InboundValueVariant::ClassValue(InboundClassValue { fields: vec![] }),
            ),
            &CffiHandleTable::new(),
        )
        .unwrap_err();
        assert!(
            matches!(
                error,
                CtypesError::ValueTypeShape {
                    node: "class_value",
                    expected: "a class type",
                    ..
                }
            ),
            "{error}"
        );
    }

    #[test]
    fn a_statement_that_mentions_a_type_variable_is_refused() {
        let open = RuntimeTy::list(RuntimeTy::TypeVar(baml_type::ParamTy::new(
            0,
            baml_type::Name::new("T"),
        )));
        let error = inbound_to_external(
            typed_input(
                &open,
                InboundValueVariant::ListValue(InboundListValue { values: vec![] }),
            ),
            &CffiHandleTable::new(),
        )
        .unwrap_err();
        assert!(
            matches!(error, CtypesError::ValueTypeNotRealized { ref var, .. } if var == "T"),
            "{error}"
        );
    }

    #[test]
    fn a_wire_type_with_a_hole_is_refused() {
        use crate::baml_bridge::cffi::{BamlTy, BamlTyList, baml_ty::Ty as BamlTyVariant};
        let holed = InboundValue {
            value_type: Some(BamlTy {
                ty: Some(BamlTyVariant::List(Box::new(BamlTyList { item: None }))),
            }),
            value: Some(InboundValueVariant::ListValue(InboundListValue {
                values: vec![],
            })),
        };
        let error = inbound_to_external(holed, &CffiHandleTable::new()).unwrap_err();
        assert!(
            matches!(
                error,
                CtypesError::IncompleteType {
                    position: "list's item type"
                }
            ),
            "{error}"
        );
        let empty = InboundValue {
            value_type: Some(BamlTy { ty: None }),
            value: Some(InboundValueVariant::ListValue(InboundListValue {
                values: vec![],
            })),
        };
        let error = inbound_to_external(empty, &CffiHandleTable::new()).unwrap_err();
        assert!(
            matches!(
                error,
                CtypesError::IncompleteType {
                    position: "variant"
                }
            ),
            "{error}"
        );
    }

    #[test]
    fn a_callable_lands_with_its_stated_function_type() {
        let table = CffiHandleTable::new();
        let handle = BamlHandle {
            key: 999,
            handle_type: BamlHandleType::HostValueCallable as i32,
        };
        let signature = RuntimeTy::Function {
            params: Box::new([baml_type::RuntimeFunctionParamTy::required(
                None,
                RuntimeTy::int(),
            )]),
            ret: Box::new(RuntimeTy::string()),
            throws: Box::new(RuntimeTy::Never),
        };
        let result = inbound_to_external(
            typed_input(&signature, InboundValueVariant::Handle(handle)),
            &table,
        )
        .expect("decode succeeds");
        let BexExternalValue::HostCallable {
            handle,
            params,
            ret,
            throws,
        } = result
        else {
            panic!("unexpected variant: {result:?}")
        };
        assert_eq!(handle.key, 999);
        assert_eq!(handle.kind, bex_project::HostValueKind::Callable);
        assert_eq!(
            RuntimeTy::Function {
                params,
                ret,
                throws
            },
            signature
        );
        // Key must NOT have been inserted into the handle table.
        assert!(
            table.resolve(999).is_none(),
            "HOST_VALUE_CALLABLE must not touch HANDLE_TABLE"
        );
    }

    #[test]
    fn a_callable_without_a_function_type_is_refused() {
        let table = CffiHandleTable::new();
        let handle = BamlHandle {
            key: 998,
            handle_type: BamlHandleType::HostValueCallable as i32,
        };
        let error = inbound_to_external(
            InboundValue {
                value_type: None,
                value: Some(InboundValueVariant::Handle(handle)),
            },
            &table,
        )
        .unwrap_err();
        assert!(
            matches!(
                error,
                CtypesError::MissingValueType {
                    node: "handle (callable)",
                    expected: "a function type"
                }
            ),
            "{error}"
        );
        let error = inbound_to_external(
            typed_input(&RuntimeTy::int(), InboundValueVariant::Handle(handle)),
            &table,
        )
        .unwrap_err();
        assert!(
            matches!(
                error,
                CtypesError::ValueTypeShape {
                    node: "handle (callable)",
                    ..
                }
            ),
            "{error}"
        );
    }

    #[test]
    fn decode_inbound_host_value_opaque() {
        let table = CffiHandleTable::new();
        let handle = BamlHandle {
            key: 777,
            handle_type: BamlHandleType::HostValueOpaque as i32,
        };
        let inbound = InboundValue {
            value_type: None,
            value: Some(InboundValueVariant::Handle(handle)),
        };
        let result = inbound_to_external(inbound, &table).expect("decode succeeds");
        match result {
            BexExternalValue::HostValue(arc) => {
                assert_eq!(arc.key, 777);
                assert_eq!(arc.kind, bex_project::HostValueKind::Opaque);
            }
            other => panic!("unexpected variant: {other:?}"),
        }
        // Like callables, opaque handles bypass the HANDLE_TABLE.
        assert!(
            table.resolve(777).is_none(),
            "HOST_VALUE_OPAQUE must not touch HANDLE_TABLE"
        );
    }

    #[test]
    fn inbound_handle_drains_table_entry() {
        let table = CffiHandleTable::new();
        let key = table.insert(CffiHandleTableEntry::FunctionRef { global_index: 7 });
        let inbound = InboundValue {
            value_type: None,
            value: Some(InboundValueVariant::Handle(BamlHandle {
                key,
                handle_type: 0,
            })),
        };
        let result = inbound_to_external(inbound, &table).expect("decode succeeds");
        assert!(matches!(
            result,
            BexExternalValue::FunctionRef { global_index: 7 }
        ));
        assert!(
            table.resolve(key).is_none(),
            "entry must be removed from table after drain"
        );
    }

    #[test]
    fn inbound_handle_missing_key_errors() {
        let table = CffiHandleTable::new();
        let inbound = InboundValue {
            value_type: None,
            value: Some(InboundValueVariant::Handle(BamlHandle {
                key: 9999,
                handle_type: 0,
            })),
        };
        let err = inbound_to_external(inbound, &table).expect_err("missing key should error");
        assert!(matches!(err, CtypesError::InvalidHandleKey(9999)));
    }

    /// Helper: round-trip a `BexExternalValue` through the outbound encoder and
    /// the inbound decoder by translating the outbound `bigint_value` discriminator
    /// to the parallel inbound discriminator.
    fn bigint_round_trip(original: &BexExternalValue) -> BexExternalValue {
        use crate::{
            baml_bridge::cffi::baml_outbound_value::Value as OutboundVariant,
            handle_table::CffiHandleTableOptions, value_encode::external_to_outbound,
        };

        let opts = CffiHandleTableOptions::for_in_process();
        let outbound = external_to_outbound(original, &opts).expect("encode succeeds");
        let s = match outbound.value {
            Some(OutboundVariant::BigintValue(s)) => s,
            other => panic!("expected outbound BigintValue, got {other:?}"),
        };
        let inbound = InboundValue {
            value_type: None,
            value: Some(InboundValueVariant::BigintValue(s)),
        };
        let table = CffiHandleTable::new();
        inbound_to_external(inbound, &table).expect("decode succeeds")
    }

    #[test]
    fn test_bigint_round_trip_small() {
        let bi = num_bigint::BigInt::from(42);
        let v = BexExternalValue::Bigint(bi);
        let decoded = bigint_round_trip(&v);
        assert_eq!(decoded, v);
    }

    #[test]
    fn test_bigint_round_trip_huge() {
        let bi = num_bigint::BigInt::parse_bytes(
            b"99999999999999999999999999999999999999999999999999",
            10,
        )
        .unwrap();
        let v = BexExternalValue::Bigint(bi);
        let decoded = bigint_round_trip(&v);
        assert_eq!(decoded, v);
    }

    #[test]
    fn test_bigint_round_trip_negative() {
        let bi = num_bigint::BigInt::from(-42);
        let v = BexExternalValue::Bigint(bi);
        let decoded = bigint_round_trip(&v);
        assert_eq!(decoded, v);
    }

    #[test]
    fn test_bigint_decode_invalid() {
        let inbound = InboundValue {
            value_type: None,
            value: Some(InboundValueVariant::BigintValue("not-a-number".into())),
        };
        let table = CffiHandleTable::new();
        let err = inbound_to_external(inbound, &table).expect_err("invalid bigint should error");
        assert!(matches!(err, CtypesError::InvalidBigint { len: 12 }));
        assert_eq!(err.to_string(), "Invalid bigint hex string (12 bytes)");
    }

    #[test]
    fn test_bigint_decode_too_long() {
        // Build a hex blob just over the FFI cap so the size check fires
        // before `parse_bytes`. The error must carry only the length, not the
        // input itself.
        let blob = "f".repeat((1 << 28) / 4 + 3);
        let blob_len = blob.len();
        let inbound = InboundValue {
            value_type: None,
            value: Some(InboundValueVariant::BigintValue(blob)),
        };
        let table = CffiHandleTable::new();
        let err = inbound_to_external(inbound, &table).expect_err("over-cap bigint should error");
        let CtypesError::InvalidBigint { len } = err else {
            panic!("expected InvalidBigint, got: {err:?}");
        };
        assert_eq!(len, blob_len);
        let message = err.to_string();
        assert_eq!(
            message,
            format!("Invalid bigint hex string ({blob_len} bytes)")
        );
        // Sanity: the error message does not embed the megabyte-scale input.
        assert!(message.len() < 200);
    }

    /// A class node states its class and type arguments once, in `value_type`;
    /// the node carries only the fields.
    #[test]
    fn a_class_lands_with_its_stated_class_and_arguments() {
        use crate::baml_bridge::cffi::{InboundClassValue, inbound_map_entry::Key};
        let class_name = baml_type::TypeName::from_dotted_path("user.GenericBox");
        let class_type = RuntimeTy::Class(class_name.clone(), Box::new([RuntimeTy::int()]));
        let decoded = inbound_to_external(
            typed_input(
                &class_type,
                InboundValueVariant::ClassValue(InboundClassValue {
                    fields: vec![InboundMapEntry {
                        key: Some(Key::StringKey("value".to_string())),
                        value: Some(InboundValue {
                            value_type: None,
                            value: Some(InboundValueVariant::IntValue(5)),
                        }),
                    }],
                }),
            ),
            &CffiHandleTable::new(),
        )
        .expect("decode succeeds");
        assert_eq!(
            decoded,
            BexExternalValue::Instance {
                class_name: class_name.to_string(),
                type_args: vec![RuntimeTy::int()],
                fields: IndexMap::from([("value".to_string(), BexExternalValue::Int(5))]),
            }
        );
    }
}
