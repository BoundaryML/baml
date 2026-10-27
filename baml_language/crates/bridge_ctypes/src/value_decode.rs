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
//!
//! A handle on the wire is one of two references, decided once from its tag
//! into a [`WireHandle`]: a host's reference to an engine object (a
//! [`TableKey`], transferred per occurrence) or the engine's reference to a
//! host object (a [`HostKey`], one registration however often it occurs). The
//! two are different types with different owners, and nothing after the tag
//! is read compares them.

use std::{
    collections::{HashMap, hash_map::Entry},
    sync::Arc,
};

use bex_project::{
    BexExternalAdt, BexExternalValue, Bounded, HostKey, HostValueArc, HostValueKind, MediaKind,
    MediaValue, PromptAst, PromptAstSimple, WireHead, WireTy,
};
use indexmap::IndexMap;

use crate::{
    baml_bridge::cffi::{
        BamlHandle, BamlHandleType, BamlTy, BamlTyArg, BamlValueMedia, BamlValuePromptAst,
        BamlValuePromptAstSimple, InboundClassValue, InboundEnumValue, InboundListValue,
        InboundMapEntry, InboundMapValue, InboundValue, MediaTypeEnum, baml_value_media,
        baml_value_prompt_ast::Value as PromptAstVariant,
        baml_value_prompt_ast_simple::Value as PromptAstSimpleVariant,
        inbound_value::Value as InboundValueVariant,
    },
    error::CtypesError,
    handle_table::{CffiHandleTable, TableKey},
};

/// A handle as the wire names it, decided once from `(key, handle_type)`.
///
/// Engine keys and host keys are references in opposite directions with
/// different owners (see [`CffiHandleTable`] and `bex_project::host_value`);
/// a number means nothing without its space, so nothing after this point sees
/// the bare `u64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireHandle {
    /// A host's reference to an engine object. Each occurrence transfers one
    /// reference, which the decoder drains.
    Engine(TableKey),
    /// The engine's reference to a host object the host registered. A key is
    /// one registration however often it occurs; the decoder interns it.
    Host(HostKey, HostValueKind),
}

impl WireHandle {
    /// The reference a wire handle's `(key, handle_type)` names.
    pub fn from_parts(key: u64, handle_type: i32) -> Result<Self, CtypesError> {
        let kind = match BamlHandleType::try_from(handle_type) {
            Ok(BamlHandleType::HostValueCallable) => Some(HostValueKind::Callable),
            Ok(BamlHandleType::HostValueOpaque) => Some(HostValueKind::Opaque),
            // Every other tag, known or not, names an engine object; the
            // table decides whether the key is live and of a usable kind.
            Ok(_) | Err(_) => None,
        };
        // Key 0 names no slot in either table. A host handle carrying it
        // would reach the host as a callback frame whose callable identity
        // the frame decoder refuses, and the call would never complete.
        match kind {
            Some(kind) => HostKey::new(key).map(|key| Self::Host(key, kind)),
            None => TableKey::new(key).map(Self::Engine),
        }
        .ok_or(CtypesError::InvalidHandleKey(0))
    }
}

impl TryFrom<&BamlHandle> for WireHandle {
    type Error = CtypesError;

    fn try_from(handle: &BamlHandle) -> Result<Self, CtypesError> {
        Self::from_parts(handle.key, handle.handle_type)
    }
}

impl TryFrom<&crate::baml_bridge::cffi::BamlOutboundHandle> for WireHandle {
    type Error = CtypesError;

    fn try_from(
        handle: &crate::baml_bridge::cffi::BamlOutboundHandle,
    ) -> Result<Self, CtypesError> {
        Self::from_parts(handle.key, handle.handle_type)
    }
}

