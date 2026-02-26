//! Contains [`TypeName`] trait and implementations.

use std::{borrow::Cow, fmt};

use crate::sap_model::*;

/// A trait that provides a type name for a given type.
/// The name may be static (`"int"`) or dynamic (`"literal[42]"` and `"array[int | string]"`).
pub trait TypeName {
    fn type_name(&self) -> Cow<'static, str>;
}
impl<T: TypeName> TypeName for &'_ T {
    fn type_name(&self) -> Cow<'static, str> {
        T::type_name(self)
    }
}
impl<T: TypeName, M> TypeName for TyWithMeta<T, M> {
    fn type_name(&self) -> Cow<'static, str> {
        self.ty.type_name()
    }
}

macro_rules! impl_type_name {
    ($ty:ty => $name:literal) => {
        impl TypeName for $ty {
            fn type_name(&self) -> Cow<'static, str> {
                Cow::Borrowed($name)
            }
        }
    };
}
impl_type_name!(IntTy => "int");
impl_type_name!(FloatTy => "float");
impl_type_name!(StringTy => "string");
impl_type_name!(BoolTy => "bool");
impl_type_name!(NullTy => "null");

impl TypeName for MediaTy {
    fn type_name(&self) -> Cow<'static, str> {
        match self {
            MediaTy::Image => Cow::Borrowed("image"),
            MediaTy::Audio => Cow::Borrowed("audio"),
            MediaTy::Pdf => Cow::Borrowed("pdf"),
            MediaTy::Video => Cow::Borrowed("video"),
        }
    }
}

impl TypeName for PrimitiveTy {
    fn type_name(&self) -> Cow<'static, str> {
        match self {
            PrimitiveTy::Int(int) => int.type_name(),
            PrimitiveTy::Float(float) => float.type_name(),
            PrimitiveTy::String(string) => string.type_name(),
            PrimitiveTy::Bool(bool) => bool.type_name(),
            PrimitiveTy::Null(null) => null.type_name(),
            PrimitiveTy::Media(media) => media.type_name(),
        }
    }
}

impl TypeName for IntLiteralTy {
    fn type_name(&self) -> Cow<'static, str> {
        Cow::Owned(format!("literal[{}]", self.0))
    }
}
impl TypeName for StringLiteralTy<'_> {
    fn type_name(&self) -> Cow<'static, str> {
        if self.0.is_empty() {
            return Cow::Borrowed("string");
        }
        Cow::Owned(format!("literal[{:?}]", self.0))
    }
}
impl TypeName for BoolLiteralTy {
    fn type_name(&self) -> Cow<'static, str> {
        Cow::Owned(format!("literal[{}]", self.0))
    }
}

impl TypeName for LiteralTy<'_> {
    fn type_name(&self) -> Cow<'static, str> {
        match self {
            LiteralTy::String(s) => s.type_name(),
            LiteralTy::Int(i) => i.type_name(),
            LiteralTy::Bool(b) => b.type_name(),
        }
    }
}

impl<N: TypeIdent> TypeName for Ty<'_, N> {
    fn type_name(&self) -> Cow<'static, str> {
        match self {
            Ty::Resolved(r) => r.type_name(),
            Ty::ResolvedRef(r) => r.type_name(),
            Ty::Unresolved(n) => Cow::Owned(n.to_string()),
        }
    }
}

impl<N: TypeIdent> TypeName for ArrayTy<'_, N> {
    fn type_name(&self) -> Cow<'static, str> {
        Cow::Owned(format!("{}[]", self.ty.type_name()))
    }
}

impl<N: TypeIdent> TypeName for MapTy<'_, N> {
    fn type_name(&self) -> Cow<'static, str> {
        Cow::Owned(format!(
            "map<{}, {}>",
            self.key.type_name(),
            self.value.type_name()
        ))
    }
}

impl<N: TypeIdent> TypeName for ClassTy<'_, N> {
    fn type_name(&self) -> Cow<'static, str> {
        Cow::Owned(self.name.to_string())
    }
}

impl<N: TypeIdent> TypeName for EnumTy<'_, N> {
    fn type_name(&self) -> Cow<'static, str> {
        Cow::Owned(self.name.to_string())
    }
}

impl<N: TypeIdent> TypeName for UnionTy<'_, N> {
    fn type_name(&self) -> Cow<'static, str> {
        let variants: Vec<_> = self.variants.iter().map(|v| v.type_name()).collect();
        Cow::Owned(variants.join(" | "))
    }
}

impl<N: TypeIdent> TypeName for TyResolved<'_, N> {
    fn type_name(&self) -> Cow<'static, str> {
        match self {
            TyResolved::Primitive(p) => p.type_name(),
            TyResolved::Literal(l) => l.type_name(),
            TyResolved::Array(a) => a.type_name(),
            TyResolved::Map(m) => m.type_name(),
            TyResolved::Class(c) => c.type_name(),
            TyResolved::Enum(e) => e.type_name(),
            TyResolved::Union(u) => u.type_name(),
            TyResolved::StreamState(s) => Cow::Owned(format!("stream_state<{}>", s.type_name())),
        }
    }
}

impl<N: TypeIdent> TypeName for StreamStateTy<'_, N> {
    fn type_name(&self) -> Cow<'static, str> {
        Cow::Owned(format!("stream_state<{}>", self.value.type_name()))
    }
}

impl<N: TypeIdent> TypeName for TyResolvedRef<'_, N> {
    fn type_name(&self) -> Cow<'static, str> {
        match self {
            TyResolvedRef::Primitive(p) => p.type_name(),
            TyResolvedRef::Literal(l) => l.type_name(),
            TyResolvedRef::Array(a) => a.type_name(),
            TyResolvedRef::Map(m) => m.type_name(),
            TyResolvedRef::Class(c) => c.type_name(),
            TyResolvedRef::Enum(e) => e.type_name(),
            TyResolvedRef::Union(u) => u.type_name(),
            TyResolvedRef::StreamState(s) => Cow::Owned(format!("stream_state<{}>", s.type_name())),
        }
    }
}

// Display delegates to TypeName for any T: TypeName wrapper.
impl<T: TypeName, M> fmt::Display for TyWithMeta<T, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.ty.type_name())
    }
}

// Manual Debug for TyResolvedRef since N: TypeIdent doesn't require Debug.
impl<N: TypeIdent> fmt::Debug for TyResolvedRef<'_, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.type_name())
    }
}
