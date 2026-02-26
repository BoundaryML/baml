//! Unified type system for BAML.
//!
//! `baml_type::Ty` is the canonical type representation used from VIR through runtime.
//! TIR keeps its own `Ty` with `QualifiedName` and `TypeAlias` — this crate
//! provides the single conversion point from TIR types.

use std::{
    fmt,
    hash::{Hash, Hasher},
};

// Re-export core baml_base types so downstream crates can depend on baml_type
// instead of baml_base directly.
pub use baml_base::{Literal, MediaKind, Name, Span};

mod convert;
mod defs;
pub mod typetag;
pub use convert::{convert_tir_ty, fqn_to_type_name, sanitize_for_runtime};
pub use defs::*;

/// A lightweight name type for class/enum/type-alias references.
///
/// Replaces both `QualifiedName` (VIR+) and plain `String` keys.
/// `display_name` is pre-computed from the source FQN and does NOT participate
/// in equality/hashing — it's a cache for display purposes.
#[derive(Debug, Clone)]
pub struct TypeName {
    /// Short name: "Response", "User"
    pub name: Name,
    /// Module path segments: empty for local types, ["http"] for baml.http.Response
    // TODO(perf): module_path is unused by all post-TIR consumers. Could be simplified
    // to just { name, display_name } in a follow-up to reduce TypeName from 72 to 48 bytes.
    pub module_path: Vec<Name>,
    /// Pre-computed display string: "baml.http.Response" for builtins, "User" for locals.
    /// Does NOT participate in PartialEq/Hash.
    pub display_name: Name,
}

impl TypeName {
    /// Create a TypeName for a local (non-namespaced) type.
    pub fn local(name: Name) -> Self {
        Self {
            display_name: name.clone(),
            name,
            module_path: vec![],
        }
    }
}

impl PartialEq for TypeName {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.module_path == other.module_path
    }
}

impl Eq for TypeName {}

impl Hash for TypeName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.module_path.hash(state);
    }
}

impl fmt::Display for TypeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name)
    }
}

/// The unified type representation for BAML, used from VIR through runtime.
///
/// Contains both core runtime variants and compiler-only variants.
/// Runtime code should use `unreachable!()` for compiler-only variants.
/// Runtime code should call `validate_runtime()` to catch any that leak.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    // --- Core: used by all VIR+ stages ---
    Int,
    Float,
    String,
    Bool,
    Null,
    /// The bottom type — uninhabited.
    /// Used in streaming type expansion. `T | never → T`.
    /// A field whose stream type is `never` is omitted entirely.
    Never,
    Media(MediaKind),
    Literal(Literal),
    Class(TypeName),
    Enum(TypeName),
    Optional(Box<Ty>),
    List(Box<Ty>),
    Map {
        key: Box<Ty>,
        value: Box<Ty>,
    },
    Union(Vec<Ty>),

    // --- Runtime-only: present at runtime, not in user-facing type syntax ---
    /// Opaque runtime-only type, identified by its qualified name.
    ///
    /// Used for types that the type system treats generically (nominal equality,
    /// no structural decomposition, infinite for exhaustiveness) but whose
    /// *values* are concrete Rust types on the VM heap.
    ///
    /// Well-known opaque types:
    /// - `baml.llm.Resource` — file/socket/HTTP response handles
    /// - `baml.llm.PromptAst` — structured prompt trees for LLM calls
    /// - `type` — meta-type wrapping a `Ty` for reflection
    ///
    /// Use the convenience constructors `Ty::resource()`, `Ty::prompt_ast()`,
    /// `Ty::type_type()` instead of constructing directly.
    Opaque(TypeName),

    // --- Compiler-specific: present in VIR/MIR, absent at runtime ---
    /// Only recursive aliases survive lower_ty; non-recursive are expanded.
    TypeAlias(TypeName),
    /// Function/arrow type: `(T1, T2, ...) -> R`
    Function {
        params: Vec<Ty>,
        ret: Box<Ty>,
    },
    /// Void type — the type of effectful expressions (was VIR `Unit`).
    /// Also used for diverging expressions (return, break, continue) since
    /// MIR encodes divergence via control flow terminators, not the type.
    Void,
    /// Watch accessor type: represents `x.$watch` on a watched variable.
    WatchAccessor(Box<Ty>),
    /// Internal-only type for builtin functions that accept any argument.
    ///
    /// Similar to TypeScript's `unknown` - any value can be passed where
    /// `BuiltinUnknown` is expected, but `BuiltinUnknown` cannot be used
    /// where a specific type is required.
    ///
    /// Used in llm.baml for functions like:
    /// ```baml
    /// function render_prompt(function_name: string, args: map<string, unknown>) -> PromptAst
    /// ```
    ///
    /// This is a compiler-only variant that should never reach runtime.
    BuiltinUnknown,
}

