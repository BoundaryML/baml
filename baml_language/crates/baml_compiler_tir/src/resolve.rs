//! Name resolution for values and methods.
//!
//! This module provides resolution for value paths (variables, functions, enum variants)
//! and method calls. Type resolution is handled separately during type lowering.
//!
//! # Transient Resolution Types
//!
//! The types in this module (`ResolvedValue`, `ResolvedMethod`) are **transient** -
//! they are used during type inference to determine what a path refers to and what
//! type it has, but they are NOT stored in the final typed IR. After resolution,
//! we extract the type information and discard the resolution result.
//!
//! If we need resolution information for IDE features (go-to-definition, find-references)
//! in the future, we could store these in `InferenceResult` keyed by expression ID.
//!
//! # Value Resolution
//!
//! Value paths like `user`, `Status.Active`, or `baml.deep_copy` are resolved
//! during type inference using the current scope context.
//!
//! Resolution order:
//! 1. Local variables (innermost scope first)
//! 2. Function parameters
//! 3. Project-level functions
//! 4. Enum variants (for two-segment paths)
//! 5. Builtin functions
//!
//! # Method Resolution
//!
//! Method resolution is type-directed - it requires knowing the receiver's type.
//! This happens after inferring the receiver type during expression type inference.

use baml_base::Name;
use baml_compiler_hir::FullyQualifiedName;

/// Result of resolving a value path.
///
/// Value paths are identifiers used in expressions: variable references,
/// function calls, enum variant access, and builtin function calls.
///
/// This type is used both during type inference and stored for IDE features
/// (go-to-definition, find-references).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedValue {
    /// Local variable (from let binding or function parameter).
    ///
    /// Locals don't have FQNs - they're ephemeral within their scope.
    Local {
        /// The local variable name.
        name: Name,
        /// Where this local was defined (for go-to-definition).
        definition_site: Option<crate::DefinitionSite>,
    },

    /// User-defined function.
    ///
    /// Functions are project-level and have a fully-qualified name.
    Function(FullyQualifiedName),

    /// User-defined class.
    Class(FullyQualifiedName),

    /// User-defined enum.
    Enum(FullyQualifiedName),

    /// Type alias.
    TypeAlias(FullyQualifiedName),

    /// Enum variant (e.g., `Status.Active`).
    ///
    /// The enum and variant are both identified by their names.
    EnumVariant {
        /// The FQN of the enum type.
        enum_fqn: FullyQualifiedName,
        /// The variant name within the enum.
        variant: Name,
    },

    /// Class field access.
    Field {
        /// The class's FQN.
        class_fqn: FullyQualifiedName,
        /// The field name.
        field: Name,
    },

    /// Builtin free function (e.g., `env.get`, `baml.deep_copy`).
    ///
    /// These are functions provided by the runtime, not user-defined.
    /// Path is normalized (e.g., "`baaml.deep_copy`" becomes "`baml.deep_copy`").
    BuiltinFunction {
        /// The normalized builtin path (e.g., "env.get", "`baml.deep_copy`").
        path: String,
    },

    /// Module item path (e.g., `baml.HttpMethod.Get`).
    ///
    /// Used for accessing items through module paths.
    ModuleItem {
        /// The module path segments.
        module_path: Vec<Name>,
        /// The final item name.
        item_name: Name,
    },

    /// Method on a type (e.g., `image.from_url`).
    ///
    /// Used when the first segment is a type name with associated methods.
    TypeMethod {
        /// The receiver type name.
        receiver_type: Name,
        /// The method name.
        method_name: Name,
    },

    /// Resolution failed.
    ///
    /// This indicates the path could not be resolved to any known entity.
    Unknown,
}

impl ResolvedValue {
    /// Check if this resolution failed.
    pub fn is_unknown(&self) -> bool {
        matches!(self, ResolvedValue::Unknown)
    }

    /// Get the local variable info if this is a local.
    pub fn as_local(&self) -> Option<(&Name, Option<crate::DefinitionSite>)> {
        match self {
            ResolvedValue::Local {
                name,
                definition_site,
            } => Some((name, *definition_site)),
            _ => None,
        }
    }

    /// Get the function FQN if this is a function.
    pub fn as_function(&self) -> Option<&FullyQualifiedName> {
        match self {
            ResolvedValue::Function(fqn) => Some(fqn),
            _ => None,
        }
    }

    /// Get the enum variant info if this is an enum variant.
    pub fn as_enum_variant(&self) -> Option<(&FullyQualifiedName, &Name)> {
        match self {
            ResolvedValue::EnumVariant { enum_fqn, variant } => Some((enum_fqn, variant)),
            _ => None,
        }
    }
}

/// Result of resolving a method call on a known receiver type.
///
/// Method resolution is type-directed: we need to know the receiver's type
/// before we can determine what `.method()` refers to.
///
/// **This is a transient type** - used during type inference to determine
/// what method is being called, then discarded. The method's return type
/// is extracted and used, but the `ResolvedMethod` itself is not stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedMethod {
    /// Builtin method (e.g., `.length()`, `.push()`).
    ///
    /// These are methods provided by the runtime for built-in types.
    Builtin {
        /// The builtin's path (e.g., "baml.Array.length").
        path: String,
    },

    /// User-defined method (future: when we have impl blocks).
    UserDefined {
        /// The FQN of the impl block (future).
        impl_fqn: FullyQualifiedName,
        /// The method name.
        method_name: Name,
    },

    /// Resolution failed.
    Unknown,
}

impl ResolvedMethod {
    /// Check if this resolution failed.
    pub fn is_unknown(&self) -> bool {
        matches!(self, ResolvedMethod::Unknown)
    }

    /// Get the builtin path if this is a builtin method.
    pub fn as_builtin(&self) -> Option<&str> {
        match self {
            ResolvedMethod::Builtin { path } => Some(path),
            _ => None,
        }
    }
}

use std::collections::HashMap;

use baml_compiler_hir::ExprId;

use crate::{Ty, builtins};

/// Resolution map for all expressions in a function body.
pub type ResolutionMap = HashMap<ExprId, ResolvedValue>;

/// Resolve a method call on a known receiver type.
///
/// This is the main entry point for method resolution. Given a receiver type
/// and a method name, it determines what method is being called.
///
/// **This is a transient operation** - the result is used during type inference
/// to determine the method's type signature, then discarded.
///
/// # Resolution Order
/// 1. Builtin methods (e.g., `.length()` on arrays, `.trim()` on strings)
/// 2. Future: User-defined methods from impl blocks
///
/// # Example
/// ```ignore
/// let arr: int[] = [1, 2, 3];
/// arr.length(); // resolves to baml.Array.length builtin
/// ```
pub fn resolve_method(receiver_ty: &Ty, method_name: &str) -> ResolvedMethod {
    // Try builtin methods first
    if let Some((def, _bindings)) = builtins::lookup_method(receiver_ty, method_name) {
        return ResolvedMethod::Builtin {
            path: def.path.to_string(),
        };
    }

    // Future: check user-defined methods from impl blocks

    ResolvedMethod::Unknown
}
