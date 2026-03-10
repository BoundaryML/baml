//! Type system for BAML.

use std::fmt;

use baml_base::TyAttr;
use baml_compiler_hir::QualifiedName;

/// The value component of a literal type.
///
/// In type theory, literal types (also called singleton types) are types
/// inhabited by exactly one value. For example, `LiteralValue::Int(42)`
/// represents the value `42` which defines the literal type `{42}` — the
/// type whose only inhabitant is `42`.
///
/// Used for exhaustiveness checking of literal unions like `200 | 201 | 204`.
///
/// Note: Float values are stored as strings because floating-point
/// equality is problematic (NaN != NaN, -0.0 == 0.0, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LiteralValue {
    Int(i64),
    /// Float literal stored as string to avoid f64's lack of Eq/Hash.
    Float(std::string::String),
    String(std::string::String),
    Bool(bool),
}

impl fmt::Display for LiteralValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LiteralValue::Int(v) => write!(f, "{v}"),
            LiteralValue::Float(v) => write!(f, "{v}"),
            LiteralValue::String(v) => write!(f, "\"{v}\""),
            LiteralValue::Bool(v) => write!(f, "{v}"),
        }
    }
}

/// A resolved type in BAML.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    // Primitive types
    Int {
        attr: TyAttr<QualifiedName>,
    },
    Float {
        attr: TyAttr<QualifiedName>,
    },
    String {
        attr: TyAttr<QualifiedName>,
    },
    Bool {
        attr: TyAttr<QualifiedName>,
    },
    Null {
        attr: TyAttr<QualifiedName>,
    },

    // Media types
    Media(baml_base::MediaKind, TyAttr<QualifiedName>),

    /// Literal type: a type inhabited by exactly one value (also called singleton type).
    /// Used for exhaustiveness checking of literal unions like `200 | 201 | 204`.
    /// The literal type `{42}` contains only the value `42`.
    ///
    /// Note: `Ty::Null` is also a singleton type but is kept as a separate variant
    /// for convenience, since null is fundamental to optional types.
    Literal(LiteralValue, TyAttr<QualifiedName>),

    // User-defined types (resolved with fully-qualified names)
    /// A class type with its fully-qualified name.
    Class(QualifiedName, TyAttr<QualifiedName>),
    /// An enum type with its fully-qualified name.
    Enum(QualifiedName, TyAttr<QualifiedName>),
    /// A type alias with its fully-qualified name.
    /// Type aliases are NOT expanded during resolution - they stay as `TypeAlias(FQN)`.
    /// Expansion happens later during normalization when needed for subtype checks.
    /// This preserves user spelling for error messages and handles recursive types.
    TypeAlias(QualifiedName, TyAttr<QualifiedName>),

    // Type constructors
    Optional(Box<Ty>, TyAttr<QualifiedName>),
    List(Box<Ty>, TyAttr<QualifiedName>),
    Map {
        key: Box<Ty>,
        value: Box<Ty>,
        attr: TyAttr<QualifiedName>,
    },
    Union(Vec<Ty>, TyAttr<QualifiedName>),

    /// Function/arrow type: `(T1, T2, ...) -> R`
    ///
    /// Parameter names are optional and used only for diagnostics (e.g., error messages
    /// like "missing argument 'foo'"). They are ignored in type equality and subtyping.
    Function {
        params: Vec<(Option<baml_base::Name>, Ty)>,
        ret: Box<Ty>,
        attr: TyAttr<QualifiedName>,
    },

    // Special types
    /// Type inference failed - used for error recovery.
    /// Treated as uninhabited to prevent cascading errors.
    Unknown {
        attr: TyAttr<QualifiedName>,
    },
    /// A type error occurred.
    Error {
        attr: TyAttr<QualifiedName>,
    },
    /// The void/unit type for functions that return nothing.
    Void {
        attr: TyAttr<QualifiedName>,
    },

    /// Opaque resource handle (file, socket, HTTP response body).
    Resource {
        attr: TyAttr<QualifiedName>,
    },
    /// Internal-only type for builtin functions that accept any argument.
    ///
    /// Similar to TypeScript's `unknown` - any value can be passed where
    /// `BuiltinUnknown` is expected, but `BuiltinUnknown` cannot be used
    /// where a specific type is required (it doesn't "escape").
    ///
    /// Used in llm.baml for functions like:
    /// ```baml
    /// function render_prompt(function_name: string, args: map<string, unknown>) -> PromptAst
    /// ```
    ///
    /// NOTE: This is different from `Ty::Unknown` which means "type inference
    /// failed" and is used for error recovery.
    ///
    /// FUTURE: Replace with proper generics/mapped types once we design the
    /// syntax for type-safe prompt functions. The goal is something like:
    /// ```typescript
    /// // TypeScript equivalent of what we eventually want:
    /// type PromptMap = {
    ///   welcome: [user: string];
    ///   order: [orderId: number, expedited?: boolean];
    /// };
    ///
    /// function render_prompt<K extends keyof PromptMap>(f: K, ...params: PromptMap[K]) {
    ///   // ...
    /// }
    /// ```
    /// But we don't yet know the BAML syntax for this. `BuiltinUnknown` is the
    /// interim solution that allows builtins to type-check while we design
    /// the proper generic system.
    BuiltinUnknown {
        attr: TyAttr<QualifiedName>,
    },

    /// Watch accessor type: represents `x.$watch` on a watched variable.
    /// Contains the inner type being watched for method resolution.
    WatchAccessor(Box<Ty>, TyAttr<QualifiedName>),

    /// The bottom type (uninhabited). A `throw` expression or a function that
    /// always diverges produces `Never`. It is a subtype of every type:
    /// `Never <: T` for all `T`, and `never | T = T`.
    Never {
        attr: TyAttr<QualifiedName>,
    },

    /// Meta-type — the type of type values at the TIR level.
    /// Matches `TypePattern::Type` in builtin signatures.
    Type {
        attr: TyAttr<QualifiedName>,
    },
}

