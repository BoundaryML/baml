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

mod attr;
mod convert;
mod defs;
pub mod typetag;
pub use attr::*;
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
///
/// Every variant carries an `attr: TyAttr` (or trailing `TyAttr` for tuple
/// variants) that holds SAP streaming annotations. All existing code uses
/// `TyAttr::default()` — only Phase 3 (stream type generation) will populate
/// non-default values.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    // --- Core: used by all VIR+ stages ---
    Int {
        attr: TyAttr,
    },
    Float {
        attr: TyAttr,
    },
    String {
        attr: TyAttr,
    },
    Bool {
        attr: TyAttr,
    },
    Null {
        attr: TyAttr,
    },
    Media(MediaKind, TyAttr),
    Literal(Literal, TyAttr),
    Class(TypeName, TyAttr),
    Enum(TypeName, TyAttr),
    Optional(Box<Ty>, TyAttr),
    List(Box<Ty>, TyAttr),
    Map {
        key: Box<Ty>,
        value: Box<Ty>,
        attr: TyAttr,
    },
    Union(Vec<Ty>, TyAttr),

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
    Opaque(TypeName, TyAttr),

    // --- Compiler-specific: present in VIR/MIR, absent at runtime ---
    /// Only recursive aliases survive lower_ty; non-recursive are expanded.
    TypeAlias(TypeName, TyAttr),
    /// Function/arrow type: `(T1, T2, ...) -> R`
    Function {
        params: Vec<Ty>,
        ret: Box<Ty>,
        attr: TyAttr,
    },
    /// Void type — the type of effectful expressions (was VIR `Unit`).
    /// Also used for diverging expressions (return, break, continue) since
    /// MIR encodes divergence via control flow terminators, not the type.
    Void {
        attr: TyAttr,
    },
    /// Watch accessor type: represents `x.$watch` on a watched variable.
    WatchAccessor(Box<Ty>, TyAttr),
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
    BuiltinUnknown {
        attr: TyAttr,
    },
}

// NOTE: `Unknown`, `Error`, and `Never` are intentionally excluded from this enum.
// - Unknown/Error are TIR-only error recovery types. They are mapped to `Null` during
//   TIR→baml_type conversion in `convert_tir_ty`. All real type checking happens in TIR
//   (which keeps its own Ty), so VIR+ stages don't need these for error recovery.
// - Never is a VIR-only bottom type for diverging expressions (return/break/continue).
//   MIR already collapsed Never→Void via control flow terminators. VIR lowering now
//   produces `Void` directly instead of `Never`.

impl Ty {
    // --- TyAttr accessor ---

    /// Replace the TyAttr on this type, returning a new Ty with the given attr.
    pub fn with_attr(self, attr: TyAttr) -> Ty {
        match self {
            Ty::Int { .. } => Ty::Int { attr },
            Ty::Float { .. } => Ty::Float { attr },
            Ty::String { .. } => Ty::String { attr },
            Ty::Bool { .. } => Ty::Bool { attr },
            Ty::Null { .. } => Ty::Null { attr },
            Ty::Void { .. } => Ty::Void { attr },
            Ty::BuiltinUnknown { .. } => Ty::BuiltinUnknown { attr },
            Ty::Media(kind, _) => Ty::Media(kind, attr),
            Ty::Literal(lit, _) => Ty::Literal(lit, attr),
            Ty::Class(tn, _) => Ty::Class(tn, attr),
            Ty::Enum(tn, _) => Ty::Enum(tn, attr),
            Ty::Optional(inner, _) => Ty::Optional(inner, attr),
            Ty::List(inner, _) => Ty::List(inner, attr),
            Ty::Map { key, value, .. } => Ty::Map { key, value, attr },
            Ty::Union(members, _) => Ty::Union(members, attr),
            Ty::Opaque(tn, _) => Ty::Opaque(tn, attr),
            Ty::TypeAlias(tn, _) => Ty::TypeAlias(tn, attr),
            Ty::Function { params, ret, .. } => Ty::Function { params, ret, attr },
            Ty::WatchAccessor(inner, _) => Ty::WatchAccessor(inner, attr),
        }
    }

