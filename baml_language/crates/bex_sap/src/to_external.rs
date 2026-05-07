//! Conversion from SAP types and values to `BexExternalValue` and `baml_type::Ty`.
//!
//! This module provides the bridge between SAP's internal type/value representation
//! and the external value tree used by the engine to push results back onto the VM heap.

use baml_type::TypeName;
use bex_external_types::BexExternalValue;
use indexmap::IndexMap;

use crate::{
    baml_value::{BamlStreamState, BamlValue},
    deserializer::types::BamlValueWithFlags,
    sap_model::{
        ArrayTy, ClassTy, EnumTy, EnumVariantTy, MapTy, MediaTy, StreamStateTy, Ty, TyResolvedRef,
        TyWithMeta, TypeAnnotations, UnionTy,
    },
};

// ============================================================================
// Type conversion: SAP types → baml_type::Ty
// ============================================================================

/// Convert a SAP type back to a `baml_type::Ty`.
///
/// # Known simplifications (not round-trip safe)
///
/// - `Optional(T)` was converted to `Union([Null, T])` on the way in; it stays as `Union` here.
/// - `TypeAlias` was flattened; the alias name is lost.
/// - `EnumVariant` becomes `Enum` (variant specificity lost at the type level).
/// - `TyAttr` annotations are not reconstructed; `TyAttr::default()` is used throughout.
pub trait ToBamlTy {
    fn to_baml_ty(&self) -> baml_type::Ty;
}

impl ToBamlTy for TyResolvedRef<'_, TypeName> {
    fn to_baml_ty(&self) -> baml_type::Ty {
        let attr = baml_type::TyAttr::default();
        match self {
            TyResolvedRef::Int(_) => baml_type::Ty::Int { attr },
            TyResolvedRef::Float(_) => baml_type::Ty::Float { attr },
            TyResolvedRef::String(_) => baml_type::Ty::String { attr },
            TyResolvedRef::Bool(_) => baml_type::Ty::Bool { attr },
            TyResolvedRef::Null(_) => baml_type::Ty::Null { attr },
            TyResolvedRef::Media(media) => baml_type::Ty::Media(media.to_baml_media_kind(), attr),
            TyResolvedRef::LiteralInt(v) => {
                baml_type::Ty::Literal(baml_type::Literal::Int(v.0), attr)
            }
            TyResolvedRef::LiteralString(v) => {
                baml_type::Ty::Literal(baml_type::Literal::String(v.0.to_string()), attr)
            }
            TyResolvedRef::LiteralBool(v) => {
                baml_type::Ty::Literal(baml_type::Literal::Bool(v.0), attr)
            }
            TyResolvedRef::Array(a) => a.to_baml_ty(),
            TyResolvedRef::Map(m) => m.to_baml_ty(),
            TyResolvedRef::Class(c) => c.to_baml_ty(),
            TyResolvedRef::Enum(e) => e.to_baml_ty(),
            TyResolvedRef::EnumVariant(ev) => ev.to_baml_ty(),
            TyResolvedRef::Union(u) => u.to_baml_ty(),
            TyResolvedRef::StreamState(s) => s.to_baml_ty(),
        }
    }
}

impl ToBamlTy for Ty<'_, TypeName> {
    fn to_baml_ty(&self) -> baml_type::Ty {
        match self {
            Ty::Resolved(resolved) => resolved.as_ref().to_baml_ty(),
            Ty::ResolvedRef(resolved_ref) => resolved_ref.to_baml_ty(),
            Ty::Unresolved(_name) => {
                // Unresolved names are class/enum references; we can't distinguish
                // without access to the TypeRefDb. Use unknown rather than guessing.
                baml_type::Ty::unknown()
            }
        }
    }
}

impl ToBamlTy for ArrayTy<'_, TypeName> {
    fn to_baml_ty(&self) -> baml_type::Ty {
        let inner = self.ty.ty.to_baml_ty();
        baml_type::Ty::List(Box::new(inner), baml_type::TyAttr::default())
    }
}

impl ToBamlTy for MapTy<'_, TypeName> {
    fn to_baml_ty(&self) -> baml_type::Ty {
        baml_type::Ty::Map {
            key: Box::new(self.key.ty.to_baml_ty()),
            value: Box::new(self.value.ty.to_baml_ty()),
            attr: baml_type::TyAttr::default(),
        }
    }
}

impl ToBamlTy for ClassTy<'_, TypeName> {
    fn to_baml_ty(&self) -> baml_type::Ty {
        baml_type::Ty::Class(self.name.clone(), Vec::new(), baml_type::TyAttr::default())
    }
}

impl ToBamlTy for EnumTy<'_, TypeName> {
    fn to_baml_ty(&self) -> baml_type::Ty {
        baml_type::Ty::Enum(self.name.clone(), baml_type::TyAttr::default())
    }
}