impl Ty {
    /// Check if this type is an error type.
    pub fn is_error(&self) -> bool {
        matches!(self, Ty::Error { .. })
    }

    /// Check if this type is unknown.
    pub fn is_unknown(&self) -> bool {
        matches!(self, Ty::Unknown { .. })
    }

    /// Check if this type is void.
    pub fn is_void(&self) -> bool {
        matches!(self, Ty::Void { .. })
    }

    /// Check if this type is uninhabited (has no possible values).
    ///
    /// An empty match on an uninhabited type is actually correct and exhaustive—there are
    /// no cases to handle because there are no possible values.
    ///
    /// Currently handled cases:
    /// - `Ty::Unknown` and `Ty::Error`: Treated as uninhabited for error recovery
    ///   (we don't want to emit additional errors when type inference already failed)
    /// - `Ty::Union(vec![])`: An empty union has no members, so no values
    ///
    /// Possible future cases to consider:
    /// - Zero-variant enums: `Ty::Enum(name)` where the enum has no variants defined
    ///   (would require access to the enum variants map to check variant count)
    /// - Recursive uninhabited types: e.g., `List<Never>` is inhabited (by empty list),
    ///   but some recursive structures could be uninhabited
    /// - Intersection of incompatible types (if the type system supports intersections)
    pub fn is_uninhabited(&self) -> bool {
        match self {
            // Error recovery: don't emit additional errors when type inference failed
            Ty::Unknown { .. } | Ty::Error { .. } => true,
            Ty::Never { .. } => true,
            // Empty union has no members, therefore no possible values
            Ty::Union(types, _) => types.is_empty(),
            // All other types are inhabited
            // TODO(exhaustiveness): Check for zero-variant enums. This requires access
            // to enum variants map. Currently only empty unions are detected.
            _ => false,
        }
    }

