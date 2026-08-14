//! Artifact-safe encoding for trace-owned value snapshots.

use baml_type::{FunctionParamMode, Literal, MediaKind, RuntimeTy};
use num_bigint::BigInt;
use prost::{Enumeration, Message};

use crate::trace_heap::{
    TraceMediaContent, TraceMediaValue, TraceOmissionDescriptor, TraceOmissionReason,
    TraceSnapshot, TraceValue, TraceValueRef,
};

#[derive(Clone, PartialEq, Message)]
struct BamlOutboundValue {
    #[prost(
        oneof = "BamlValueVariant",
        tags = "2, 3, 4, 5, 6, 7, 8, 11, 12, 17, 19, 20"
    )]
    value: Option<BamlValueVariant>,
}

#[derive(Clone, PartialEq, ::prost::Oneof)]
#[expect(
    clippy::enum_variant_names,
    reason = "Variant names mirror the protobuf value oneof fields."
)]
enum BamlValueVariant {
    #[prost(message, tag = "2")]
    NullValue(BamlValueNull),
    #[prost(string, tag = "3")]
    StringValue(String),
    #[prost(int64, tag = "4")]
    IntValue(i64),
    #[prost(double, tag = "5")]
    FloatValue(f64),
    #[prost(bool, tag = "6")]
    BoolValue(bool),
    #[prost(message, tag = "7")]
    ClassValue(BamlValueClass),
    #[prost(message, tag = "8")]
    EnumValue(BamlValueEnum),
    #[prost(message, tag = "11")]
    ListValue(BamlValueList),
    #[prost(message, tag = "12")]
    MapValue(BamlValueMap),
    #[prost(message, tag = "17")]
    MediaValue(BamlValueMedia),
    #[prost(bytes, tag = "19")]
    Uint8arrayValue(Vec<u8>),
    #[prost(string, tag = "20")]
    BigintValue(String),
}

#[derive(Clone, PartialEq, Message)]
#[expect(
    clippy::empty_structs_with_brackets,
    reason = "prost empty message structs use braced form."
)]
struct BamlValueNull {}

#[derive(Clone, PartialEq, Message)]
struct BamlValueList {
    #[prost(message, repeated, tag = "2")]
    items: Vec<BamlOutboundValue>,
}

#[derive(Clone, PartialEq, Message)]
struct BamlOutboundMapEntry {
    #[prost(string, tag = "1")]
    key: String,
    #[prost(message, optional, tag = "2")]
    value: Option<BamlOutboundValue>,
}

#[derive(Clone, PartialEq, Message)]
struct BamlValueMap {
    #[prost(message, repeated, tag = "3")]
    entries: Vec<BamlOutboundMapEntry>,
}

#[derive(Clone, PartialEq, Message)]
struct BamlValueClass {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(message, repeated, tag = "2")]
    fields: Vec<BamlOutboundMapEntry>,
    #[prost(message, repeated, tag = "3")]
    type_args: Vec<BamlTy>,
}

#[derive(Clone, PartialEq, Message)]
struct BamlValueEnum {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(string, tag = "2")]
    value: String,
    #[prost(bool, tag = "3")]
    is_dynamic: bool,
}

#[derive(Clone, PartialEq, Message)]
struct BamlValueMedia {
    #[prost(enumeration = "MediaTypeEnum", tag = "1")]
    media: i32,
    #[prost(string, optional, tag = "2")]
    mime_type: Option<String>,
    #[prost(oneof = "BamlValueMediaValue", tags = "3, 4, 5")]
    value: Option<BamlValueMediaValue>,
}

#[derive(Clone, PartialEq, ::prost::Oneof)]
enum BamlValueMediaValue {
    #[prost(string, tag = "3")]
    Url(String),
    #[prost(string, tag = "4")]
    Base64(String),
    #[prost(string, tag = "5")]
    File(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
enum MediaTypeEnum {
    Unspecified = 0,
    Image = 1,
    Audio = 2,
    Pdf = 3,
    Video = 4,
    Other = 5,
}

#[derive(Clone, PartialEq, Message)]
struct BamlTy {
    #[prost(
        oneof = "BamlTyVariant",
        tags = "1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24"
    )]
    ty: Option<BamlTyVariant>,
}

