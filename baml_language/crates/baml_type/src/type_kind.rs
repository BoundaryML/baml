//! Closed BEP-066 reflection-kind classification.

use baml_base::{LangPackage, LangRoots, SourceRoot};

use crate::{DeclName, Name, QualifiedTypeName, RealizedTy};

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

    /// The builtin class that is the sealed view for this kind, by wire name.
    pub fn class_name(self) -> QualifiedTypeName {
        QualifiedTypeName::new(
            Name::new("reflect"),
            vec![Name::new(self.namespace())],
            Name::new("Type"),
        )
    }

    /// [`Self::class_name`] in the installed `reflect` package.
    pub fn class_decl(self, reflect: SourceRoot) -> DeclName {
        DeclName::in_root(
            reflect,
            vec![Name::new(self.namespace())],
            Name::new("Type"),
        )
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

/// Whether a nominal class is one of the nine sealed reflection-kind classes,
/// by wire name.
pub fn is_type_kind_class(name: &QualifiedTypeName) -> bool {
    name.package().as_str() == "reflect" && is_type_kind_class_path(name.namespace(), name.name())
}

/// [`is_type_kind_class`] for a compile-time head: the class must live in the
/// installed `reflect` package.
pub fn is_type_kind_class_decl(lang: LangRoots, name: &DeclName) -> bool {
    lang.is(LangPackage::Reflect, name.root())
        && is_type_kind_class_path(name.namespace(), name.name())
}

/// The `<kind>.Type` shape inside the `reflect` package.
fn is_type_kind_class_path(namespace: &[Name], name: &Name) -> bool {
    namespace.len() == 1
        && TypeKind::ALL
            .iter()
            .any(|kind| namespace[0].as_str() == kind.namespace())
        && name.as_str() == "Type"
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
    let package = match name.package().as_str() {
        "reflect" => LangPackage::Reflect,
        "baml" => LangPackage::Baml,
        _ => return None,
    };
    builtin_companion_in(package, name.namespace(), name.name())
}

/// [`builtin_companion_of`] for a compile-time head: the class must live in
/// the installed `reflect` or `baml` package.
pub fn builtin_companion_of_decl(lang: LangRoots, name: &DeclName) -> Option<BuiltinCompanion> {
    let package = [LangPackage::Reflect, LangPackage::Baml]
        .into_iter()
        .find(|package| lang.is(*package, name.root()))?;
    builtin_companion_in(package, name.namespace(), name.name())
}

fn builtin_companion_in(
    package: LangPackage,
    namespace: &[Name],
    name: &Name,
) -> Option<BuiltinCompanion> {
    if !namespace.is_empty() {
        return None;
    }
    match package {
        LangPackage::Reflect => {
            return (name.as_str() == "Type").then_some(BuiltinCompanion {
                builtin: "reflect.Type",
                origin: "`reflect.Type.of<T>()` and reflection",
                carries_methods: true,
            });
        }
        LangPackage::Baml => {}
        LangPackage::Ai | LangPackage::Boundary => return None,
    }
    let (builtin, origin, carries_methods) = match name.as_str() {
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
}
