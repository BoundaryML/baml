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
            Name::new("reflect"),
            vec![Name::new(self.namespace())],
            Name::new("Type"),
        )
    }

    pub fn concrete_class_ty(self) -> ConcreteRealizedTy {
        ConcreteRealizedTy::Class(self.class_name(), Vec::new(), TyAttr::default())
    }
}

/// Classify every realized runtime type into exactly one reflection kind.
pub fn classify_type(ty: &RealizedTy) -> TypeKind {
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
    name.package().as_str() == "reflect"
        && name.namespace().len() == 1
        && TypeKind::ALL
            .iter()
            .any(|kind| name.namespace()[0].as_str() == kind.namespace())
        && name.name().as_str() == "Type"
}

/// A builtin type and where its values actually come from, for the
/// companion carrier class that stands in for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinCompanion {
    /// The builtin type this class is the companion of.
    pub builtin: &'static str,
    /// How a value of that builtin is produced instead.
    pub origin: &'static str,
    /// Whether the class actually declares methods. `baml.Bool` and
    /// `baml.Null` have entirely empty bodies — they exist only to give
    /// their builtin a nominal companion — so a diagnostic must not tell
    /// the user they carry methods.
    pub carries_methods: bool,
}

/// The builtin a companion carrier class stands in for, or `None` when the
/// class is not a carrier.
///
/// These classes hold no fields; they exist so `5.abs()` and `"x".len()`
/// have somewhere to resolve. Constructing one yields a nonsense empty
/// instance (and, for the generic carriers, an error-recovery type that
/// reaches MIR lowering), so the two class-literal sites reject them.
///
/// Same discipline as [`QualifiedTypeName::is_builtin_root_type`]: the
/// `baml` package plus an EMPTY namespace, so a user's own `Int` class and
/// the field-carrying `reflect.class.Field` are both untouched.
pub fn builtin_companion_of(name: &QualifiedTypeName) -> Option<BuiltinCompanion> {
    if name.package().as_str() == "reflect"
        && name.namespace().is_empty()
        && name.name().as_str() == "Type"
    {
        return Some(BuiltinCompanion {
            builtin: "reflect.Type",
            origin: "`reflect.Type.of<T>()` and reflection",
            carries_methods: true,
        });
    }
    if name.package().as_str() != "baml" || !name.namespace().is_empty() {
        return None;
    }
    let (builtin, origin, carries_methods) = match name.name().as_str() {
        "Int" => ("int", "literals", true),
        "Bigint" => ("bigint", "literals", true),
        "Float" => ("float", "literals", true),
        "String" => ("string", "literals", true),
        "Bool" => ("bool", "literals", false),
        "Null" => ("null", "the `null` literal", false),
        "Uint8Array" => ("uint8array", "byte-string literals", true),
        "Array" => ("array", "array literals", true),
        "Map" => ("map", "map literals", true),
        _ => return None,
    };
    Some(BuiltinCompanion {
        builtin,
        origin,
        carries_methods,
    })
}

/// Whether a nominal class inhabits the compiler-derived `baml.AnyClass`
/// interface. Ordinary classes do. Within the sealed reflection-kind family,
/// only the class-kind value view is intentionally admitted.
pub fn class_inhabits_any_class(name: &QualifiedTypeName) -> bool {
    !is_type_kind_class(name) || name == &TypeKind::Class.class_name()
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
            "reflect.Type"
        )));
    }

    #[test]
    fn companion_carriers_need_the_baml_package_and_an_empty_namespace() {
        for (path, builtin) in [
            ("baml.Int", "int"),
            ("baml.Bigint", "bigint"),
            ("baml.Float", "float"),
            ("baml.String", "string"),
            ("baml.Bool", "bool"),
            ("baml.Null", "null"),
            ("baml.Uint8Array", "uint8array"),
            ("baml.Array", "array"),
            ("baml.Map", "map"),
            ("reflect.Type", "reflect.Type"),
        ] {
            assert_eq!(
                builtin_companion_of(&QualifiedTypeName::from_dotted_path(path))
                    .map(|companion| companion.builtin),
                Some(builtin),
                "{path} should be a companion carrier"
            );
        }
        for path in [
            // A user package may name a class whatever it likes.
            "user.Int",
            "user.Map",
            // Namespaced `baml` classes are out of scope, including the
            // field-carrying reflect classes the stdlib itself constructs.
            "reflect.class.Field",
            "reflect.class.Type",
            "baml.media.Image",
            "baml.iter.Done",
            // A root-namespace `baml` class that really does hold fields.
            "baml.TaggedString",
        ] {
            assert_eq!(
                builtin_companion_of(&QualifiedTypeName::from_dotted_path(path)),
                None,
                "{path} should not be a companion carrier"
            );
        }
    }

    #[test]
    fn any_class_admits_ordinary_classes_and_only_the_class_kind_view() {
        assert!(class_inhabits_any_class(
            &QualifiedTypeName::from_dotted_path("user.Record")
        ));
        for kind in TypeKind::ALL {
            assert_eq!(
                class_inhabits_any_class(&kind.class_name()),
                kind == TypeKind::Class,
                "unexpected AnyClass membership for {kind:?}"
            );
        }
    }
}
