//! Conversion from SAP model types back to [`ParseTy`].
//!
//! A parsed value's metadata carries SAP model types; the VM, which builds the
//! value on the heap, turns them into the element, key, and value types of the
//! lists and maps it allocates. Those must be exactly the types the target
//! declares, as a container is never covariant: a `Speaker[]` that parsed
//! empty is still a `Speaker[]`, never a `never[]` or an `unknown[]`.

use crate::sap_model::{
    ArrayTy, ClassTy, EnumTy, EnumVariantTy, MapTy, MediaTy, NamedTy, ParseTy, StreamStateTy,
    TupleTy, Ty, TyResolvedRef, UnionTy,
};

// ============================================================================
// Type conversion: SAP types → ParseTy
// ============================================================================

/// Convert a SAP type back to the `ParseTy` it models.
///
/// The result is the same type as the one converted into the model (they hold
/// the same values), though possibly spelled otherwise: the model holds the
/// target as [`baml_type::simplify_sap`] rewrote it (a union flat, `5 | int` as
/// `int`, a non-recursive alias inlined). Every named type, interfaces
/// included, converts back through its [`NamedTy`], exactly.
pub trait ToBamlTy {
    fn to_baml_ty(&self) -> ParseTy;
}

impl ToBamlTy for TyResolvedRef<'_, NamedTy> {
    fn to_baml_ty(&self) -> ParseTy {
        match self {
            TyResolvedRef::Int(_) => ParseTy::Int,
            TyResolvedRef::Bigint(_) => ParseTy::Bigint,
            TyResolvedRef::Float(_) => ParseTy::Float,
            TyResolvedRef::String(_) => ParseTy::String,
            TyResolvedRef::Bool(_) => ParseTy::Bool,
            TyResolvedRef::Null(_) => ParseTy::Null,
            TyResolvedRef::Media(media) => ParseTy::Media(media.to_baml_media_kind()),
            TyResolvedRef::LiteralInt(v) => {
                ParseTy::Literal(baml_type::Literal::Int(v.0), baml_type::Freshness::Regular)
            }
            TyResolvedRef::LiteralBigint(v) => ParseTy::Literal(
                baml_type::Literal::Bigint(v.0.clone()),
                baml_type::Freshness::Regular,
            ),
            TyResolvedRef::LiteralString(v) => ParseTy::Literal(
                baml_type::Literal::String(v.0.to_string()),
                baml_type::Freshness::Regular,
            ),
            TyResolvedRef::LiteralBool(v) => {
                ParseTy::Literal(baml_type::Literal::Bool(v.0), baml_type::Freshness::Regular)
            }
            TyResolvedRef::Array(a) => a.to_baml_ty(),
            TyResolvedRef::Tuple(t) => t.to_baml_ty(),
            TyResolvedRef::Map(m) => m.to_baml_ty(),
            TyResolvedRef::Class(c) => c.to_baml_ty(),
            TyResolvedRef::Enum(e) => e.to_baml_ty(),
            TyResolvedRef::EnumVariant(ev) => ev.to_baml_ty(),
            TyResolvedRef::Union(u) => u.to_baml_ty(),
            TyResolvedRef::StreamState(s) => s.to_baml_ty(),
        }
    }
}

impl ToBamlTy for Ty<'_, NamedTy> {
    fn to_baml_ty(&self) -> ParseTy {
        match self {
            Ty::Resolved(resolved) => resolved.as_ref().to_baml_ty(),
            Ty::ResolvedRef(resolved_ref) => resolved_ref.to_baml_ty(),
            // A named type converts back by name, never through its entry: an
            // alias such as `json` stays nominal, and an interface's entry is
            // `never`, which is not the interface.
            Ty::Unresolved(name) => name.to_ty(),
        }
    }
}

impl ToBamlTy for ArrayTy<'_, NamedTy> {
    fn to_baml_ty(&self) -> ParseTy {
        ParseTy::List(Box::new(self.ty.to_baml_ty()))
    }
}

impl ToBamlTy for TupleTy<'_, NamedTy> {
    fn to_baml_ty(&self) -> ParseTy {
        ParseTy::Tuple(self.items.iter().map(ToBamlTy::to_baml_ty).collect())
    }
}

impl ToBamlTy for MapTy<'_, NamedTy> {
    fn to_baml_ty(&self) -> ParseTy {
        ParseTy::Map {
            key: Box::new(self.key.to_baml_ty()),
            value: Box::new(self.value.to_baml_ty()),
        }
    }
}

impl ToBamlTy for ClassTy<'_, NamedTy> {
    fn to_baml_ty(&self) -> ParseTy {
        self.name.to_ty()
    }
}

impl ToBamlTy for EnumTy<'_, NamedTy> {
    fn to_baml_ty(&self) -> ParseTy {
        self.name.to_ty()
    }
}

impl ToBamlTy for EnumVariantTy<'_, NamedTy> {
    /// The variant's own type, `Color.Red`: a narrower type than its enum.
    fn to_baml_ty(&self) -> ParseTy {
        ParseTy::EnumVariant(
            self.name.decl().clone(),
            baml_type::Name::new(self.value.name.as_ref()),
        )
    }
}

impl ToBamlTy for UnionTy<'_, NamedTy> {
    fn to_baml_ty(&self) -> ParseTy {
        ParseTy::Union(self.variants.iter().map(ToBamlTy::to_baml_ty).collect())
    }
}

impl ToBamlTy for StreamStateTy<'_, NamedTy> {
    /// `StreamState` is a value-level concept; at the type level we return the inner type.
    fn to_baml_ty(&self) -> ParseTy {
        self.value.to_baml_ty()
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
