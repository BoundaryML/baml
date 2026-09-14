//! Conversion from SAP types and values to `BexExternalValue` and `SapTy`.
//!
//! This module provides the bridge between SAP's internal type/value representation
//! and the external value tree used by the engine to push results back onto the VM heap.

use bex_external_types::BexExternalValue;
use indexmap::IndexMap;
use sys_types::{DefKey, SapTy};

use crate::{
    baml_value::{BamlStreamState, BamlValue},
    deserializer::types::BamlValueWithFlags,
    sap_model::{
        ArrayTy, ClassTy, EnumTy, EnumVariantTy, MapTy, MediaTy, StreamStateTy, Ty, TyResolvedRef,
        TyWithMeta, TypeAnnotations, TypeRefDb, UnionTy,
    },
};

// ============================================================================
// Type conversion: SAP types → SapTy
// ============================================================================

/// Convert a SAP type back to a `SapTy`.
///
/// # Known simplifications (not round-trip safe)
///
/// - `Optional(T)` was converted to `Union([Null, T])` on the way in; it stays as `Union` here.
/// - Aliases already resolved into the target type have no nominal name to recover; named alias
///   references inside containers are preserved.
/// - `EnumVariant` becomes `Enum` (variant specificity lost at the type level).
/// - `TyAttr` annotations are not reconstructed; `TyAttr::default()` is used throughout.
pub trait ToBamlTy {
    fn to_baml_ty(&self, db: &TypeRefDb<'_, DefKey>) -> SapTy;
}

impl ToBamlTy for TyResolvedRef<'_, DefKey> {
    fn to_baml_ty(&self, db: &TypeRefDb<'_, DefKey>) -> SapTy {
        let attr = baml_type::TyAttr::default();
        match self {
            TyResolvedRef::Int(_) => SapTy::Int { attr },
            TyResolvedRef::Bigint(_) => SapTy::Bigint { attr },
            TyResolvedRef::Float(_) => SapTy::Float { attr },
            TyResolvedRef::String(_) => SapTy::String { attr },
            TyResolvedRef::Bool(_) => SapTy::Bool { attr },
            TyResolvedRef::Null(_) => SapTy::Null { attr },
            TyResolvedRef::Media(media) => SapTy::Media(media.to_baml_media_kind(), attr),
            TyResolvedRef::LiteralInt(v) => SapTy::Literal(
                baml_type::Literal::Int(v.0),
                baml_type::Freshness::Regular,
                attr,
            ),
            TyResolvedRef::LiteralBigint(v) => SapTy::Literal(
                baml_type::Literal::Bigint(v.0.clone()),
                baml_type::Freshness::Regular,
                attr,
            ),
            TyResolvedRef::LiteralString(v) => SapTy::Literal(
                baml_type::Literal::String(v.0.to_string()),
                baml_type::Freshness::Regular,
                attr,
            ),
            TyResolvedRef::LiteralBool(v) => SapTy::Literal(
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

impl ToBamlTy for Ty<'_, DefKey> {
    fn to_baml_ty(&self, db: &TypeRefDb<'_, DefKey>) -> SapTy {
        match self {
            Ty::Resolved(resolved) => resolved.as_ref().to_baml_ty(db),
            Ty::ResolvedRef(resolved_ref) => resolved_ref.to_baml_ty(db),
            Ty::Unresolved(name) => match db.resolve_name(name) {
                Some(TyResolvedRef::Class(class)) if class.name == *name => class.to_baml_ty(db),
                Some(TyResolvedRef::Enum(enm)) if enm.name == *name => enm.to_baml_ty(db),
                // Any other named entry is an alias. Keep it nominal here
                // instead of recursively expanding aliases such as `json`.
                Some(_) => SapTy::TypeAlias(name.clone(), baml_type::TyAttr::default()),
                None => SapTy::unknown(),
            },
        }
    }
}

impl ToBamlTy for ArrayTy<'_, DefKey> {
    fn to_baml_ty(&self, db: &TypeRefDb<'_, DefKey>) -> SapTy {
        let inner = self.ty.ty.to_baml_ty(db);
        SapTy::List(Box::new(inner), baml_type::TyAttr::default())
    }
}

impl ToBamlTy for MapTy<'_, DefKey> {
    fn to_baml_ty(&self, db: &TypeRefDb<'_, DefKey>) -> SapTy {
        SapTy::Map {
            key: Box::new(self.key.ty.to_baml_ty(db)),
            value: Box::new(self.value.ty.to_baml_ty(db)),
            attr: baml_type::TyAttr::default(),
        }
    }
}

impl ToBamlTy for ClassTy<'_, DefKey> {
    fn to_baml_ty(&self, _db: &TypeRefDb<'_, DefKey>) -> SapTy {
        SapTy::Class(
            self.name.clone(),
            Box::new([]),
            baml_type::TyAttr::default(),
        )
    }
}

