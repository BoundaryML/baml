//! Conversion from SAP model types back to `SapTy`.
//!
//! A parsed value's metadata carries SAP model types; the VM, which builds the
//! value on the heap, turns them into the element, key, and value types of the
//! lists and maps it allocates.

use sys_types::{DefKey, SapTy};

use crate::sap_model::{
    ArrayTy, ClassTy, EnumTy, EnumVariantTy, MapTy, MediaTy, StreamStateTy, Ty, TyResolvedRef,
    TypeRefDb, UnionTy,
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
pub trait ToBamlTy {
    fn to_baml_ty(&self, db: &TypeRefDb<'_, DefKey>) -> SapTy;
}

impl ToBamlTy for TyResolvedRef<'_, DefKey> {
    fn to_baml_ty(&self, db: &TypeRefDb<'_, DefKey>) -> SapTy {
        match self {
            TyResolvedRef::Int(_) => SapTy::Int,
            TyResolvedRef::Bigint(_) => SapTy::Bigint,
            TyResolvedRef::Float(_) => SapTy::Float,
            TyResolvedRef::String(_) => SapTy::String,
            TyResolvedRef::Bool(_) => SapTy::Bool,
            TyResolvedRef::Null(_) => SapTy::Null,
            TyResolvedRef::Media(media) => SapTy::Media(media.to_baml_media_kind()),
            TyResolvedRef::LiteralInt(v) => {
                SapTy::Literal(baml_type::Literal::Int(v.0), baml_type::Freshness::Regular)
            }
            TyResolvedRef::LiteralBigint(v) => SapTy::Literal(
                baml_type::Literal::Bigint(v.0.clone()),
                baml_type::Freshness::Regular,
            ),
            TyResolvedRef::LiteralString(v) => SapTy::Literal(
                baml_type::Literal::String(v.0.to_string()),
                baml_type::Freshness::Regular,
            ),
            TyResolvedRef::LiteralBool(v) => {
                SapTy::Literal(baml_type::Literal::Bool(v.0), baml_type::Freshness::Regular)
            }
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
                Some(_) => SapTy::TypeAlias(name.clone()),
                None => SapTy::unknown(),
            },
        }
    }
}

impl ToBamlTy for ArrayTy<'_, DefKey> {
    fn to_baml_ty(&self, db: &TypeRefDb<'_, DefKey>) -> SapTy {
        let inner = self.ty.to_baml_ty(db);
        SapTy::List(Box::new(inner))
    }
}

impl ToBamlTy for MapTy<'_, DefKey> {
    fn to_baml_ty(&self, db: &TypeRefDb<'_, DefKey>) -> SapTy {
        SapTy::Map {
            key: Box::new(self.key.to_baml_ty(db)),
            value: Box::new(self.value.to_baml_ty(db)),
        }
    }
}

impl ToBamlTy for ClassTy<'_, DefKey> {
    fn to_baml_ty(&self, _db: &TypeRefDb<'_, DefKey>) -> SapTy {
        SapTy::Class(self.name.clone(), Box::new([]))
    }
}

impl ToBamlTy for EnumTy<'_, DefKey> {
    fn to_baml_ty(&self, _db: &TypeRefDb<'_, DefKey>) -> SapTy {
        SapTy::Enum(self.name.clone())
    }
}

impl ToBamlTy for EnumVariantTy<'_, DefKey> {
    /// Loses variant specificity — maps back to the parent enum type.
    fn to_baml_ty(&self, _db: &TypeRefDb<'_, DefKey>) -> SapTy {
        SapTy::Enum(self.name.clone())
    }
}

impl ToBamlTy for UnionTy<'_, DefKey> {
    fn to_baml_ty(&self, db: &TypeRefDb<'_, DefKey>) -> SapTy {
        let members: Vec<SapTy> = self.variants.iter().map(|v| v.to_baml_ty(db)).collect();
        SapTy::Union(members.into())
    }
}

impl ToBamlTy for StreamStateTy<'_, DefKey> {
    /// `StreamState` is a value-level concept; at the type level we return the inner type.
    fn to_baml_ty(&self, db: &TypeRefDb<'_, DefKey>) -> SapTy {
        self.value.to_baml_ty(db)
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