impl ToBamlTy for EnumVariantTy<'_, TypeName> {
    /// Loses variant specificity — maps back to the parent enum type.
    fn to_baml_ty(&self) -> baml_type::Ty {
        baml_type::Ty::Enum(self.name.clone(), baml_type::TyAttr::default())
    }
}

impl ToBamlTy for UnionTy<'_, TypeName> {
    fn to_baml_ty(&self) -> baml_type::Ty {
        let members: Vec<baml_type::Ty> = self.variants.iter().map(|v| v.ty.to_baml_ty()).collect();
        baml_type::Ty::Union(members, baml_type::TyAttr::default())
    }
}

impl ToBamlTy for StreamStateTy<'_, TypeName> {
    /// `StreamState` is a value-level concept; at the type level we return the inner type.
    fn to_baml_ty(&self) -> baml_type::Ty {
        self.value.ty.to_baml_ty()
    }
}

impl MediaTy {
    fn to_baml_media_kind(self) -> baml_type::MediaKind {
        match self {
            MediaTy::Image => baml_type::MediaKind::Image,
            MediaTy::Audio => baml_type::MediaKind::Audio,
            MediaTy::Pdf => baml_type::MediaKind::Pdf,
            MediaTy::Video => baml_type::MediaKind::Video,
        }
    }
}

// ============================================================================
// Value conversion: SAP BamlValueWithFlags → BexExternalValue
// ============================================================================

/// Convert a SAP `BamlValueWithFlags` to a `BexExternalValue`, using the
/// type metadata from the deserializer to populate type information on
/// composite values (arrays, maps).
///
/// Deserializer metadata (flags, scores) is discarded.
pub fn baml_value_to_external(
    value: &BamlValueWithFlags<'_, '_, '_, TypeName>,
) -> BexExternalValue {
    baml_value_inner_to_external(&value.value, &value.meta.ty)
}

fn baml_value_inner_to_external(
    value: &BamlValue<'_, '_, '_, TypeName>,
    ty: &TyWithMeta<TyResolvedRef<'_, TypeName>, &TypeAnnotations<'_, TypeName>>,
) -> BexExternalValue {
    match value {
        BamlValue::String(s) => BexExternalValue::String(s.value.to_string()),
        BamlValue::Int(i) => BexExternalValue::Int(i.value),
        BamlValue::Float(f) => BexExternalValue::Float(f.value),
        BamlValue::Bool(b) => BexExternalValue::Bool(b.value),
        BamlValue::Null(_) => BexExternalValue::Null,
        BamlValue::Media(_) => {
            unimplemented!("Media value conversion to BexExternalValue is not yet implemented")
        }
        BamlValue::Array(arr) => {
            let element_type = match ty.ty {
                TyResolvedRef::Array(a) => a.ty.ty.to_baml_ty(),
                _ => baml_type::Ty::unknown(),
            };
            let items: Vec<BexExternalValue> =
                arr.value.iter().map(baml_value_to_external).collect();
            BexExternalValue::Array {
                element_type,
                items,
            }
        }
        BamlValue::Map(map) => {
            let (key_type, value_type) = match ty.ty {
                TyResolvedRef::Map(m) => (m.key.ty.to_baml_ty(), m.value.ty.to_baml_ty()),
                _ => (baml_type::Ty::string(), baml_type::Ty::unknown()),
            };
            let entries: IndexMap<String, BexExternalValue> = map
                .value
                .iter()
                .map(|(k, v)| (k.to_string(), baml_value_to_external(v)))
                .collect();
            BexExternalValue::Map {
                key_type,
                value_type,
                entries,
            }
        }
        BamlValue::Enum(e) => BexExternalValue::Variant {
            enum_name: e.name.to_string(),
            variant_name: e.value.to_string(),
        },
        BamlValue::Class(c) => {
            let fields: IndexMap<String, BexExternalValue> = c
                .value
                .iter()
                .map(|(k, v)| (k.to_string(), baml_value_to_external(v)))
                .collect();
            BexExternalValue::Instance {
                class_name: c.name.to_string(),
                fields,
            }
        }
        BamlValue::StreamState(state) => stream_state_to_external(state),
    }
}

fn stream_state_to_external(state: &BamlStreamState<'_, '_, '_, TypeName>) -> BexExternalValue {
    let (inner, state_name) = match state {
        BamlStreamState::Pending(v) => (v, "Pending"),
        BamlStreamState::Incomplete(v) => (v, "Incomplete"),
        BamlStreamState::Complete(v) => (v, "Complete"),
    };
    let mut fields = IndexMap::new();
    fields.insert("value".to_string(), baml_value_to_external(inner));
    fields.insert(
        "state".to_string(),
        BexExternalValue::String(state_name.to_string()),
    );
    BexExternalValue::Instance {
        class_name: "StreamState".to_string(),
        fields,
    }
}