impl ToBamlTy for EnumTy<'_, DefKey> {
    fn to_baml_ty(&self, _db: &TypeRefDb<'_, DefKey>) -> SapTy {
        SapTy::Enum(self.name.clone(), baml_type::TyAttr::default())
    }
}

impl ToBamlTy for EnumVariantTy<'_, DefKey> {
    /// Loses variant specificity — maps back to the parent enum type.
    fn to_baml_ty(&self, _db: &TypeRefDb<'_, DefKey>) -> SapTy {
        SapTy::Enum(self.name.clone(), baml_type::TyAttr::default())
    }
}

impl ToBamlTy for UnionTy<'_, DefKey> {
    fn to_baml_ty(&self, db: &TypeRefDb<'_, DefKey>) -> SapTy {
        let members: Vec<SapTy> = self.variants.iter().map(|v| v.ty.to_baml_ty(db)).collect();
        SapTy::Union(members.into(), baml_type::TyAttr::default())
    }
}

impl ToBamlTy for StreamStateTy<'_, DefKey> {
    /// `StreamState` is a value-level concept; at the type level we return the inner type.
    fn to_baml_ty(&self, db: &TypeRefDb<'_, DefKey>) -> SapTy {
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

/// Convert a lane type to the name-headed form `BexExternalValue` carries.
///
/// The value metadata a parsed result carries has nowhere to put a declaration
/// identity, so it converts here — through the same per-call overlay spelling
/// the definition tables for this call were built with, so a returned value's
/// type names agree with the ones that call rendered. An anonymous declaration
/// has no resolvable name; its overlay spelling is meaningful only inside this
/// call and must not be treated as a lookup key beyond it.
fn to_value_metadata_ty(ty: &SapTy) -> baml_type::RuntimeTy {
    match ty.try_map_heads(&mut |head: &DefKey| {
        Ok::<_, std::convert::Infallible>(head.name().overlay_name())
    }) {
        Ok(converted) => converted,
        Err(never) => match never {},
    }
}

/// Convert a SAP `BamlValueWithFlags` to a `BexExternalValue`, using the
/// type metadata from the deserializer to populate type information on
/// composite values (arrays, maps).
///
/// Deserializer metadata (flags, scores) is discarded.
pub fn baml_value_to_external(
    value: &BamlValueWithFlags<'_, '_, '_, DefKey>,
    db: &TypeRefDb<'_, DefKey>,
) -> BexExternalValue {
    baml_value_inner_to_external(&value.value, &value.meta.ty, db)
}

fn baml_value_inner_to_external(
    value: &BamlValue<'_, '_, '_, DefKey>,
    ty: &TyWithMeta<TyResolvedRef<'_, DefKey>, &TypeAnnotations<'_, DefKey>>,
    db: &TypeRefDb<'_, DefKey>,
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
            let element_type = to_value_metadata_ty(&match ty.ty {
                TyResolvedRef::Array(a) => a.ty.ty.to_baml_ty(db),
                _ => SapTy::unknown(),
            });
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
                _ => (SapTy::string(), SapTy::unknown()),
            };
            let (key_type, value_type) = (
                to_value_metadata_ty(&key_type),
                to_value_metadata_ty(&value_type),
            );
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
            // As for classes above: the qualified overlay spelling.
            enum_name: e.name.name().overlay_name().to_string(),
            variant_name: e.value.to_string(),
        },
        BamlValue::Class(c) => {
            let fields: IndexMap<String, BexExternalValue> = c
                .value
                .iter()
                .map(|(k, v)| (k.to_string(), baml_value_to_external(v, db)))
                .collect();
            BexExternalValue::Instance {
                // The landing resolves this against the per-call handle view,
                // which is keyed by the *qualified* overlay spelling — so emit
                // that, not the display name. One declaration has several name
                // forms; the two sides of this hand-off must pick the same one.
                class_name: c.name.name().overlay_name().to_string(),
                type_args: vec![],
                fields,
            }
        }
        BamlValue::StreamState(state) => stream_state_to_external(state, db),
    }
}

fn stream_state_to_external(
    state: &BamlStreamState<'_, '_, '_, DefKey>,
    db: &TypeRefDb<'_, DefKey>,
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