#[derive(Clone, PartialEq, ::prost::Oneof)]
enum BamlTyVariant {
    #[prost(message, tag = "1")]
    Primitive(BamlTyPrimitive),
    #[prost(message, tag = "2")]
    ClassTy(BamlTyClass),
    #[prost(message, tag = "3")]
    Enum(BamlTyEnum),
    #[prost(message, tag = "4")]
    List(BamlTyList),
    #[prost(message, tag = "5")]
    Map(BamlTyMap),
    #[prost(message, tag = "6")]
    Optional(BamlTyOptional),
    #[prost(message, tag = "7")]
    Union(BamlTyUnion),
    #[prost(message, tag = "8")]
    Literal(BamlTyLiteral),
    #[prost(message, tag = "9")]
    TypeAlias(BamlTyTypeAlias),
    #[prost(message, tag = "10")]
    Unknown(BamlTyUnknown),
    #[prost(message, tag = "11")]
    Media(BamlTyMedia),
    #[prost(message, tag = "12")]
    Interface(BamlTyInterface),
    #[prost(message, tag = "13")]
    EnumVariant(BamlTyEnumVariant),
    #[prost(message, tag = "14")]
    Function(BamlTyFunction),
    #[prost(message, tag = "15")]
    Future(BamlTyFuture),
    #[prost(message, tag = "16")]
    RustType(BamlTyRustType),
    #[prost(message, tag = "17")]
    MetaType(BamlTyMetaType),
    #[prost(message, tag = "18")]
    Resource(BamlTyResource),
    #[prost(message, tag = "19")]
    PromptAst(BamlTyPromptAst),
    #[prost(message, tag = "20")]
    Void(BamlTyVoid),
    #[prost(message, tag = "22")]
    TypeVar(BamlTyTypeVar),
    #[prost(message, tag = "23")]
    AssociatedTypeProjection(BamlTyAssociatedTypeProjection),
    #[prost(message, tag = "24")]
    Never(BamlTyNever),
}

#[derive(Clone, PartialEq, Message)]
struct BamlTyPrimitive {
    #[prost(enumeration = "BamlTyPrimitiveKind", tag = "1")]
    kind: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
enum BamlTyPrimitiveKind {
    Unspecified = 0,
    String = 1,
    Int = 2,
    Float = 3,
    Bool = 4,
    Null = 5,
    Bytes = 6,
    Bigint = 7,
}

#[derive(Clone, PartialEq, Message)]
struct BamlTyClass {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(message, repeated, tag = "2")]
    type_args: Vec<BamlTy>,
}

#[derive(Clone, PartialEq, Message)]
struct BamlTyTypeAlias {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(message, repeated, tag = "2")]
    type_args: Vec<BamlTy>,
}

#[derive(Clone, PartialEq, Message)]
struct BamlTyEnum {
    #[prost(string, tag = "1")]
    name: String,
}

#[derive(Clone, PartialEq, Message)]
struct BamlTyList {
    #[prost(message, optional, boxed, tag = "1")]
    item: Option<Box<BamlTy>>,
}

#[derive(Clone, PartialEq, Message)]
struct BamlTyMap {
    #[prost(message, optional, boxed, tag = "1")]
    key: Option<Box<BamlTy>>,
    #[prost(message, optional, boxed, tag = "2")]
    value: Option<Box<BamlTy>>,
}

#[derive(Clone, PartialEq, Message)]
struct BamlTyOptional {
    #[prost(message, optional, boxed, tag = "1")]
    inner: Option<Box<BamlTy>>,
}

#[derive(Clone, PartialEq, Message)]
struct BamlTyUnion {
    #[prost(message, repeated, tag = "1")]
    options: Vec<BamlTy>,
}

#[derive(Clone, PartialEq, Message)]
#[allow(
    clippy::empty_structs_with_brackets,
    reason = "Empty message mirrors the outbound protobuf shape."
)]
struct BamlTyUnknown {}

#[derive(Clone, PartialEq, Message)]
struct BamlTyMedia {
    #[prost(enumeration = "BamlTyMediaKind", tag = "1")]
    kind: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
enum BamlTyMediaKind {
    Unspecified = 0,
    Image = 1,
    Audio = 2,
    Video = 3,
    Pdf = 4,
    Generic = 5,
}

#[derive(Clone, PartialEq, Message)]
struct BamlTyInterface {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(message, repeated, tag = "2")]
    type_args: Vec<BamlTy>,
    #[prost(message, repeated, tag = "3")]
    bindings: Vec<BamlTyAssociatedBinding>,
}

#[derive(Clone, PartialEq, Message)]
struct BamlTyAssociatedBinding {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(message, optional, tag = "2")]
    ty: Option<BamlTy>,
}

#[derive(Clone, PartialEq, Message)]
struct BamlTyEnumVariant {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(string, tag = "2")]
    variant: String,
}

