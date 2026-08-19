//! Closed BEP-066 reflection-kind classification.

use crate::{ConcreteRealizedTy, Name, QualifiedTypeName, RealizedTy, TyAttr};

/// The nine sealed runtime views of a reflected `type` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeKind {
    Class,
    Enum,
    Union,
    Literal,
    Array,
    Map,
    Interface,
    Primitive,
    Function,
}

impl TypeKind {
    pub const ALL: [Self; 9] = [
        Self::Class,
        Self::Enum,
        Self::Union,
        Self::Literal,
        Self::Array,
        Self::Map,
        Self::Interface,
        Self::Primitive,
        Self::Function,
    ];

    pub const fn namespace(self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Enum => "enum",
            Self::Union => "union",
            Self::Literal => "literal",
            Self::Array => "array",
            Self::Map => "map",
            Self::Interface => "interface",
            Self::Primitive => "primitive",
            Self::Function => "function",
        }
    }

    /// The builtin class that is the sealed view for this kind.
    pub fn class_name(self) -> QualifiedTypeName {
        QualifiedTypeName::new(
            Name::new("baml"),
            vec![Name::new("reflect"), Name::new(self.namespace())],
            Name::new("Type"),
        )
    }

    pub fn concrete_class_ty(self) -> ConcreteRealizedTy {
        ConcreteRealizedTy::Class(self.class_name(), Vec::new(), TyAttr::default())
    }
}

/// Classify every realized runtime type into exactly one reflection kind.
///
/// Shape only — no head is inspected — so this answers the same at either head.
pub fn classify_type<N: Clone>(ty: &RealizedTy<N>) -> TypeKind {
    match ty {
        RealizedTy::Class(..) => TypeKind::Class,
        RealizedTy::Enum(..) => TypeKind::Enum,
        RealizedTy::Union(..) => TypeKind::Union,
        RealizedTy::Literal(..) | RealizedTy::EnumVariant(..) => TypeKind::Literal,
        RealizedTy::List(..) => TypeKind::Array,
        RealizedTy::Map { .. } => TypeKind::Map,
        RealizedTy::Interface(..) => TypeKind::Interface,
        RealizedTy::Function { .. } => TypeKind::Function,
        _ => TypeKind::Primitive,
    }
}

/// Whether a nominal class is one of the nine sealed reflection-kind classes.
pub fn is_type_kind_class(name: &QualifiedTypeName) -> bool {
    name.package().as_str() == "baml"
        && name.namespace().len() == 2
        && name.namespace()[0].as_str() == "reflect"
        && TypeKind::ALL
            .iter()
            .any(|kind| name.namespace()[1].as_str() == kind.namespace())
        && name.name().as_str() == "Type"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_names_are_closed_and_recognized() {
        for kind in TypeKind::ALL {
            assert!(is_type_kind_class(&kind.class_name()));
        }
        assert!(!is_type_kind_class(&QualifiedTypeName::from_dotted_path(
            "baml.reflect.Type"
        )));
    }
}