/// The transfers a request carries that the engine has not yet taken over.
///
/// A request is decoded in two passes: [`Self::collect`] walks every value and
/// records what decoding will consume, then decoding consumes it. Whatever is
/// left when the journal drops — after a refusal anywhere in the request,
/// including a malformed sibling — is released, so a request consumes
/// everything it carried, on success and on every failure, and nothing is
/// released twice. Each reference table keeps its own unit: an engine
/// reference is transferred per occurrence, a host registration once per key
/// however often the key occurs. One key arriving under two host kinds names
/// two different things with one identity and is refused before anything is
/// consumed.
pub struct InboundOwners<'a> {
    table: &'a CffiHandleTable,
    /// Engine references not yet drained, counted per occurrence.
    engine: HashMap<TableKey, usize>,
    /// Host registrations not yet interned, with the kind each arrived under.
    host: HashMap<HostKey, HostValueKind>,
}

impl<'a> InboundOwners<'a> {
    pub fn new(table: &'a CffiHandleTable) -> Self {
        Self {
            table,
            engine: HashMap::new(),
            host: HashMap::new(),
        }
    }

    /// Record every handle under `value` before anything is decoded. A
    /// handle that cannot be recorded (key 0, a key under two host kinds) is
    /// the request's refusal, reported once the whole value has been walked
    /// so every other transfer is still journaled and released with it.
    pub fn collect(&mut self, value: &InboundValue) -> Result<(), CtypesError> {
        let mut first_fault = None;
        self.walk(value, &mut first_fault);
        first_fault.map_or(Ok(()), Err)
    }

    /// [`Self::collect`] over the values of map entries, class fields or
    /// keyword arguments.
    pub fn collect_entries(&mut self, entries: &[InboundMapEntry]) -> Result<(), CtypesError> {
        let mut first_fault = None;
        self.walk_entries(entries, &mut first_fault);
        first_fault.map_or(Ok(()), Err)
    }

    /// Record every `live` head under `ty`: each occurrence is one engine
    /// reference the statement transfers, like a handle's. The keys are read
    /// off the raw message, so a statement the parse refuses still has every
    /// reference it transferred journaled and released with the refusal.
    fn walk_ty(&mut self, ty: &BamlTy, first_fault: &mut Option<CtypesError>) {
        crate::ty_decode::live_keys(ty, &mut |key| {
            *self.engine.entry(key).or_insert(0) += 1;
        });
        if let Err(fault) = crate::ty_decode::proto_ty_to_wire_ty(ty)
            && first_fault.is_none()
        {
            *first_fault = Some(fault);
        }
    }

    /// Record every `live` head in a call's explicit type arguments before
    /// anything is decoded: each occurrence is one engine reference the
    /// request transfers, released with the rest if the request is refused.
    pub fn collect_type_args(&mut self, type_args: &[BamlTyArg]) -> Result<(), CtypesError> {
        let mut first_fault = None;
        for arg in type_args {
            if let Some(ty) = &arg.type_value {
                self.walk_ty(ty, &mut first_fault);
            }
        }
        first_fault.map_or(Ok(()), Err)
    }

    fn walk(&mut self, value: &InboundValue, first_fault: &mut Option<CtypesError>) {
        if let Some(stated) = &value.value_type {
            self.walk_ty(stated, first_fault);
        }
        match &value.value {
            Some(InboundValueVariant::Handle(handle)) => {
                if let Err(fault) = self.record(handle)
                    && first_fault.is_none()
                {
                    *first_fault = Some(fault);
                }
            }
            Some(InboundValueVariant::ListValue(list)) => {
                for value in &list.values {
                    self.walk(value, first_fault);
                }
            }
            Some(InboundValueVariant::MapValue(map)) => {
                self.walk_entries(&map.entries, first_fault);
            }
            Some(InboundValueVariant::ClassValue(class)) => {
                self.walk_entries(&class.fields, first_fault);
            }
            Some(InboundValueVariant::TyValue(ty)) => self.walk_ty(ty, first_fault),
            Some(
                InboundValueVariant::StringValue(_)
                | InboundValueVariant::IntValue(_)
                | InboundValueVariant::BigintValue(_)
                | InboundValueVariant::FloatValue(_)
                | InboundValueVariant::JsNumberValue(_)
                | InboundValueVariant::BoolValue(_)
                | InboundValueVariant::EnumValue(_)
                | InboundValueVariant::Uint8arrayValue(_)
                | InboundValueVariant::MediaValue(_)
                | InboundValueVariant::PromptAstValue(_),
            )
            | None => {}
        }
    }