#[derive(Clone, PartialEq, Message)]
struct BamlTyFunction {
    #[prost(string, repeated, tag = "1")]
    generic_params: Vec<String>,
    #[prost(message, repeated, tag = "3")]
    params: Vec<BamlTyFunctionParam>,
    #[prost(message, optional, boxed, tag = "4")]
    ret: Option<Box<BamlTy>>,
    #[prost(message, optional, boxed, tag = "5")]
    throws: Option<Box<BamlTy>>,
}

#[derive(Clone, PartialEq, Message)]
struct BamlTyFunctionParam {
    #[prost(string, optional, tag = "1")]
    name: Option<String>,
    #[prost(message, optional, tag = "2")]
    ty: Option<BamlTy>,
    #[prost(enumeration = "BamlTyFunctionParamMode", tag = "3")]
    mode: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
enum BamlTyFunctionParamMode {
    Unspecified = 0,
    Required = 1,
    Optional = 2,
}

#[derive(Clone, PartialEq, Message)]
struct BamlTyFuture {
    #[prost(message, optional, boxed, tag = "1")]
    value: Option<Box<BamlTy>>,
    #[prost(message, optional, boxed, tag = "2")]
    error: Option<Box<BamlTy>>,
}

#[derive(Clone, PartialEq, Message)]
struct BamlTyTypeVar {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(uint32, tag = "2")]
    index: u32,
}

#[derive(Clone, PartialEq, Message)]
struct BamlTyAssociatedTypeProjection {
    #[prost(message, optional, boxed, tag = "1")]
    base: Option<Box<BamlTy>>,
    #[prost(message, optional, boxed, tag = "2")]
    interface: Option<Box<BamlTy>>,
    #[prost(string, tag = "3")]
    member: String,
}

#[derive(Clone, PartialEq, Message)]
struct BamlTyLiteral {
    #[prost(oneof = "BamlTyLiteralVariant", tags = "1, 2, 3, 4, 5")]
    literal: Option<BamlTyLiteralVariant>,
}

#[derive(Clone, PartialEq, ::prost::Oneof)]
#[allow(
    clippy::enum_variant_names,
    reason = "Variant names mirror the protobuf literal oneof fields."
)]
enum BamlTyLiteralVariant {
    #[prost(string, tag = "1")]
    StringValue(String),
    #[prost(int64, tag = "2")]
    IntValue(i64),
    #[prost(bool, tag = "3")]
    BoolValue(bool),
    #[prost(string, tag = "4")]
    BigintValue(String),
    #[prost(string, tag = "5")]
    FloatValue(String),
}

#[derive(Clone, PartialEq, Message)]
#[allow(
    clippy::empty_structs_with_brackets,
    reason = "Empty message mirrors the outbound protobuf shape."
)]
struct BamlTyRustType {}

#[derive(Clone, PartialEq, Message)]
#[allow(
    clippy::empty_structs_with_brackets,
    reason = "Empty message mirrors the outbound protobuf shape."
)]
struct BamlTyMetaType {}

#[derive(Clone, PartialEq, Message)]
#[allow(
    clippy::empty_structs_with_brackets,
    reason = "Empty message mirrors the outbound protobuf shape."
)]
struct BamlTyResource {}

#[derive(Clone, PartialEq, Message)]
#[allow(
    clippy::empty_structs_with_brackets,
    reason = "Empty message mirrors the outbound protobuf shape."
)]
struct BamlTyPromptAst {}

#[derive(Clone, PartialEq, Message)]
#[allow(
    clippy::empty_structs_with_brackets,
    reason = "Empty message mirrors the outbound protobuf shape."
)]
struct BamlTyVoid {}

#[derive(Clone, PartialEq, Message)]
#[allow(
    clippy::empty_structs_with_brackets,
    reason = "Empty message mirrors the outbound protobuf shape."
)]
struct BamlTyNever {}

pub(crate) fn encode_trace_snapshot_body(snapshot: &TraceSnapshot) -> Result<Vec<u8>, String> {
    let value = encode_value(snapshot, snapshot.root())?;
    Ok(value.encode_to_vec())
}