// NOTE: `Unknown` and `Error` are intentionally excluded from this enum.
// They are TIR-only error recovery types, mapped to `Null` during TIR→baml_type
// conversion in `convert_tir_ty`. All real type checking happens in TIR (which
// keeps its own Ty), so VIR+ stages don't need these for error recovery.
//
// `Never` IS included — it's the bottom type used by the streaming type system
// (e.g., `@stream.starts_as(never)`, `@stream.type(never)`). It can appear in
// `ClassField.field_type` at runtime.

impl Ty {
    // --- Opaque type constructors ---

    /// Helper to build a TypeName for a builtin opaque type.
    ///
    /// `qualified_name`: dotted path like `"baml.llm.Resource"` — the last
    /// segment becomes `name`, everything before it becomes `module_path`.
    ///
    /// `display`: the user-facing display string (may differ from the
    /// qualified name).
    fn opaque_builtin(qualified_name: &str, display: &str) -> Self {
        let segments: Vec<&str> = qualified_name.split('.').collect();
        let name = Name::new(*segments.last().expect("qualified_name must be non-empty"));
        let module_path = segments[..segments.len() - 1]
            .iter()
            .map(Name::new)
            .collect();
        Ty::Opaque(TypeName {
            name,
            module_path,
            display_name: Name::new(display),
        })
    }

    /// Opaque resource handle type (file, socket, HTTP response body).
    pub fn resource() -> Self {
        Self::opaque_builtin("baml.llm.Resource", "baml.llm.Resource")
    }

    /// Opaque structured prompt tree type for LLM calls.
    pub fn prompt_ast() -> Self {
        Self::opaque_builtin("baml.llm.PromptAst", "baml.llm.PromptAst")
    }

    /// Meta-type — a runtime value that wraps a `Ty`.
    pub fn type_type() -> Self {
        Self::opaque_builtin("baml.reflect.Type", "type")
    }

    /// Check if this is an opaque type with the given qualified name
    /// (e.g. `"baml.llm.PromptAst"`).
    pub fn is_opaque(&self, qualified_name: &str) -> bool {
        match self {
            Ty::Opaque(tn) => {
                // Build "module.path.Name" and compare.
                let mut parts: Vec<&str> = tn.module_path.iter().map(|n| n.as_str()).collect();
                parts.push(tn.name.as_str());
                let full = parts.join(".");
                full == qualified_name
            }
            _ => false,
        }
    }

    /// If this is an opaque type, return its TypeName.
    pub fn as_opaque(&self) -> Option<&TypeName> {
        match self {
            Ty::Opaque(tn) => Some(tn),
            _ => None,
        }
    }

    /// Check if this is the void type.
    pub fn is_void(&self) -> bool {
        matches!(self, Ty::Void)
    }