    fn walk_entries(&mut self, entries: &[InboundMapEntry], first_fault: &mut Option<CtypesError>) {
        for value in entries.iter().filter_map(|entry| entry.value.as_ref()) {
            self.walk(value, first_fault);
        }
    }

    fn record(&mut self, handle: &BamlHandle) -> Result<(), CtypesError> {
        match WireHandle::try_from(handle)? {
            WireHandle::Engine(key) => *self.engine.entry(key).or_insert(0) += 1,
            WireHandle::Host(key, kind) => match self.host.entry(key) {
                Entry::Vacant(slot) => {
                    slot.insert(kind);
                }
                Entry::Occupied(slot) if *slot.get() == kind => {}
                Entry::Occupied(_) => return Err(CtypesError::HandleKindConflict(key.get())),
            },
        }
        Ok(())
    }

    /// One occurrence of `key` was drained into a value.
    fn drained(&mut self, key: TableKey) {
        if let Entry::Occupied(mut slot) = self.engine.entry(key) {
            *slot.get_mut() -= 1;
            if *slot.get() == 0 {
                slot.remove();
            }
        }
    }

    /// `key` was interned into a value; the engine now holds the registration.
    fn interned(&mut self, key: HostKey) {
        self.host.remove(&key);
    }

    /// Decode one collected value.
    pub fn decode(&mut self, value: InboundValue) -> Result<BexExternalValue, CtypesError> {
        decode_inbound(value, self.table, self)
    }

    /// Decode a collected type statement: every `live` head drains the row it
    /// references, which must be a reference to an engine object.
    pub fn decode_ty(&mut self, ty: &BamlTy) -> Result<WireTy, CtypesError> {
        crate::ty_decode::proto_ty_to_wire_ty(ty)?.try_map_live(&mut |key: &TableKey| {
            let row = self
                .table
                .resolve(*key)
                .ok_or(CtypesError::InvalidHandleKey(key.get()))?;
            let crate::CffiHandleTableEntry::BexHeapHandle(handle) = &*row else {
                return Err(CtypesError::LiveHeadNotADeclaration(key.get()));
            };
            let handle = handle.clone();
            self.table
                .drain(*key)
                .ok_or(CtypesError::InvalidHandleKey(key.get()))?;
            self.drained(*key);
            Ok(handle)
        })
    }

    /// Decode a call's collected type arguments.
    pub fn decode_type_args(
        &mut self,
        type_args: &[BamlTyArg],
    ) -> Result<IndexMap<String, WireTy>, CtypesError> {
        let mut decoded = IndexMap::new();
        for arg in type_args {
            let ty = arg.type_value.as_ref().ok_or(CtypesError::IncompleteType {
                position: "type argument's type",
            })?;
            decoded.insert(arg.type_var.clone(), self.decode_ty(ty)?);
        }
        Ok(decoded)
    }

    /// Decode collected keyword arguments into engine call arguments.
    pub fn decode_entries(
        &mut self,
        entries: Vec<InboundMapEntry>,
    ) -> Result<HashMap<String, BexExternalValue>, CtypesError> {
        let mut result = HashMap::new();
        for entry in entries {
            let key = extract_string_key(&entry)?;
            let value = entry
                .value
                .map(|value| self.decode(value))
                .transpose()?
                .unwrap_or(BexExternalValue::Null);
            if result.insert(key.clone(), value).is_some() {
                return Err(CtypesError::DuplicateKey(Bounded::text(key)));
            }
        }
        Ok(result)
    }
}

impl Drop for InboundOwners<'_> {
    fn drop(&mut self) {
        for (key, occurrences) in self.engine.drain() {
            for _ in 0..occurrences {
                self.table.release(key);
            }
        }
        // A key never interned owes the host one release: a fresh identity
        // dropped at once fires it. A key the engine already holds, under this
        // kind or another, was never this request's to release.
        for (key, kind) in self.host.drain() {
            if let Ok(arc) = HostValueArc::try_intern(key, kind) {
                drop(arc);
            }
        }
    }
}