    /// Check if this type is a primitive type.
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            Ty::Int { .. }
                | Ty::Float { .. }
                | Ty::String { .. }
                | Ty::Bool { .. }
                | Ty::Null { .. }
        )
    }

    /// Check if this type is a media type.
    #[allow(dead_code)]
    pub fn is_media(&self) -> bool {
        matches!(self, Ty::Media(..))
    }

    /// Replace the `TyAttr` on this type, returning a new Ty with the given attr.
    /// Short-circuits if the attr is default (avoids unnecessary cloning).
    #[must_use]
    pub fn with_attr(self, attr: TyAttr<QualifiedName>) -> Self {
        if attr.is_default() {
            return self;
        }
        match self {
            Ty::Int { .. } => Ty::Int { attr },
            Ty::Float { .. } => Ty::Float { attr },
            Ty::String { .. } => Ty::String { attr },
            Ty::Bool { .. } => Ty::Bool { attr },
            Ty::Null { .. } => Ty::Null { attr },
            Ty::Media(kind, _) => Ty::Media(kind, attr),
            Ty::Literal(lit, _) => Ty::Literal(lit, attr),
            Ty::Class(qn, _) => Ty::Class(qn, attr),
            Ty::Enum(qn, _) => Ty::Enum(qn, attr),
            Ty::TypeAlias(qn, _) => Ty::TypeAlias(qn, attr),
            Ty::Optional(inner, _) => Ty::Optional(inner, attr),
            Ty::List(inner, _) => Ty::List(inner, attr),
            Ty::Map { key, value, .. } => Ty::Map { key, value, attr },
            Ty::Union(types, _) => Ty::Union(types, attr),
            Ty::Function { params, ret, .. } => Ty::Function { params, ret, attr },
            Ty::Unknown { .. } => Ty::Unknown { attr },
            Ty::Error { .. } => Ty::Error { attr },
            Ty::Void { .. } => Ty::Void { attr },
            Ty::Resource { .. } => Ty::Resource { attr },
            Ty::BuiltinUnknown { .. } => Ty::BuiltinUnknown { attr },
            Ty::WatchAccessor(inner, _) => Ty::WatchAccessor(inner, attr),
            Ty::Never { .. } => Ty::Never { attr },
            Ty::Type { .. } => Ty::Type { attr },
        }
    }

    /// Make this type optional.
    // The wrapper gets default attr because SAP attrs describe the wrapped type itself,
    // not the container. The inner type's attr is preserved on the inner type.
    #[must_use]
    #[allow(dead_code)]
    pub fn into_optional(self) -> Self {
        Ty::Optional(Box::new(self), TyAttr::default())
    }

    /// Make a list of this type.
    #[must_use]
    #[allow(dead_code)]
    pub fn into_list(self) -> Self {
        Ty::List(Box::new(self), TyAttr::default())
    }

    /// Get the element type if this is a list type.
    #[allow(dead_code)]
    pub fn list_element(&self) -> Option<&Ty> {
        match self {
            Ty::List(inner, _) => Some(inner),
            _ => None,
        }
    }

    /// Get the key and value types if this is a map type.
    #[allow(dead_code)]
    pub fn map_types(&self) -> Option<(&Ty, &Ty)> {
        match self {
            Ty::Map { key, value, .. } => Some((key, value)),
            _ => None,
        }
    }

    /// Unwrap optional type.
    #[allow(dead_code)]
    pub fn unwrap_optional(&self) -> &Ty {
        match self {
            Ty::Optional(inner, _) => inner,
            _ => self,
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Int { .. } => write!(f, "int"),
            Ty::Float { .. } => write!(f, "float"),
            Ty::String { .. } => write!(f, "string"),
            Ty::Bool { .. } => write!(f, "bool"),
            Ty::Null { .. } => write!(f, "null"),
            Ty::Media(kind, _) => write!(f, "{kind}"),
            Ty::Literal(val, _) => write!(f, "{val}"),
            Ty::Class(fqn, _) => write!(f, "{fqn}"),
            Ty::Enum(fqn, _) => write!(f, "{fqn}"),
            Ty::TypeAlias(fqn, _) => write!(f, "{fqn}"),
            Ty::Optional(inner, _) => write!(f, "{inner}?"),
            Ty::List(inner, _) => write!(f, "{inner}[]"),
            Ty::Map { key, value, .. } => write!(f, "map<{key}, {value}>"),
            Ty::Union(types, _) => {
                let parts: Vec<std::string::String> =
                    types.iter().map(std::string::ToString::to_string).collect();
                write!(f, "{}", parts.join(" | "))
            }
            Ty::Function { params, ret, .. } => {
                let param_strs: Vec<std::string::String> = params
                    .iter()
                    .map(|(name, ty)| {
                        if let Some(n) = name {
                            format!("{n}: {ty}")
                        } else {
                            ty.to_string()
                        }
                    })
                    .collect();
                write!(f, "({}) -> {}", param_strs.join(", "), ret)
            }
            Ty::Unknown { .. } => write!(f, "unknown"),
            Ty::Error { .. } => write!(f, "error"),
            Ty::Void { .. } => write!(f, "void"),
            Ty::Resource { .. } => write!(f, "resource"),
            Ty::BuiltinUnknown { .. } => write!(f, "unknown"),
            Ty::WatchAccessor(inner, _) => write!(f, "{inner}.$watch"),
            Ty::Never { .. } => write!(f, "never"),
            Ty::Type { .. } => write!(f, "type"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: Subtyping tests are now in normalize.rs which handles
    // type alias resolution and equirecursive subtyping.

    fn d() -> TyAttr<QualifiedName> {
        TyAttr::default()
    }

    #[test]
    fn test_display() {
        assert_eq!(Ty::Int { attr: d() }.to_string(), "int");
        assert_eq!(
            Ty::Optional(Box::new(Ty::String { attr: d() }), d()).to_string(),
            "string?"
        );
        assert_eq!(
            Ty::List(Box::new(Ty::Int { attr: d() }), d()).to_string(),
            "int[]"
        );
        assert_eq!(
            Ty::Union(vec![Ty::Int { attr: d() }, Ty::String { attr: d() }], d()).to_string(),
            "int | string"
        );
    }

    #[test]
    fn test_is_uninhabited() {
        // Unknown and Error are treated as uninhabited for error recovery
        assert!(Ty::Unknown { attr: d() }.is_uninhabited());
        assert!(Ty::Error { attr: d() }.is_uninhabited());

        // Empty union is uninhabited (no possible values)
        assert!(Ty::Union(vec![], d()).is_uninhabited());

        // Non-empty union is inhabited
        assert!(!Ty::Union(vec![Ty::Int { attr: d() }], d()).is_uninhabited());
        assert!(
            !Ty::Union(vec![Ty::Int { attr: d() }, Ty::String { attr: d() }], d()).is_uninhabited()
        );

        // Regular types are inhabited
        assert!(!Ty::Int { attr: d() }.is_uninhabited());
        assert!(!Ty::String { attr: d() }.is_uninhabited());
        assert!(!Ty::Bool { attr: d() }.is_uninhabited());
        assert!(!Ty::Null { attr: d() }.is_uninhabited());
        assert!(!Ty::Void { attr: d() }.is_uninhabited());
        assert!(!Ty::List(Box::new(Ty::Int { attr: d() }), d()).is_uninhabited());
        assert!(!Ty::Optional(Box::new(Ty::Int { attr: d() }), d()).is_uninhabited());
    }
}