fn encode_value(
    snapshot: &TraceSnapshot,
    value_ref: TraceValueRef,
) -> Result<BamlOutboundValue, String> {
    let value = snapshot
        .value(value_ref)
        .ok_or_else(|| format!("trace value ref {} is missing", value_ref.raw()))?;
    let variant = match value {
        TraceValue::Null => Some(BamlValueVariant::NullValue(BamlValueNull {})),
        TraceValue::Bool(value) => Some(BamlValueVariant::BoolValue(*value)),
        TraceValue::Int(value) => Some(BamlValueVariant::IntValue(*value)),
        TraceValue::Float(value) => Some(BamlValueVariant::FloatValue(*value)),
        TraceValue::Bigint(value) => Some(BamlValueVariant::BigintValue(bigint_to_hex(value))),
        TraceValue::String(value) => Some(BamlValueVariant::StringValue(value.clone())),
        TraceValue::Bytes(value) => Some(BamlValueVariant::Uint8arrayValue(value.clone())),
        TraceValue::Array(items) => Some(BamlValueVariant::ListValue(BamlValueList {
            items: items
                .iter()
                .copied()
                .map(|item| encode_value(snapshot, item))
                .collect::<Result<_, _>>()?,
        })),
        TraceValue::Map(entries) => Some(BamlValueVariant::MapValue(BamlValueMap {
            entries: entries
                .iter()
                .map(|(key, value)| {
                    Ok(BamlOutboundMapEntry {
                        key: key.clone(),
                        value: Some(encode_value(snapshot, *value)?),
                    })
                })
                .collect::<Result<_, String>>()?,
        })),
        TraceValue::Media(media) => Some(BamlValueVariant::MediaValue(media_to_proto(media))),
        TraceValue::Instance {
            type_name,
            type_args,
            fields,
        } => Some(BamlValueVariant::ClassValue(BamlValueClass {
            name: type_name.clone(),
            fields: fields
                .iter()
                .map(|(key, value)| {
                    Ok(BamlOutboundMapEntry {
                        key: key.clone(),
                        value: Some(encode_value(snapshot, *value)?),
                    })
                })
                .collect::<Result<_, String>>()?,
            type_args: type_args.iter().map(runtime_ty_to_proto_ty).collect(),
        })),
        TraceValue::Enum { type_name, variant } => {
            Some(BamlValueVariant::EnumValue(BamlValueEnum {
                name: type_name.clone(),
                value: variant.clone(),
                is_dynamic: false,
            }))
        }
        TraceValue::Omitted(descriptor) => {
            Some(BamlValueVariant::ClassValue(omission_to_class(descriptor)))
        }
    };
    Ok(BamlOutboundValue { value: variant })
}

fn media_to_proto(media: &TraceMediaValue) -> BamlValueMedia {
    BamlValueMedia {
        media: media_kind_to_proto_enum(media.kind) as i32,
        mime_type: media.mime_type.clone(),
        value: Some(match &media.content {
            TraceMediaContent::Url(url) => BamlValueMediaValue::Url(url.clone()),
            TraceMediaContent::Base64(base64) => BamlValueMediaValue::Base64(base64.clone()),
            TraceMediaContent::File(file) => BamlValueMediaValue::File(file.clone()),
        }),
    }
}

fn media_kind_to_proto_enum(kind: bex_external_types::MediaKind) -> MediaTypeEnum {
    match kind {
        bex_external_types::MediaKind::Image => MediaTypeEnum::Image,
        bex_external_types::MediaKind::Audio => MediaTypeEnum::Audio,
        bex_external_types::MediaKind::Pdf => MediaTypeEnum::Pdf,
        bex_external_types::MediaKind::Video => MediaTypeEnum::Video,
        bex_external_types::MediaKind::Generic => MediaTypeEnum::Other,
    }
}

fn omission_to_class(descriptor: &TraceOmissionDescriptor) -> BamlValueClass {
    BamlValueClass {
        name: "baml.trace.OmittedValue".to_string(),
        type_args: Vec::new(),
        fields: vec![
            BamlOutboundMapEntry {
                key: "reason".to_string(),
                value: Some(BamlOutboundValue {
                    value: Some(BamlValueVariant::StringValue(
                        omission_reason_wire(descriptor.reason).to_string(),
                    )),
                }),
            },
            BamlOutboundMapEntry {
                key: "message".to_string(),
                value: Some(BamlOutboundValue {
                    value: Some(BamlValueVariant::StringValue(descriptor.message.clone())),
                }),
            },
        ],
    }
}

fn runtime_ty_to_proto_ty(ty: &RuntimeTy) -> BamlTy {
    BamlTy {
        ty: Some(runtime_ty_to_variant(ty)),
    }
}

fn primitive(kind: BamlTyPrimitiveKind) -> BamlTyVariant {
    BamlTyVariant::Primitive(BamlTyPrimitive { kind: kind as i32 })
}