    /// Get the TyAttr for this type.
    pub fn attr(&self) -> &TyAttr {
        match self {
            Ty::Int { attr }
            | Ty::Float { attr }
            | Ty::String { attr }
            | Ty::Bool { attr }
            | Ty::Null { attr }
            | Ty::Void { attr }
            | Ty::BuiltinUnknown { attr }
            | Ty::Map { attr, .. }
            | Ty::Function { attr, .. } => attr,
            Ty::Media(_, attr)
            | Ty::Literal(_, attr)
            | Ty::Class(_, attr)
            | Ty::Enum(_, attr)
            | Ty::Optional(_, attr)
            | Ty::List(_, attr)
            | Ty::Union(_, attr)
            | Ty::Opaque(_, attr)
            | Ty::TypeAlias(_, attr)
            | Ty::WatchAccessor(_, attr) => attr,
        }
    }

    // --- Opaque type constructors ---

    /// Helper to build a TypeName for a builtin opaque type.
    ///
    /// `qualified_name`: dotted path like `"baml.llm.Resource"` — the last
    /// segment becomes `name`, everything before it becomes `module_path`.
    ///
    /// `display`: the user-facing display string (may differ from the
    /// qualified name).
    fn opaque_builtin(qualified_name: &str, display: &str, attr: TyAttr) -> Self {
        let segments: Vec<&str> = qualified_name.split('.').collect();
        let name = Name::new(*segments.last().expect("qualified_name must be non-empty"));
        let module_path = segments[..segments.len() - 1]
            .iter()
            .map(Name::new)
            .collect();
        Ty::Opaque(
            TypeName {
                name,
                module_path,
                display_name: Name::new(display),
            },
            attr,
        )
    }

    /// Opaque resource handle type (file, socket, HTTP response body).
    /// NOTE: Uses TyAttr::default(). Callers with a source attr should use opaque_builtin() directly.
    pub fn resource() -> Self {
        Self::opaque_builtin("baml.llm.Resource", "baml.llm.Resource", TyAttr::default())
    }

    /// Opaque structured prompt tree type for LLM calls.
    /// NOTE: Uses TyAttr::default(). Callers with a source attr should use opaque_builtin() directly.
    pub fn prompt_ast() -> Self {
        Self::opaque_builtin(
            "baml.llm.PromptAst",
            "baml.llm.PromptAst",
            TyAttr::default(),
        )
    }

    /// Meta-type — a runtime value that wraps a `Ty`.
    /// NOTE: Uses TyAttr::default(). Callers with a source attr should use opaque_builtin() directly.
    pub fn type_type() -> Self {
        Self::opaque_builtin("baml.reflect.Type", "type", TyAttr::default())
    }

