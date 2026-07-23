//! Conversion from SAP types and values to `BexExternalValue` and `baml_type::RuntimeTy`.
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
        TyWithMeta, TypeAnnotations, TypeRefDb, UnionTy,
    },
};

// ============================================================================
// Type conversion: SAP types → baml_type::RuntimeTy
// ============================================================================

/// Convert a SAP type back to a `baml_type::RuntimeTy`.
///
/// # Known simplifications (not round-trip safe)
///
/// - `Optional(T)` was converted to `Union([Null, T])` on the way in; it stays as `Union` here.
/// - Aliases already resolved into the target type have no nominal name to recover; named alias
///   references inside containers are preserved.
/// - `EnumVariant` becomes `Enum` (variant specificity lost at the type level).
/// - `TyAttr` annotations are not reconstructed; `TyAttr::default()` is used throughout.
pub trait ToBamlTy {
    fn to_baml_ty(&self, db: &TypeRefDb<'_, TypeName>) -> baml_type::RuntimeTy;
}

impl ToBamlTy for TyResolvedRef<'_, TypeName> {
    fn to_baml_ty(&self, db: &TypeRefDb<'_, TypeName>) -> baml_type::RuntimeTy {
        let attr = baml_type::TyAttr::default();
        match self {
            TyResolvedRef::Int(_) => baml_type::RuntimeTy::Int { attr },
            TyResolvedRef::Bigint(_) => baml_type::RuntimeTy::Bigint { attr },
            TyResolvedRef::Float(_) => baml_type::RuntimeTy::Float { attr },
            TyResolvedRef::String(_) => baml_type::RuntimeTy::String { attr },
            TyResolvedRef::Bool(_) => baml_type::RuntimeTy::Bool { attr },
            TyResolvedRef::Null(_) => baml_type::RuntimeTy::Null { attr },
            TyResolvedRef::Media(media) => {
                baml_type::RuntimeTy::Media(media.to_baml_media_kind(), attr)
            }
            TyResolvedRef::LiteralInt(v) => baml_type::RuntimeTy::Literal(
                baml_type::Literal::Int(v.0),
                baml_type::Freshness::Regular,
                attr,
            ),
            TyResolvedRef::LiteralBigint(v) => baml_type::RuntimeTy::Literal(
                baml_type::Literal::Bigint(v.0.clone()),
                baml_type::Freshness::Regular,
                attr,
            ),
            TyResolvedRef::LiteralString(v) => baml_type::RuntimeTy::Literal(
                baml_type::Literal::String(v.0.to_string()),
                baml_type::Freshness::Regular,
                attr,
            ),
            TyResolvedRef::LiteralBool(v) => baml_type::RuntimeTy::Literal(
                baml_type::Literal::Bool(v.0),
                baml_type::Freshness::Regular,
                attr,
            ),
            TyResolvedRef::Array(a) => a.to_baml_ty(db),
            TyResolvedRef::Map(m) => m.to_baml_ty(db),
            TyResolvedRef::Class(c) => c.to_baml_ty(db),
            TyResolvedRef::Enum(e) => e.to_baml_ty(db),
            TyResolvedRef::EnumVariant(ev) => ev.to_baml_ty(db),
            TyResolvedRef::Union(u) => u.to_baml_ty(db),
            TyResolvedRef::StreamState(s) => s.to_baml_ty(db),
        }
    }
}

impl ToBamlTy for Ty<'_, TypeName> {
    fn to_baml_ty(&self, db: &TypeRefDb<'_, TypeName>) -> baml_type::RuntimeTy {
        match self {
            Ty::Resolved(resolved) => resolved.as_ref().to_baml_ty(db),
            Ty::ResolvedRef(resolved_ref) => resolved_ref.to_baml_ty(db),
            Ty::Unresolved(name) => match db.resolve_name(name) {
                Some(TyResolvedRef::Class(class)) if class.name == *name => class.to_baml_ty(db),
                Some(TyResolvedRef::Enum(enm)) if enm.name == *name => enm.to_baml_ty(db),
                // Any other named entry is an alias. Keep it nominal here
                // instead of recursively expanding aliases such as `json`.
                Some(_) => {
                    baml_type::RuntimeTy::TypeAlias(name.clone(), baml_type::TyAttr::default())
                }
                None => baml_type::RuntimeTy::unknown(),
            },
        }
    }
}

impl ToBamlTy for ArrayTy<'_, TypeName> {
    fn to_baml_ty(&self, db: &TypeRefDb<'_, TypeName>) -> baml_type::RuntimeTy {
        let inner = self.ty.ty.to_baml_ty(db);
        baml_type::RuntimeTy::List(Box::new(inner), baml_type::TyAttr::default())
    }
}

impl ToBamlTy for MapTy<'_, TypeName> {
    fn to_baml_ty(&self, db: &TypeRefDb<'_, TypeName>) -> baml_type::RuntimeTy {
        baml_type::RuntimeTy::Map {
            key: Box::new(self.key.ty.to_baml_ty(db)),
            value: Box::new(self.value.ty.to_baml_ty(db)),
            attr: baml_type::TyAttr::default(),
        }
    }
}