fn interface_to_proto_ty(
    name: &baml_type::TypeName,
    type_args: &[RuntimeTy],
    bindings: &[(baml_type::Name, RuntimeTy)],
) -> BamlTy {
    BamlTy {
        ty: Some(BamlTyVariant::Interface(BamlTyInterface {
            name: name.render_dotted(false),
            type_args: type_args.iter().map(runtime_ty_to_proto_ty).collect(),
            bindings: bindings
                .iter()
                .map(|(name, ty)| BamlTyAssociatedBinding {
                    name: name.as_str().to_string(),
                    ty: Some(runtime_ty_to_proto_ty(ty)),
                })
                .collect(),
        })),
    }
}

fn runtime_ty_to_variant(ty: &RuntimeTy) -> BamlTyVariant {
    match ty {
        RuntimeTy::String { .. } => primitive(BamlTyPrimitiveKind::String),
        RuntimeTy::Int { .. } => primitive(BamlTyPrimitiveKind::Int),
        RuntimeTy::Float { .. } => primitive(BamlTyPrimitiveKind::Float),
        RuntimeTy::Bool { .. } => primitive(BamlTyPrimitiveKind::Bool),
        RuntimeTy::Null { .. } => primitive(BamlTyPrimitiveKind::Null),
        RuntimeTy::Uint8Array { .. } => primitive(BamlTyPrimitiveKind::Bytes),
        RuntimeTy::Bigint { .. } => primitive(BamlTyPrimitiveKind::Bigint),
        RuntimeTy::Class(name, args, _) => BamlTyVariant::ClassTy(BamlTyClass {
            name: name.render_dotted(false),
            type_args: args.iter().map(runtime_ty_to_proto_ty).collect(),
        }),
        RuntimeTy::TypeAlias(name, _) => BamlTyVariant::TypeAlias(BamlTyTypeAlias {
            name: name.render_dotted(false),
            type_args: Vec::new(),
        }),
        RuntimeTy::Enum(name, _) => BamlTyVariant::Enum(BamlTyEnum {
            name: name.render_dotted(false),
        }),
        RuntimeTy::EnumVariant(name, variant, _) => BamlTyVariant::EnumVariant(BamlTyEnumVariant {
            name: name.render_dotted(false),
            variant: variant.as_str().to_string(),
        }),
        RuntimeTy::List(inner, _) => BamlTyVariant::List(BamlTyList {
            item: Some(Box::new(runtime_ty_to_proto_ty(inner))),
        }),
        RuntimeTy::Map { key, value, .. } => BamlTyVariant::Map(BamlTyMap {
            key: Some(Box::new(runtime_ty_to_proto_ty(key))),
            value: Some(Box::new(runtime_ty_to_proto_ty(value))),
        }),
        RuntimeTy::Union(members, _) => {
            let has_null = members.iter().any(RuntimeTy::is_null);
            let non_null = members
                .iter()
                .filter(|member| !member.is_null())
                .collect::<Vec<_>>();
            if has_null && non_null.len() == 1 {
                BamlTyVariant::Optional(BamlTyOptional {
                    inner: Some(Box::new(runtime_ty_to_proto_ty(non_null[0]))),
                })
            } else {
                BamlTyVariant::Union(BamlTyUnion {
                    options: members.iter().map(runtime_ty_to_proto_ty).collect(),
                })
            }
        }
        RuntimeTy::Literal(lit, _, _) => BamlTyVariant::Literal(literal_to_proto(lit)),
        RuntimeTy::Media(kind, _) => BamlTyVariant::Media(BamlTyMedia {
            kind: media_kind_to_proto_ty(*kind) as i32,
        }),
        RuntimeTy::Interface(name, args, bindings, _) => {
            interface_to_proto_ty(name, args, bindings)
                .ty
                .unwrap_or_else(|| unreachable!("interface helper always sets ty"))
        }
        RuntimeTy::Function {
            params,
            ret,
            throws,
            ..
        } => BamlTyVariant::Function(BamlTyFunction {
            generic_params: Vec::new(),
            params: params
                .iter()
                .map(|param| BamlTyFunctionParam {
                    name: param.name.as_ref().map(|name| name.as_str().to_string()),
                    ty: Some(runtime_ty_to_proto_ty(&param.ty)),
                    mode: function_param_mode_to_proto(param.mode) as i32,
                })
                .collect(),
            ret: Some(Box::new(runtime_ty_to_proto_ty(ret))),
            throws: Some(Box::new(runtime_ty_to_proto_ty(throws))),
        }),
        RuntimeTy::Future(value, error, _) => BamlTyVariant::Future(BamlTyFuture {
            value: Some(Box::new(runtime_ty_to_proto_ty(value))),
            error: Some(Box::new(runtime_ty_to_proto_ty(error))),
        }),
        RuntimeTy::RustType { .. } => BamlTyVariant::RustType(BamlTyRustType {}),
        RuntimeTy::Type { .. } => BamlTyVariant::MetaType(BamlTyMetaType {}),
        RuntimeTy::Resource { .. } => BamlTyVariant::Resource(BamlTyResource {}),
        RuntimeTy::PromptAst { .. } => BamlTyVariant::PromptAst(BamlTyPromptAst {}),
        RuntimeTy::Void { .. } => BamlTyVariant::Void(BamlTyVoid {}),
        RuntimeTy::TypeVar(param, _) => BamlTyVariant::TypeVar(BamlTyTypeVar {
            name: param.as_str().to_string(),
            index: param.index(),
        }),
        RuntimeTy::AssociatedTypeProjection {
            base,
            interface,
            member,
            ..
        } => BamlTyVariant::AssociatedTypeProjection(BamlTyAssociatedTypeProjection {
            base: Some(Box::new(runtime_ty_to_proto_ty(base))),
            // Always present on the Rust side; the wire field stays optional.
            interface: Some(Box::new(interface_to_proto_ty(
                &interface.name,
                &interface.generics,
                &interface.associated_types,
            ))),
            member: member.as_str().to_string(),
        }),
        RuntimeTy::BuiltinUnknown { .. } => BamlTyVariant::Unknown(BamlTyUnknown {}),
        RuntimeTy::Never { .. } => BamlTyVariant::Never(BamlTyNever {}),
    }
}