/// Decode a protobuf `InboundValue` into a `BexExternalValue` for use by the BEX engine.
///
/// Handles are resolved via `handle_table`; an unknown key returns `InvalidHandleKey`.
/// A value that is one of several in a request shares the request's
/// [`InboundOwners`] instead.
pub fn inbound_to_external(
    value: InboundValue,
    handle_table: &CffiHandleTable,
) -> Result<BexExternalValue, CtypesError> {
    let mut owners = InboundOwners::new(handle_table);
    owners.collect(&value)?;
    owners.decode(value)
}

fn decode_inbound(
    value: InboundValue,
    handle_table: &CffiHandleTable,
    owners: &mut InboundOwners<'_>,
) -> Result<BexExternalValue, CtypesError> {
    let stated = value
        .value_type
        .as_ref()
        .map(|ty| owners.decode_ty(ty))
        .transpose()?;
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
                WireTy::List(element) => *element,
                other => return Err(shape("list_value", "a list type", &other)),
            };
            convert_list(list, element, handle_table, owners)
        }
        InboundValueVariant::MapValue(map) => {
            let (key, value) = match required(stated, "map_value", "a map type")? {
                WireTy::Map { key, value } => (*key, *value),
                other => return Err(shape("map_value", "a map type", &other)),
            };
            convert_map(map, key, value, handle_table, owners)
        }
        InboundValueVariant::ClassValue(class) => {
            let (name, type_args) = match required(stated, "class_value", "a class type")? {
                WireTy::Class(name, type_args) => (name, type_args),
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
            Ok(BexExternalValue::Adt(BexExternalAdt::WireType(
                owners.decode_ty(&ty)?,
            )))
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
        InboundValueVariant::Handle(handle) => match WireHandle::try_from(&handle)? {
            WireHandle::Host(key, HostValueKind::Callable) => {
                // A callable has no type of its own on the wire: the host
                // states the function type it inhabits.
                let (params, ret, throws) =
                    match required(stated, "handle (callable)", "a function type")? {
                        WireTy::Function {
                            params,
                            ret,
                            throws,
                        } => (params, ret, throws),
                        other => {
                            return Err(shape("handle (callable)", "a function type", &other));
                        }
                    };
                let handle = intern_host_value(key, HostValueKind::Callable, owners)?;
                Ok(BexExternalValue::HostCallable {
                    handle,
                    params,
                    ret,
                    throws,
                })
            }
            WireHandle::Host(key, HostValueKind::Opaque) => {
                implied(stated.as_ref(), "handle")?;
                Ok(BexExternalValue::HostValue(intern_host_value(
                    key,
                    HostValueKind::Opaque,
                    owners,
                )?))
            }
            WireHandle::Engine(key) => {
                implied(stated.as_ref(), "handle")?;
                // Internal invocation capabilities are control-channel data,
                // not application values, even if the host forges their tag.
                let pinned = handle_table
                    .resolve(key)
                    .ok_or(CtypesError::InvalidHandleKey(key.get()))?;
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
                    .drain(key)
                    .ok_or(CtypesError::InvalidHandleKey(key.get()))?;
                owners.drained(key);
                Ok(BexExternalValue::from((*value).clone()))
            }
        },
    }
}

/// The engine's one identity for a host-registered value: repeated decodes of
/// the same key (the host passes one registration in several requests, or
/// twice in one) share one `Arc`, so the host is told to release exactly
/// once, at the last drop anywhere in the process. A key live under another
/// kind is refused; nothing is minted for it.
fn intern_host_value(
    key: HostKey,
    kind: HostValueKind,
    owners: &mut InboundOwners<'_>,
) -> Result<Arc<HostValueArc>, CtypesError> {
    let arc = HostValueArc::try_intern(key, kind)
        .map_err(|_live| CtypesError::HandleKindConflict(key.get()))?;
    owners.interned(key);
    Ok(arc)
}