    /// Check if this is an opaque type with the given qualified name
    /// (e.g. `"baml.llm.PromptAst"`).
    pub fn is_opaque(&self, qualified_name: &str) -> bool {
        match self {
            Ty::Opaque(tn, _) => {
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
            Ty::Opaque(tn, _) => Some(tn),
            _ => None,
        }
    }

    /// Check if this is the void type.
    pub fn is_void(&self) -> bool {
        matches!(self, Ty::Void { .. })
    }

    /// Check if this is a primitive type (including literals of primitive types).
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            Ty::Int { .. }
                | Ty::Float { .. }
                | Ty::String { .. }
                | Ty::Bool { .. }
                | Ty::Null { .. }
                | Ty::Literal(..)
        )
    }

    /// Check if this type is a subtype of another.
    ///
    /// Returns true if `self` can be used where `other` is expected.
    /// Ported from VIR `ty.rs:93-140` with literal subtyping rules.
    ///
    /// Note: TyAttr does NOT affect subtyping. Two types with different
    /// attrs are not subtypes of each other (they're different types via
    /// PartialEq), but attr content isn't checked for subtype relationships.
    ///
    /// Note: Unknown/Error/Never handling is not needed here because:
    /// - Unknown/Error are mapped to Null during TIR→baml_type conversion
    /// - Never is mapped to Void during VIR lowering
    /// - All real type checking (where those variants matter) happens in TIR
    pub fn is_subtype_of(&self, other: &Ty) -> bool {
        // Same types are subtypes
        if self == other {
            return true;
        }

        // Any type is a subtype of BuiltinUnknown (it accepts everything)
        if matches!(other, Ty::BuiltinUnknown { .. }) {
            return true;
        }

        match (self, other) {
            // Literal types are subtypes of their corresponding primitives
            (Ty::Literal(Literal::Int(_), _), Ty::Int { .. }) => true,
            (Ty::Literal(Literal::Float(_), _), Ty::Float { .. }) => true,
            (Ty::Literal(Literal::String(_), _), Ty::String { .. }) => true,
            (Ty::Literal(Literal::Bool(_), _), Ty::Bool { .. }) => true,
            // Literal int widens to float
            (Ty::Literal(Literal::Int(_), _), Ty::Float { .. }) => true,

            // Null is a subtype of Optional<T>
            (Ty::Null { .. }, Ty::Optional(..)) => true,

            // T is a subtype of Optional<T>
            (inner, Ty::Optional(opt_inner, _)) => inner.is_subtype_of(opt_inner),

            // T is a subtype of T | U (union containing T)
            (inner, Ty::Union(types, _)) => types.iter().any(|t| inner.is_subtype_of(t)),

            // Union<T1, T2> is a subtype of U if all Ti are subtypes of U
            (Ty::Union(types, _), other) => types.iter().all(|t| t.is_subtype_of(other)),

            // List covariance
            (Ty::List(inner1, _), Ty::List(inner2, _)) => inner1.is_subtype_of(inner2),

            // Map covariance in value (key invariant)
            (
                Ty::Map {
                    key: k1, value: v1, ..
                },
                Ty::Map {
                    key: k2, value: v2, ..
                },
            ) => k1 == k2 && v1.is_subtype_of(v2),

            // Int is a subtype of Float (numeric widening)
            (Ty::Int { .. }, Ty::Float { .. }) => true,

            _ => false,
        }
    }

    /// Returns true if this type is a compiler-only variant that should
    /// never appear at runtime.
    pub fn is_compiler_only(&self) -> bool {
        matches!(
            self,
            Ty::TypeAlias(..)
                | Ty::Function { .. }
                | Ty::Void { .. }
                | Ty::WatchAccessor(..)
                | Ty::BuiltinUnknown { .. }
        )
    }

    /// Recursively walk this type tree and return an error if any compiler-only
    /// variants are found.
    pub fn validate_runtime(&self) -> Result<(), String> {
        match self {
            Ty::TypeAlias(tn, _) => Err(format!(
                "TypeAlias '{}' should be expanded before reaching runtime",
                tn.display_name
            )),
            Ty::Void { .. } => Err("Void type should not reach runtime".to_string()),
            Ty::WatchAccessor(inner, _) => inner.validate_runtime(),
            Ty::BuiltinUnknown { .. } => Ok(()),
            // Recurse into containers
            Ty::Optional(inner, _) => inner.validate_runtime(),
            Ty::List(inner, _) => inner.validate_runtime(),
            Ty::Map { key, value, .. } => {
                key.validate_runtime()?;
                value.validate_runtime()
            }
            Ty::Union(members, _) => {
                for m in members {
                    m.validate_runtime()?;
                }
                Ok(())
            }
            // All other variants are fine at runtime
            Ty::Function { params, ret, .. } => {
                for p in params {
                    p.validate_runtime()?;
                }
                ret.validate_runtime()
            }
            Ty::Int { .. }
            | Ty::Float { .. }
            | Ty::String { .. }
            | Ty::Bool { .. }
            | Ty::Null { .. }
            | Ty::Media(..)
            | Ty::Literal(..)
            | Ty::Class(..)
            | Ty::Enum(..)
            | Ty::Opaque(..) => Ok(()),
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
            Ty::Literal(lit, _) => match lit {
                Literal::Int(i) => write!(f, "{i}"),
                Literal::Float(s) => write!(f, "{s}"),
                Literal::String(s) => write!(f, "\"{s}\""),
                Literal::Bool(b) => write!(f, "{b}"),
            },
            Ty::Class(tn, _) => write!(f, "{tn}"),
            Ty::Enum(tn, _) => write!(f, "{tn}"),
            Ty::Opaque(tn, _) => write!(f, "{tn}"),
            Ty::TypeAlias(tn, _) => write!(f, "{tn}"),
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
                    .map(std::string::ToString::to_string)
                    .collect();
                write!(f, "({}) -> {}", param_strs.join(", "), ret)
            }
            Ty::Void { .. } => write!(f, "void"),
            Ty::WatchAccessor(inner, _) => write!(f, "{inner}.$watch"),
            Ty::BuiltinUnknown { .. } => write!(f, "unknown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shorthand helpers for tests — all use default TyAttr.
    fn ty_int() -> Ty {
        Ty::Int {
            attr: TyAttr::default(),
        }
    }
    fn ty_float() -> Ty {
        Ty::Float {
            attr: TyAttr::default(),
        }
    }
    fn ty_string() -> Ty {
        Ty::String {
            attr: TyAttr::default(),
        }
    }
    fn ty_bool() -> Ty {
        Ty::Bool {
            attr: TyAttr::default(),
        }
    }
    fn ty_null() -> Ty {
        Ty::Null {
            attr: TyAttr::default(),
        }
    }

    #[test]
    fn test_literal_int_subtype_of_int() {
        let lit_42 = Ty::Literal(Literal::Int(42), TyAttr::default());
        assert!(lit_42.is_subtype_of(&ty_int()));
    }

    #[test]
    fn test_literal_float_subtype_of_float() {
        let lit_3_14 = Ty::Literal(Literal::Float("3.14".to_string()), TyAttr::default());
        assert!(lit_3_14.is_subtype_of(&ty_float()));
    }

    #[test]
    fn test_literal_int_widens_to_float() {
        let lit_42 = Ty::Literal(Literal::Int(42), TyAttr::default());
        assert!(lit_42.is_subtype_of(&ty_float()));
    }

    #[test]
    fn test_literal_string_subtype_of_string() {
        let lit_hello = Ty::Literal(Literal::String("hello".to_string()), TyAttr::default());
        assert!(lit_hello.is_subtype_of(&ty_string()));
    }

    #[test]
    fn test_literal_bool_subtype_of_bool() {
        let lit_true = Ty::Literal(Literal::Bool(true), TyAttr::default());
        assert!(lit_true.is_subtype_of(&ty_bool()));
    }

    #[test]
    fn test_literal_in_union() {
        let lit_42 = Ty::Literal(Literal::Int(42), TyAttr::default());
        let union_type = Ty::Union(vec![ty_string(), ty_int()], TyAttr::default());
        assert!(lit_42.is_subtype_of(&union_type));
    }

    #[test]
    fn test_literal_float_in_union() {
        let lit_3_14 = Ty::Literal(Literal::Float("3.14".to_string()), TyAttr::default());
        let union_type = Ty::Union(vec![ty_string(), ty_float()], TyAttr::default());
        assert!(lit_3_14.is_subtype_of(&union_type));
    }

    #[test]
    fn test_literal_in_optional() {
        let lit_42 = Ty::Literal(Literal::Int(42), TyAttr::default());
        let opt_int = Ty::Optional(Box::new(ty_int()), TyAttr::default());
        assert!(lit_42.is_subtype_of(&opt_int));
    }

    #[test]
    fn test_null_subtype_of_optional() {
        let opt_string = Ty::Optional(Box::new(ty_string()), TyAttr::default());
        assert!(ty_null().is_subtype_of(&opt_string));
    }

    #[test]
    fn test_int_subtype_of_float() {
        assert!(ty_int().is_subtype_of(&ty_float()));
    }

    #[test]
    fn test_list_covariance() {
        let list_lit = Ty::List(
            Box::new(Ty::Literal(Literal::Int(42), TyAttr::default())),
            TyAttr::default(),
        );
        let list_int = Ty::List(Box::new(ty_int()), TyAttr::default());
        assert!(list_lit.is_subtype_of(&list_int));
    }

    #[test]
    fn test_validate_runtime_accepts_core_types() {
        assert!(ty_int().validate_runtime().is_ok());
        assert!(ty_float().validate_runtime().is_ok());
        assert!(ty_string().validate_runtime().is_ok());
        assert!(
            Ty::Literal(Literal::Float("3.14".to_string()), TyAttr::default())
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
        assert_eq!(ty_int().as_opaque(), None);
    }

    #[test]
    fn test_validate_runtime_rejects_compiler_types() {
        assert!(
            (Ty::Void {
                attr: TyAttr::default()
            })
            .validate_runtime()
            .is_err()
        );
        assert!(
            Ty::TypeAlias(TypeName::local(Name::new("MyAlias")), TyAttr::default())
                .validate_runtime()
                .is_err()
        );
    }
}