fn literal_to_proto(lit: &Literal) -> BamlTyLiteral {
    BamlTyLiteral {
        literal: Some(match lit {
            Literal::String(value) => BamlTyLiteralVariant::StringValue(value.clone()),
            Literal::Int(value) => BamlTyLiteralVariant::IntValue(*value),
            Literal::Bool(value) => BamlTyLiteralVariant::BoolValue(*value),
            Literal::Bigint(value) => BamlTyLiteralVariant::BigintValue(value.to_string()),
            Literal::Float(value) => BamlTyLiteralVariant::FloatValue(value.clone()),
        }),
    }
}

fn media_kind_to_proto_ty(kind: MediaKind) -> BamlTyMediaKind {
    match kind {
        MediaKind::Image => BamlTyMediaKind::Image,
        MediaKind::Audio => BamlTyMediaKind::Audio,
        MediaKind::Video => BamlTyMediaKind::Video,
        MediaKind::Pdf => BamlTyMediaKind::Pdf,
        MediaKind::Generic => BamlTyMediaKind::Generic,
    }
}

fn function_param_mode_to_proto(mode: FunctionParamMode) -> BamlTyFunctionParamMode {
    match mode {
        FunctionParamMode::Required => BamlTyFunctionParamMode::Required,
        FunctionParamMode::Optional => BamlTyFunctionParamMode::Optional,
    }
}

fn omission_reason_wire(reason: TraceOmissionReason) -> &'static str {
    match reason {
        TraceOmissionReason::OmittedArgument => "omittedArgument",
        TraceOmissionReason::UnsupportedValue => "unsupportedValue",
        TraceOmissionReason::HostOwnedValue => "hostOwnedValue",
        TraceOmissionReason::InvalidRuntimeValue => "invalidRuntimeValue",
        TraceOmissionReason::CyclicReference => "cyclicReference",
    }
}

fn bigint_to_hex(value: &str) -> String {
    value
        .parse::<BigInt>()
        .map(|value| format!("{value:x}"))
        .unwrap_or_else(|_| value.to_string())
}

/// Render a captured trace value for a human-facing log consumer.
///
/// The trace artifact itself remains an encoded [`BamlOutboundValue`]. This
/// renderer is only used at the display boundary (for example, `baml test
/// --log`) and mirrors BAML's default structural `to_string()` shape:
/// top-level strings are bare, nested strings are quoted, and containers and
/// class instances recurse without exposing protobuf or Rust wrapper types.
pub(crate) fn render_encoded_trace_value(body: &[u8]) -> Result<String, String> {
    let value = BamlOutboundValue::decode(body)
        .map_err(|err| format!("failed to decode captured BAML log body: {err}"))?;
    Ok(render_trace_value(&value, false))
}