/// A node whose type the wire implies carries no statement.
fn implied(stated: Option<&WireTy>, node: &'static str) -> Result<(), CtypesError> {
    match stated {
        None => Ok(()),
        Some(_) => Err(CtypesError::UnexpectedValueType { node }),
    }
}

/// The statement a list, map, class or callable node has to carry.
fn required(
    stated: Option<WireTy>,
    node: &'static str,
    expected: &'static str,
) -> Result<WireTy, CtypesError> {
    stated.ok_or(CtypesError::MissingValueType { node, expected })
}

/// A statement of another shape than the node it is on.
fn shape(node: &'static str, expected: &'static str, stated: &WireTy) -> CtypesError {
    CtypesError::ValueTypeShape {
        node,
        expected,
        stated: Bounded::ty(stated.to_string()),
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
    element_type: WireTy,
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
    key_type: WireTy,
    value_type: WireTy,
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
            return Err(CtypesError::DuplicateKey(Bounded::text(key)));
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
    head: &WireHead<bex_project::Handle>,
    type_args: Vec<WireTy>,
    handle_table: &CffiHandleTable,
    owners: &mut InboundOwners<'_>,
) -> Result<BexExternalValue, CtypesError> {
    // An instance by fields is an instance of a compiled class; a runtime
    // class's instances cross by reference.
    let name = match head {
        WireHead::Static(name) => name,
        WireHead::Live(_) | WireHead::Anonymous(_) => {
            return Err(CtypesError::ClassValueOfRuntimeClass);
        }
    };
    let mut fields = IndexMap::new();
    for entry in class.fields {
        let key = extract_string_key(&entry)?;
        let value = entry
            .value
            .map(|v| decode_inbound(v, handle_table, owners))
            .transpose()?
            .unwrap_or(BexExternalValue::Null);
        if fields.insert(key.clone(), value).is_some() {
            return Err(CtypesError::DuplicateKey(Bounded::text(key)));
        }
    }
    Ok(BexExternalValue::Instance {
        class_name: name.to_string(),
        type_args,
        fields,
    })
}

/// An enum variant, its enum spelled canonically: the host's `Color` or
/// `user.Color` both name the root package's `Color`, as a stated type's
/// `name` does, so the engine resolves one spelling and needs no retry.
fn convert_enum(e: InboundEnumValue) -> BexExternalValue {
    BexExternalValue::Variant {
        enum_name: baml_type::TypeName::from_dotted_path(&e.name).to_string(),
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
///
/// Arguments that are part of a larger request share the request's
/// [`InboundOwners`] instead.
pub fn kwargs_to_bex_values(
    kwargs: Vec<InboundMapEntry>,
    handle_table: &CffiHandleTable,
) -> Result<HashMap<String, BexExternalValue>, CtypesError> {
    let mut owners = InboundOwners::new(handle_table);
    owners.collect_entries(&kwargs)?;
    owners.decode_entries(kwargs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{baml_bridge::cffi::BamlHandle, handle_table::CffiHandleTableEntry};

    /// A stated type on the wire; a test states compiled declarations only.
    fn proto_of(ty: &WireTy) -> crate::baml_bridge::cffi::BamlTy {
        crate::ty_encode::wire_ty_to_proto_ty(ty, &mut |_: &bex_project::Handle| {
            unreachable!("a test states no runtime declaration")
        })
    }

    fn typed_input(value_type: &WireTy, value: InboundValueVariant) -> InboundValue {
        InboundValue {
            value_type: Some(proto_of(value_type)),
            value: Some(value),
        }
    }

    fn owned_input(key: TableKey) -> InboundValue {
        InboundValue {
            value: Some(InboundValueVariant::Handle(BamlHandle {
                key: key.get(),
                handle_type: BamlHandleType::AdtMediaGeneric as i32,
            })),
            ..Default::default()
        }
    }

    /// Any engine row a test can mint without a heap: a media value.
    fn media_row() -> CffiHandleTableEntry {
        CffiHandleTableEntry::Adt(BexExternalAdt::Media(MediaValue::from_url(
            MediaKind::Generic,
            "https://example.com/",
            None,
        )))
    }

    /// A no-op heap for minting real engine-heap handles without a VM.
    struct StubHeap;

    impl bex_project::WeakHeapRef for StubHeap {
        fn release_handle(&self, _slab_key: usize) {}

        fn resolve_handle_ptr(&self, _slab_key: usize) -> Option<bex_project::HeapPtr> {
            None
        }
    }

    fn callable_ty() -> WireTy {
        WireTy::Function {
            params: Box::new([]),
            ret: Box::new(WireTy::int()),
            throws: Box::new(WireTy::Never),
        }
    }

    fn host_input(key: u64, kind: BamlHandleType) -> InboundValue {
        let value_type = match kind {
            BamlHandleType::HostValueCallable => Some(proto_of(&callable_ty())),
            _ => None,
        };
        InboundValue {
            value_type,
            value: Some(InboundValueVariant::Handle(BamlHandle {
                key,
                handle_type: kind as i32,
            })),
        }
    }

    /// The engine table and a host registry number their references
    /// independently, so one number is routinely two different references.
    #[test]
    fn an_engine_reference_and_a_host_registration_may_share_a_number() {
        let table = CffiHandleTable::new();
        let engine = table.insert(media_row());
        let input = typed_input(
            &WireTy::list(WireTy::unknown()),
            InboundValueVariant::ListValue(InboundListValue {
                values: vec![
                    owned_input(engine),
                    host_input(engine.get(), BamlHandleType::HostValueCallable),
                ],
            }),
        );
        let decoded = inbound_to_external(input, &table).expect("two references, one number");
        let BexExternalValue::Array { items, .. } = decoded else {
            panic!("expected a list, got {decoded:?}")
        };
        assert!(matches!(
            items[0],
            BexExternalValue::Adt(BexExternalAdt::Media(_))
        ));
        let BexExternalValue::HostCallable { handle, .. } = &items[1] else {
            panic!("expected a host callable, got {:?}", items[1])
        };
        assert_eq!(handle.key.get(), engine.get());
        assert!(table.is_empty(), "the engine reference was drained");
    }

    /// Every occurrence of an engine key is one reference the host added, so
    /// a refusal releases one per occurrence and a success drains one each.
    #[test]
    fn a_shared_engine_reference_is_released_once_per_occurrence() {
        let table = CffiHandleTable::new();
        let heap: Arc<dyn bex_project::WeakHeapRef> = Arc::new(StubHeap);
        let twice = |slab: usize| {
            let object = bex_project::Handle::new(slab, Arc::clone(&heap));
            let key = table.insert(CffiHandleTableEntry::BexHeapHandle(object.clone()));
            assert_eq!(
                table.insert(CffiHandleTableEntry::BexHeapHandle(object)),
                key,
                "one object, one key"
            );
            assert_eq!(table.refcount(key), Some(2));
            key
        };
        let key = twice(7);
        let refused = typed_input(
            &WireTy::list(WireTy::unknown()),
            InboundValueVariant::ListValue(InboundListValue {
                values: vec![
                    owned_input(key),
                    InboundValue {
                        value: Some(InboundValueVariant::BigintValue("invalid".into())),
                        ..Default::default()
                    },
                    owned_input(key),
                ],
            }),
        );
        assert!(inbound_to_external(refused, &table).is_err());
        assert!(table.is_empty(), "both occurrences were released");

        let key = twice(8);
        let accepted = typed_input(
            &WireTy::list(WireTy::unknown()),
            InboundValueVariant::ListValue(InboundListValue {
                values: vec![owned_input(key), owned_input(key)],
            }),
        );
        assert!(inbound_to_external(accepted, &table).is_ok());
        assert!(table.is_empty(), "both occurrences were drained");
    }

    /// A host key's kind is fixed while the engine holds it: a later decode
    /// under the other kind is refused, and refusing it mints nothing.
    #[test]
    fn a_host_key_live_under_another_kind_is_refused_across_decodes() {
        let table = CffiHandleTable::new();
        let live = inbound_to_external(host_input(4242, BamlHandleType::HostValueOpaque), &table)
            .expect("an opaque host value decodes");
        let error =
            inbound_to_external(host_input(4242, BamlHandleType::HostValueCallable), &table)
                .expect_err("the key is live as an opaque value");
        assert!(
            matches!(error, CtypesError::HandleKindConflict(4242)),
            "{error}"
        );
        drop(live);
    }

    #[test]
    fn a_zero_handle_key_is_refused_for_every_handle_kind() {
        let table = CffiHandleTable::new();
        let callable_ty = WireTy::Function {
            params: Box::new([]),
            ret: Box::new(WireTy::int()),
            throws: Box::new(WireTy::Never),
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
        let engine = InboundValue {
            value: Some(InboundValueVariant::Handle(BamlHandle {
                key: 0,
                handle_type: BamlHandleType::AdtMediaGeneric as i32,
            })),
            ..Default::default()
        };
        for input in [callable, opaque, engine] {
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
            &WireTy::Map {
                key: Box::new(WireTy::string()),
                value: Box::new(WireTy::int()),
            },
            InboundValueVariant::MapValue(InboundMapValue {
                entries: vec![entry("a", int(1)), entry("b", int(2)), entry("a", int(3))],
            }),
        );
        assert!(matches!(
            inbound_to_external(map, &table),
            Err(CtypesError::DuplicateKey(key)) if key.as_str() == "a"
        ));
        let class = typed_input(
            &WireTy::Class(
                WireHead::Static(baml_type::TypeName::from_dotted_path("user.P")),
                Box::new([]),
            ),
            InboundValueVariant::ClassValue(InboundClassValue {
                fields: vec![entry("x", int(1)), entry("x", int(2))],
            }),
        );
        assert!(matches!(
            inbound_to_external(class, &table),
            Err(CtypesError::DuplicateKey(key)) if key.as_str() == "x"
        ));
        assert!(matches!(
            kwargs_to_bex_values(vec![entry("n", int(1)), entry("n", int(2))], &table),
            Err(CtypesError::DuplicateKey(key)) if key.as_str() == "n"
        ));
    }

    #[test]
    fn one_handle_key_under_two_kinds_is_refused() {
        let table = CffiHandleTable::new();
        let callable_ty = WireTy::Function {
            params: Box::new([]),
            ret: Box::new(WireTy::int()),
            throws: Box::new(WireTy::Never),
        };
        let as_kind = |handle_type: BamlHandleType, stated: Option<&WireTy>| InboundValue {
            value_type: stated.map(proto_of),
            value: Some(InboundValueVariant::Handle(BamlHandle {
                key: 42,
                handle_type: handle_type as i32,
            })),
        };
        let input = typed_input(
            &WireTy::list(WireTy::unknown()),
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
        let first = table.insert(media_row());
        let last = table.insert(media_row());
        let input = typed_input(
            &WireTy::list(WireTy::unknown()),
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
        let key = table.insert(media_row());
        let input = typed_input(
            &WireTy::Union(vec![WireTy::int(), WireTy::Null].into()),
            InboundValueVariant::ListValue(InboundListValue {
                values: vec![owned_input(key)],
            }),
        );
        assert!(inbound_to_external(input, &table).is_err());
        assert_eq!(table.len(), 0);
    }

    /// A statement the parse refuses (a class with an empty type argument)
    /// still transferred one reference per `live` head: the refusal releases
    /// each.
    #[test]
    fn a_refused_statement_releases_the_live_heads_it_carries() {
        use crate::baml_bridge::cffi::{BamlTy, BamlTyClass, baml_ty, baml_ty_class};
        let table = CffiHandleTable::new();
        let heap: Arc<dyn bex_project::WeakHeapRef> = Arc::new(StubHeap);
        let declaration = table.insert(CffiHandleTableEntry::BexHeapHandle(
            bex_project::Handle::new(9, Arc::clone(&heap)),
        ));
        let element = table.insert(media_row());
        let malformed = BamlTy {
            ty: Some(baml_ty::Ty::ClassTy(BamlTyClass {
                head: Some(baml_ty_class::Head::Live(declaration.get())),
                type_args: vec![BamlTy { ty: None }],
            })),
        };
        let input = InboundValue {
            value_type: Some(BamlTy {
                ty: Some(baml_ty::Ty::List(Box::new(
                    crate::baml_bridge::cffi::BamlTyList {
                        item: Some(Box::new(malformed)),
                    },
                ))),
            }),
            value: Some(InboundValueVariant::ListValue(InboundListValue {
                values: vec![owned_input(element)],
            })),
        };
        let error = inbound_to_external(input, &table).expect_err("an empty type is refused");
        assert!(
            matches!(error, CtypesError::IncompleteType { .. }),
            "{error}"
        );
        assert!(
            table.is_empty(),
            "the live head and the element were both released"
        );
    }

    #[test]
    fn invalid_kwarg_releases_later_argument_owners() {
        let table = CffiHandleTable::new();
        let key = table.insert(media_row());
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
            typed_input(&WireTy::int(), InboundValueVariant::IntValue(7)),
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
        let literal = WireTy::Literal(
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
        let list_type = WireTy::list(WireTy::int());
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
                element_type: WireTy::int(),
                items: vec![],
            }
        );
        // A union is a fine component type.
        let either = WireTy::Union(Box::new([WireTy::int(), WireTy::string()]));
        let decoded = inbound_to_external(
            typed_input(
                &WireTy::list(either.clone()),
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
        let map_type = WireTy::Map {
            key: Box::new(WireTy::string()),
            value: Box::new(WireTy::Float),
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
                key_type: WireTy::string(),
                value_type: WireTy::Float,
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
            WireTy::Union(Box::new([WireTy::list(WireTy::int()), WireTy::string()])),
            WireTy::optional(WireTy::list(WireTy::int())),
            WireTy::int(),
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
                &WireTy::list(WireTy::int()),
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
        let signature = WireTy::Function {
            params: Box::new([baml_type::RealizedFunctionParamTy::required(
                None,
                WireTy::int(),
            )]),
            ret: Box::new(WireTy::string()),
            throws: Box::new(WireTy::Never),
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
        assert_eq!(handle.key.get(), 999);
        assert_eq!(handle.kind, bex_project::HostValueKind::Callable);
        assert_eq!(
            WireTy::Function {
                params,
                ret,
                throws
            },
            signature
        );
        // Key must NOT have been inserted into the handle table.
        assert!(
            table.resolve(TableKey::new(999).unwrap()).is_none(),
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
            typed_input(&WireTy::int(), InboundValueVariant::Handle(handle)),
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
                assert_eq!(arc.key.get(), 777);
                assert_eq!(arc.kind, bex_project::HostValueKind::Opaque);
            }
            other => panic!("unexpected variant: {other:?}"),
        }
        // Like callables, opaque handles bypass the HANDLE_TABLE.
        assert!(
            table.resolve(TableKey::new(777).unwrap()).is_none(),
            "HOST_VALUE_OPAQUE must not touch HANDLE_TABLE"
        );
    }

    #[test]
    fn inbound_handle_drains_table_entry() {
        let table = CffiHandleTable::new();
        let key = table.insert(media_row());
        let inbound = InboundValue {
            value_type: None,
            value: Some(InboundValueVariant::Handle(BamlHandle {
                key: key.get(),
                handle_type: 0,
            })),
        };
        let result = inbound_to_external(inbound, &table).expect("decode succeeds");
        assert!(matches!(
            result,
            BexExternalValue::Adt(BexExternalAdt::Media(_))
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
        let class_type = WireTy::Class(
            WireHead::Static(class_name.clone()),
            Box::new([WireTy::int()]),
        );
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
                type_args: vec![WireTy::int()],
                fields: IndexMap::from([("value".to_string(), BexExternalValue::Int(5))]),
            }
        );
    }
}