    /// Check if this is a primitive type (including literals of primitive types).
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            Ty::Int | Ty::Float | Ty::String | Ty::Bool | Ty::Null | Ty::Literal(_)
        )
    }

    /// Check if this type is a subtype of another.
    ///
    /// Returns true if `self` can be used where `other` is expected.
    /// Ported from VIR `ty.rs:93-140` with literal subtyping rules.
    ///
    /// Note: Unknown/Error handling is not needed here because:
    /// - Unknown/Error are mapped to Null during TIR→baml_type conversion
    /// - All real type checking (where those variants matter) happens in TIR
    ///
    /// Never IS handled: it's the bottom type, subtype of everything.
    pub fn is_subtype_of(&self, other: &Ty) -> bool {
        // Same types are subtypes
        if self == other {
            return true;
        }

        // Never <: T (bottom type is subtype of everything)
        if matches!(self, Ty::Never) {
            return true;
        }

        // Any type is a subtype of BuiltinUnknown (it accepts everything)
        if matches!(other, Ty::BuiltinUnknown) {
            return true;
        }

        match (self, other) {
            // Literal types are subtypes of their corresponding primitives
            (Ty::Literal(Literal::Int(_)), Ty::Int) => true,
            (Ty::Literal(Literal::Float(_)), Ty::Float) => true,
            (Ty::Literal(Literal::String(_)), Ty::String) => true,
            (Ty::Literal(Literal::Bool(_)), Ty::Bool) => true,
            // Literal int widens to float
            (Ty::Literal(Literal::Int(_)), Ty::Float) => true,

            // Null is a subtype of Optional<T>
            (Ty::Null, Ty::Optional(_)) => true,

            // T is a subtype of Optional<T>
            (inner, Ty::Optional(opt_inner)) => inner.is_subtype_of(opt_inner),

            // T is a subtype of T | U (union containing T)
            (inner, Ty::Union(types)) => types.iter().any(|t| inner.is_subtype_of(t)),

            // Union<T1, T2> is a subtype of U if all Ti are subtypes of U
            (Ty::Union(types), other) => types.iter().all(|t| t.is_subtype_of(other)),

            // List covariance
            (Ty::List(inner1), Ty::List(inner2)) => inner1.is_subtype_of(inner2),

            // Map covariance in value (key invariant)
            (Ty::Map { key: k1, value: v1 }, Ty::Map { key: k2, value: v2 }) => {
                k1 == k2 && v1.is_subtype_of(v2)
            }

            // Int is a subtype of Float (numeric widening)
            (Ty::Int, Ty::Float) => true,

            _ => false,
        }
    }

    /// Returns true if this type is a compiler-only variant that should
    /// never appear at runtime.
    pub fn is_compiler_only(&self) -> bool {
        matches!(
            self,
            Ty::TypeAlias(_)
                | Ty::Function { .. }
                | Ty::Void
                | Ty::WatchAccessor(_)
                | Ty::BuiltinUnknown
        )
    }

    /// Recursively walk this type tree and return an error if any compiler-only
    /// variants are found.
    pub fn validate_runtime(&self) -> Result<(), String> {
        match self {
            Ty::TypeAlias(tn) => Err(format!(
                "TypeAlias '{}' should be expanded before reaching runtime",
                tn.display_name
            )),
            Ty::Void => Err("Void type should not reach runtime".to_string()),
            Ty::WatchAccessor(inner) => inner.validate_runtime(),
            Ty::BuiltinUnknown => Ok(()),
            // Recurse into containers
            Ty::Optional(inner) => inner.validate_runtime(),
            Ty::List(inner) => inner.validate_runtime(),
            Ty::Map { key, value } => {
                key.validate_runtime()?;
                value.validate_runtime()
            }
            Ty::Union(members) => {
                for m in members {
                    m.validate_runtime()?;
                }
                Ok(())
            }
            // All other variants are fine at runtime
            Ty::Function { params, ret } => {
                for p in params {
                    p.validate_runtime()?;
                }
                ret.validate_runtime()
            }
            Ty::Int
            | Ty::Float
            | Ty::String
            | Ty::Bool
            | Ty::Null
            | Ty::Never
            | Ty::Media(_)
            | Ty::Literal(_)
            | Ty::Class(_)
            | Ty::Enum(_)
            | Ty::Opaque(_) => Ok(()),
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Int => write!(f, "int"),
            Ty::Float => write!(f, "float"),
            Ty::String => write!(f, "string"),
            Ty::Bool => write!(f, "bool"),
            Ty::Null => write!(f, "null"),
            Ty::Media(kind) => write!(f, "{kind}"),
            Ty::Literal(lit) => match lit {
                Literal::Int(i) => write!(f, "{i}"),
                Literal::Float(s) => write!(f, "{s}"),
                Literal::String(s) => write!(f, "\"{s}\""),
                Literal::Bool(b) => write!(f, "{b}"),
            },
            Ty::Class(tn) => write!(f, "{tn}"),
            Ty::Enum(tn) => write!(f, "{tn}"),
            Ty::Opaque(tn) => write!(f, "{tn}"),
            Ty::TypeAlias(tn) => write!(f, "{tn}"),
            Ty::Optional(inner) => write!(f, "{inner}?"),
            Ty::List(inner) => write!(f, "{inner}[]"),
            Ty::Map { key, value } => write!(f, "map<{key}, {value}>"),
            Ty::Union(types) => {
                let parts: Vec<std::string::String> =
                    types.iter().map(std::string::ToString::to_string).collect();
                write!(f, "{}", parts.join(" | "))
            }
            Ty::Function { params, ret } => {
                let param_strs: Vec<std::string::String> = params
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect();
                write!(f, "({}) -> {}", param_strs.join(", "), ret)
            }
            Ty::Never => write!(f, "never"),
            Ty::Void => write!(f, "void"),
            Ty::WatchAccessor(inner) => write!(f, "{inner}.$watch"),
            Ty::BuiltinUnknown => write!(f, "unknown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literal_int_subtype_of_int() {
        let lit_42 = Ty::Literal(Literal::Int(42));
        assert!(lit_42.is_subtype_of(&Ty::Int));
    }

    #[test]
    fn test_literal_float_subtype_of_float() {
        let lit_3_14 = Ty::Literal(Literal::Float("3.14".to_string()));
        assert!(lit_3_14.is_subtype_of(&Ty::Float));
    }

    #[test]
    fn test_literal_int_widens_to_float() {
        let lit_42 = Ty::Literal(Literal::Int(42));
        assert!(lit_42.is_subtype_of(&Ty::Float));
    }

    #[test]
    fn test_literal_string_subtype_of_string() {
        let lit_hello = Ty::Literal(Literal::String("hello".to_string()));
        assert!(lit_hello.is_subtype_of(&Ty::String));
    }

    #[test]
    fn test_literal_bool_subtype_of_bool() {
        let lit_true = Ty::Literal(Literal::Bool(true));
        assert!(lit_true.is_subtype_of(&Ty::Bool));
    }

    #[test]
    fn test_literal_in_union() {
        let lit_42 = Ty::Literal(Literal::Int(42));
        let union_type = Ty::Union(vec![Ty::String, Ty::Int]);
        assert!(lit_42.is_subtype_of(&union_type));
    }

    #[test]
    fn test_literal_float_in_union() {
        let lit_3_14 = Ty::Literal(Literal::Float("3.14".to_string()));
        let union_type = Ty::Union(vec![Ty::String, Ty::Float]);
        assert!(lit_3_14.is_subtype_of(&union_type));
    }

    #[test]
    fn test_literal_in_optional() {
        let lit_42 = Ty::Literal(Literal::Int(42));
        let opt_int = Ty::Optional(Box::new(Ty::Int));
        assert!(lit_42.is_subtype_of(&opt_int));
    }

    #[test]
    fn test_null_subtype_of_optional() {
        let opt_string = Ty::Optional(Box::new(Ty::String));
        assert!(Ty::Null.is_subtype_of(&opt_string));
    }

    #[test]
    fn test_int_subtype_of_float() {
        assert!(Ty::Int.is_subtype_of(&Ty::Float));
    }

    #[test]
    fn test_list_covariance() {
        let list_lit = Ty::List(Box::new(Ty::Literal(Literal::Int(42))));
        let list_int = Ty::List(Box::new(Ty::Int));
        assert!(list_lit.is_subtype_of(&list_int));
    }

    #[test]
    fn test_validate_runtime_accepts_core_types() {
        assert!(Ty::Int.validate_runtime().is_ok());
        assert!(Ty::Float.validate_runtime().is_ok());
        assert!(Ty::String.validate_runtime().is_ok());
        assert!(
            Ty::Literal(Literal::Float("3.14".to_string()))
                .validate_runtime()
                .is_ok()
        );
    }

    #[test]
    fn test_validate_runtime_accepts_opaque_types() {
        assert!(Ty::resource().validate_runtime().is_ok());
        assert!(Ty::prompt_ast().validate_runtime().is_ok());
        assert!(Ty::type_type().validate_runtime().is_ok());
    }

    #[test]
    fn test_display_opaque_types() {
        assert_eq!(Ty::resource().to_string(), "baml.llm.Resource");
        assert_eq!(Ty::prompt_ast().to_string(), "baml.llm.PromptAst");
        assert_eq!(Ty::type_type().to_string(), "type");
    }

    #[test]
    fn test_opaque_helpers() {
        assert!(Ty::resource().is_opaque("baml.llm.Resource"));
        assert!(!Ty::resource().is_opaque("baml.reflect.Type"));
        assert_eq!(
            Ty::prompt_ast().as_opaque().map(|tn| tn.name.as_str()),
            Some("PromptAst"),
        );
        assert_eq!(Ty::Int.as_opaque(), None);
    }

    #[test]
    fn test_validate_runtime_rejects_compiler_types() {
        assert!(Ty::Void.validate_runtime().is_err());
        assert!(
            Ty::TypeAlias(TypeName::local(Name::new("MyAlias")))
                .validate_runtime()
                .is_err()
        );
    }
}