fn render_trace_value(value: &BamlOutboundValue, nested: bool) -> String {
    match value.value.as_ref() {
        None | Some(BamlValueVariant::NullValue(_)) => "null".to_string(),
        Some(BamlValueVariant::StringValue(value)) => {
            if nested {
                format!("{value:?}")
            } else {
                value.clone()
            }
        }
        Some(BamlValueVariant::IntValue(value)) => value.to_string(),
        Some(BamlValueVariant::FloatValue(value)) => bex_vm_types::format_float(*value),
        Some(BamlValueVariant::BoolValue(value)) => value.to_string(),
        Some(BamlValueVariant::BigintValue(value)) => {
            // Bigints use base-16 on the bridge wire, while BAML's string
            // representation is decimal.
            BigInt::parse_bytes(value.as_bytes(), 16)
                .map_or_else(|| value.clone(), |value| value.to_string())
        }
        Some(BamlValueVariant::ListValue(list)) => {
            let items = list
                .items
                .iter()
                .map(|item| render_trace_value(item, true))
                .collect::<Vec<_>>();
            format!("[{}]", items.join(", "))
        }
        Some(BamlValueVariant::MapValue(map)) => {
            let entries = map
                .entries
                .iter()
                .map(|entry| {
                    let value = entry.value.as_ref().map_or_else(
                        || "null".to_string(),
                        |value| render_trace_value(value, true),
                    );
                    format!("{:?}: {value}", entry.key)
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", entries.join(", "))
        }
        Some(BamlValueVariant::ClassValue(class)) => {
            let class_name = class.name.rsplit('.').next().unwrap_or(class.name.as_str());
            if class.fields.is_empty() {
                return class_name.to_string();
            }
            let fields = class
                .fields
                .iter()
                .map(|field| {
                    let value = field.value.as_ref().map_or_else(
                        || "null".to_string(),
                        |value| render_trace_value(value, true),
                    );
                    format!("{}: {value}", field.key)
                })
                .collect::<Vec<_>>();
            format!("{class_name} {{ {} }}", fields.join(", "))
        }
        Some(BamlValueVariant::EnumValue(value)) => value.value.clone(),
        Some(BamlValueVariant::Uint8arrayValue(bytes)) => {
            let bytes = bytes.iter().map(u8::to_string).collect::<Vec<_>>();
            format!("[{}]", bytes.join(", "))
        }
        Some(BamlValueVariant::MediaValue(media)) => render_media_value(media),
    }
}

fn render_media_value(media: &BamlValueMedia) -> String {
    let kind = MediaTypeEnum::try_from(media.media).map_or("media", |kind| match kind {
        MediaTypeEnum::Image => "image",
        MediaTypeEnum::Audio => "audio",
        MediaTypeEnum::Pdf => "pdf",
        MediaTypeEnum::Video => "video",
        MediaTypeEnum::Unspecified | MediaTypeEnum::Other => "media",
    });
    let mime = media
        .mime_type
        .as_ref()
        .map(|mime| format!(", mime_type={mime:?}"))
        .unwrap_or_default();
    match media.value.as_ref() {
        Some(BamlValueMediaValue::Url(value)) => {
            format!("{kind}::url({value:?}{mime})")
        }
        Some(BamlValueMediaValue::File(value)) => {
            format!("{kind}::file({value:?}{mime})")
        }
        Some(BamlValueMediaValue::Base64(value)) => {
            let preview = if value.len() <= 10 {
                value.clone()
            } else {
                format!(
                    "{}...{}",
                    &value[..5],
                    &value[value.len().saturating_sub(5)..]
                )
            };
            format!(
                "{kind}::base64({preview:?}, len={}{}{})",
                value.len(),
                if mime.is_empty() { "" } else { ", " },
                mime.trim_start_matches(", ")
            )
        }
        None => format!("{kind}::missing"),
    }
}

#[cfg(test)]
mod tests {
    use bridge_ctypes::baml_bridge::cffi::{
        BamlOutboundValue, BamlTyPrimitiveKind, baml_outbound_value::Value as BamlValueVariant,
        baml_ty,
    };
    use prost::Message;

    use crate::trace_heap::{
        TraceOmissionDescriptor, TraceOmissionReason, TraceSnapshot, TraceValue, TraceValueRef,
    };

    #[test]
    fn trace_snapshot_encodes_as_bare_baml_outbound_value() {
        let snapshot = TraceSnapshot::for_test(
            TraceValueRef::for_test(2),
            vec![
                TraceValue::String("world".to_string()),
                TraceValue::Int(7),
                TraceValue::Map(vec![
                    ("hello".to_string(), TraceValueRef::for_test(0)),
                    ("count".to_string(), TraceValueRef::for_test(1)),
                ]),
            ],
        );

        let bytes = super::encode_trace_snapshot_body(&snapshot).unwrap();
        let decoded = BamlOutboundValue::decode(bytes.as_slice()).unwrap();
        let Some(BamlValueVariant::MapValue(map)) = decoded.value else {
            panic!("root should encode as a map");
        };
        assert_eq!(map.entries.len(), 2);
    }

    #[test]
    fn captured_values_render_in_baml_structural_string_shape() {
        let snapshot = TraceSnapshot::for_test(
            TraceValueRef::for_test(7),
            vec![
                TraceValue::String("ada".to_string()),
                TraceValue::Int(1),
                TraceValue::Float(2.0),
                TraceValue::Array(vec![TraceValueRef::for_test(1), TraceValueRef::for_test(2)]),
                TraceValue::Instance {
                    type_name: "user.Person".to_string(),
                    type_args: Vec::new(),
                    fields: vec![
                        ("name".to_string(), TraceValueRef::for_test(0)),
                        ("scores".to_string(), TraceValueRef::for_test(3)),
                    ],
                },
                TraceValue::Bool(true),
                TraceValue::Bigint("42".to_string()),
                TraceValue::Map(vec![
                    ("event".to_string(), TraceValueRef::for_test(4)),
                    ("ok".to_string(), TraceValueRef::for_test(5)),
                    ("big".to_string(), TraceValueRef::for_test(6)),
                ]),
            ],
        );

        let bytes = super::encode_trace_snapshot_body(&snapshot).unwrap();
        assert_eq!(
            super::render_encoded_trace_value(&bytes).unwrap(),
            r#"{"event": Person { name: "ada", scores: [1, 2.0] }, "ok": true, "big": 42}"#
        );
    }

    #[test]
    fn omitted_trace_values_encode_as_renderable_class_values() {
        let snapshot = TraceSnapshot::for_test(
            TraceValueRef::for_test(0),
            vec![TraceValue::Omitted(TraceOmissionDescriptor {
                reason: TraceOmissionReason::HostOwnedValue,
                message: "host-owned callable".to_string(),
            })],
        );

        let bytes = super::encode_trace_snapshot_body(&snapshot).unwrap();
        let decoded = BamlOutboundValue::decode(bytes.as_slice()).unwrap();
        let Some(BamlValueVariant::ClassValue(class)) = decoded.value else {
            panic!("omission should encode as a class");
        };
        assert_eq!(class.name, "baml.trace.OmittedValue");
        assert_eq!(class.fields[0].key, "reason");
    }

    #[test]
    fn media_trace_values_encode_as_media_values() {
        let snapshot = TraceSnapshot::for_test(
            TraceValueRef::for_test(0),
            vec![TraceValue::Media(crate::trace_heap::TraceMediaValue {
                kind: bex_external_types::MediaKind::Image,
                mime_type: Some("image/png".to_string()),
                content: crate::trace_heap::TraceMediaContent::Base64(
                    "aW1hZ2UtYnl0ZXM=".to_string(),
                ),
            })],
        );

        let bytes = super::encode_trace_snapshot_body(&snapshot).unwrap();
        let decoded = BamlOutboundValue::decode(bytes.as_slice()).unwrap();
        let Some(BamlValueVariant::MediaValue(media)) = decoded.value else {
            panic!("root should encode as media");
        };
        assert_eq!(media.mime_type.as_deref(), Some("image/png"));
        assert!(matches!(
            media.value,
            Some(bridge_ctypes::baml_bridge::cffi::baml_value_media::Value::Base64(_))
        ));
    }

    #[test]
    fn generic_instance_trace_values_preserve_type_args() {
        let snapshot = TraceSnapshot::for_test(
            TraceValueRef::for_test(0),
            vec![TraceValue::Instance {
                type_name: "user.Box".to_string(),
                type_args: vec![baml_type::RuntimeTy::string()],
                fields: Vec::new(),
            }],
        );

        let bytes = super::encode_trace_snapshot_body(&snapshot).unwrap();
        let decoded = BamlOutboundValue::decode(bytes.as_slice()).unwrap();
        let Some(BamlValueVariant::ClassValue(class)) = decoded.value else {
            panic!("root should encode as a class");
        };
        assert_eq!(class.name, "user.Box");
        assert_eq!(class.type_args.len(), 1);
        let Some(baml_ty::Ty::Primitive(primitive)) = class.type_args[0].ty.as_ref() else {
            panic!("type arg should encode as a primitive string type");
        };
        assert_eq!(
            primitive.kind,
            BamlTyPrimitiveKind::BamlTyPrimitiveString as i32
        );
    }
}