impl ToBamlTy for ClassTy<'_, TypeName> {
    fn to_baml_ty(&self, _db: &TypeRefDb<'_, TypeName>) -> baml_type::RuntimeTy {
        baml_type::RuntimeTy::Class(self.name.clone(), Vec::new(), baml_type::TyAttr::default())
    }
}

impl ToBamlTy for EnumTy<'_, TypeName> {
    fn to_baml_ty(&self, _db: &TypeRefDb<'_, TypeName>) -> baml_type::RuntimeTy {
        baml_type::RuntimeTy::Enum(self.name.clone(), baml_type::TyAttr::default())
    }
}

impl ToBamlTy for EnumVariantTy<'_, TypeName> {
    /// Loses variant specificity — maps back to the parent enum type.
    fn to_baml_ty(&self, _db: &TypeRefDb<'_, TypeName>) -> baml_type::RuntimeTy {
        baml_type::RuntimeTy::Enum(self.name.clone(), baml_type::TyAttr::default())
    }
}

impl ToBamlTy for UnionTy<'_, TypeName> {
    fn to_baml_ty(&self, db: &TypeRefDb<'_, TypeName>) -> baml_type::RuntimeTy {
        let members: Vec<baml_type::RuntimeTy> =
            self.variants.iter().map(|v| v.ty.to_baml_ty(db)).collect();
        baml_type::RuntimeTy::Union(members, baml_type::TyAttr::default())
    }
}

impl ToBamlTy for StreamStateTy<'_, TypeName> {
    /// `StreamState` is a value-level concept; at the type level we return the inner type.
    fn to_baml_ty(&self, db: &TypeRefDb<'_, TypeName>) -> baml_type::RuntimeTy {
        self.value.ty.to_baml_ty(db)
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
    db: &TypeRefDb<'_, TypeName>,
) -> BexExternalValue {
    baml_value_inner_to_external(&value.value, &value.meta.ty, db)
}

fn baml_value_inner_to_external(
    value: &BamlValue<'_, '_, '_, TypeName>,
    ty: &TyWithMeta<TyResolvedRef<'_, TypeName>, &TypeAnnotations<'_, TypeName>>,
    db: &TypeRefDb<'_, TypeName>,
) -> BexExternalValue {
    match value {
        BamlValue::String(s) => BexExternalValue::String(bex_str::BexStr::from(s.value.as_ref())),
        BamlValue::Int(i) => BexExternalValue::Int(i.value),
        BamlValue::Bigint(b) => BexExternalValue::Bigint(b.value.clone()),
        BamlValue::Float(f) => BexExternalValue::Float(f.value),
        BamlValue::Bool(b) => BexExternalValue::Bool(b.value),
        BamlValue::Null(_) => BexExternalValue::Null,
        BamlValue::Media(_) => {
            unimplemented!("Media value conversion to BexExternalValue is not yet implemented")
        }
        BamlValue::Array(arr) => {
            let element_type = match ty.ty {
                TyResolvedRef::Array(a) => a.ty.ty.to_baml_ty(db),
                _ => baml_type::RuntimeTy::unknown(),
            };
            let items: Vec<BexExternalValue> = arr
                .value
                .iter()
                .map(|item| baml_value_to_external(item, db))
                .collect();
            BexExternalValue::Array {
                element_type,
                items,
            }
        }
        BamlValue::Map(map) => {
            let (key_type, value_type) = match ty.ty {
                TyResolvedRef::Map(m) => (m.key.ty.to_baml_ty(db), m.value.ty.to_baml_ty(db)),
                _ => (
                    baml_type::RuntimeTy::string(),
                    baml_type::RuntimeTy::unknown(),
                ),
            };
            let entries: IndexMap<String, BexExternalValue> = map
                .value
                .iter()
                .map(|(k, v)| (k.to_string(), baml_value_to_external(v, db)))
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
                .map(|(k, v)| (k.to_string(), baml_value_to_external(v, db)))
                .collect();
            BexExternalValue::Instance {
                class_name: c.name.to_string(),
                type_args: vec![],
                fields,
            }
        }
        BamlValue::StreamState(state) => stream_state_to_external(state, db),
    }
}

fn stream_state_to_external(
    state: &BamlStreamState<'_, '_, '_, TypeName>,
    db: &TypeRefDb<'_, TypeName>,
) -> BexExternalValue {
    let (inner, state_name) = match state {
        BamlStreamState::Pending(v) => (v, "Pending"),
        BamlStreamState::Incomplete(v) => (v, "Incomplete"),
        BamlStreamState::Complete(v) => (v, "Complete"),
    };
    let mut fields = IndexMap::new();
    fields.insert("value".to_string(), baml_value_to_external(inner, db));
    fields.insert(
        "state".to_string(),
        BexExternalValue::String(bex_str::BexStr::from(state_name)),
    );
    BexExternalValue::Instance {
        class_name: "StreamState".to_string(),
        type_args: vec![],
        fields,
    }
}
