//! Unresolved type references in the HIR.
//!
//! These are type references before name resolution.
//! `TypeRef` -> Ty happens during THIR construction.

use baml_base::Name;
use rowan::ast::AstNode;

use crate::path::Path;

/// A type reference before name resolution.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeRef {
    /// Named type (with path for future module support).
    /// Examples:
    ///   `Path::single("User`") -> User
    ///   `Path::new`(`["users", "User"]`) -> `users::User` (future)
    Path(Path),

    /// Primitive types (no resolution needed).
    Int,
    Float,
    String,
    Bool,
    Null,

    Media(baml_base::MediaKind),

    /// Type constructors.
    Optional(Box<TypeRef>),
    List(Box<TypeRef>),
    Map {
        key: Box<TypeRef>,
        value: Box<TypeRef>,
    },
    Union(Vec<TypeRef>),

    /// Literal types in unions.
    StringLiteral(String),
    IntLiteral(i64),
    /// Float literal stored as string to avoid f64's lack of Eq/Hash.
    FloatLiteral(String),
    /// Boolean literal for pattern matching (true/false as types).
    BoolLiteral(bool),

    /// Future: Generic type application.
    /// Example: Result<User, string>
    #[allow(dead_code)]
    Generic {
        base: Box<TypeRef>,
        args: Vec<TypeRef>,
    },

    /// Future: Type parameter reference.
    /// Example: T in `function<T>(x: T) -> T`
    #[allow(dead_code)]
    TypeParam(Name),

    /// Error sentinel.
    Error,

    /// Unknown/inferred.
    Unknown,
}

impl TypeRef {
    /// Create a simple named type reference.
    pub fn named(name: Name) -> Self {
        TypeRef::Path(Path::single(name))
    }

    /// Create an optional type.
    pub fn optional(inner: TypeRef) -> Self {
        TypeRef::Optional(Box::new(inner))
    }

    /// Create a list type.
    pub fn list(inner: TypeRef) -> Self {
        TypeRef::List(Box::new(inner))
    }

    /// Create a union type.
    pub fn union(types: Vec<TypeRef>) -> Self {
        TypeRef::Union(types)
    }

    /// Create a `TypeRef` from an AST `TypeExpr` node.
    ///
    /// This uses structured CST accessors to properly handle complex types including:
    /// - Primitives: int, string, bool, etc.
    /// - Named types: User, `MyClass`
    /// - Optional types: string?
    /// - List types: string[]
    /// - Union types: Success | Failure
    /// - String literal types: "user" | "assistant"
    /// - Parenthesized types: (int | string)[]
    /// - Generic types: map<K, V>
    pub fn from_ast(type_expr: &baml_compiler_syntax::ast::TypeExpr) -> Self {
        // Handle optional modifier (outermost)
        // For `int[]?`, optional wraps the array
        if type_expr.is_optional() {
            let inner = Self::from_ast_without_optional(type_expr);
            return TypeRef::Optional(Box::new(inner));
        }

        Self::from_ast_without_optional(type_expr)
    }

    /// Parse a `TypeExpr` assuming the optional modifier has been handled.
    fn from_ast_without_optional(type_expr: &baml_compiler_syntax::ast::TypeExpr) -> Self {
        // Handle union FIRST (top-level PIPE)
        // For `int[] | string[]`, this is a union of arrays, not an array of unions
        // Note: `(int | string)[]` has PIPE inside parens, so is_union() returns false
        if type_expr.is_union() {
            // Parse each union member using structured token/node accessors
            let member_parts = type_expr.union_member_parts();
            let members: Vec<TypeRef> = member_parts.iter().map(Self::from_union_member).collect();
            return TypeRef::Union(members);
        }

        // Handle array modifier
        // For `(int | string)[]`, array wraps the parenthesized union
        if type_expr.is_array() {
            let element = Self::from_ast_array_element(type_expr);
            return TypeRef::List(Box::new(element));
        }

        Self::from_ast_base(type_expr)
    }

    /// Get the element type for an array `TypeExpr`.
    ///
    /// Uses token-based `array_depth()` to handle nested arrays without string manipulation.
    fn from_ast_array_element(type_expr: &baml_compiler_syntax::ast::TypeExpr) -> Self {
        // For parenthesized arrays like `(int | string)[]`, the element is the inner TypeExpr
        if let Some(inner) = type_expr.inner_type_expr() {
            return Self::from_ast(&inner);
        }

        // For non-parenthesized arrays like `int[]`, `string[][]`, `"user"[]`:
        // Use array_depth() to count nesting levels and from_ast_base_type() for the base.
        //
        // For `int[][]`: depth=2, base=Int -> element is List(Int) i.e. `int[]`
        // For `int[]`: depth=1, base=Int -> element is Int
        let depth = type_expr.array_depth();
        let base = Self::from_ast_base_type(type_expr);

        // Wrap base type in (depth-1) List layers to get the element type
        let mut result = base;
        for _ in 0..depth.saturating_sub(1) {
            result = TypeRef::List(Box::new(result));
        }
        result
    }

    /// Parse the base type (no optional, array, or union modifiers).
    fn from_ast_base(type_expr: &baml_compiler_syntax::ast::TypeExpr) -> Self {
        // Handle parenthesized types like `(int | string)`
        if let Some(inner) = type_expr.inner_type_expr() {
            return Self::from_ast(&inner);
        }

        Self::from_ast_base_type(type_expr)
    }

    /// Parse a base type (no modifiers, not a union).
    fn from_ast_base_type(type_expr: &baml_compiler_syntax::ast::TypeExpr) -> Self {
        // Check for string literal types like `"user"`
        if let Some(s) = type_expr.string_literal() {
            return TypeRef::StringLiteral(s);
        }

        // Check for integer literal types like `200`
        if let Some(i) = type_expr.integer_literal() {
            return TypeRef::IntLiteral(i);
        }

        // Check for boolean literal types
        if let Some(b) = type_expr.bool_literal() {
            return TypeRef::BoolLiteral(b);
        }

        // Check for map type with type args
        if let Some(name) = type_expr.base_name() {
            if name == "map" {
                let args = type_expr.type_arg_exprs();
                if args.len() == 2 {
                    let key = Self::from_ast(&args[0]);
                    let value = Self::from_ast(&args[1]);
                    return TypeRef::Map {
                        key: Box::new(key),
                        value: Box::new(value),
                    };
                }
            }

            // Named type (primitive or user-defined)
            return Self::from_type_name(&name);
        }

        TypeRef::Unknown
    }

    /// Parse a union member from its structured parts (tokens and child nodes).
    ///
    /// Uses token kinds and child node kinds directly instead of string manipulation.
    fn from_union_member(parts: &baml_compiler_syntax::ast::UnionMemberParts) -> Self {
        // Check for parenthesized type first (e.g., `(int | string)` in `A | (int | string)`)
        if let Some(type_expr) = parts.type_expr() {
            let inner = Self::from_ast(&type_expr);
            // Apply array and optional modifiers from tokens
            return Self::apply_modifiers_from_parts(inner, parts);
        }

        // Check for string literal (e.g., `"user"` in `"user" | "admin"`)
        if let Some(s) = parts.string_literal() {
            let base = TypeRef::StringLiteral(s);
            return Self::apply_modifiers_from_parts(base, parts);
        }

        // Check for integer literal (e.g., `200` in `200 | 201`)
        if let Some(i) = parts.integer_literal() {
            let base = TypeRef::IntLiteral(i);
            return Self::apply_modifiers_from_parts(base, parts);
        }

        // Check for named/primitive type or map type (e.g., `int`, `User`, `map<K,V>`)
        if let Some(name) = parts.first_word() {
            // Check for map type with TYPE_ARGS
            if name == "map" {
                if let Some(type_args_node) = parts.type_args() {
                    let type_arg_exprs: Vec<_> = type_args_node
                        .children()
                        .filter(|n| n.kind() == baml_compiler_syntax::SyntaxKind::TYPE_EXPR)
                        .map(|n| baml_compiler_syntax::ast::TypeExpr::cast(n).unwrap())
                        .collect();

                    if type_arg_exprs.len() == 2 {
                        let key = Self::from_ast(&type_arg_exprs[0]);
                        let value = Self::from_ast(&type_arg_exprs[1]);
                        let base = TypeRef::Map {
                            key: Box::new(key),
                            value: Box::new(value),
                        };
                        return Self::apply_modifiers_from_parts(base, parts);
                    }
                }
            }

            // Check for boolean literals
            let base = match name {
                "true" => TypeRef::BoolLiteral(true),
                "false" => TypeRef::BoolLiteral(false),
                _ => Self::from_type_name(name),
            };
            return Self::apply_modifiers_from_parts(base, parts);
        }

        TypeRef::Unknown
    }

    /// Apply array and optional modifiers from `UnionMemberParts` to a base type.
    fn apply_modifiers_from_parts(
        base: Self,
        parts: &baml_compiler_syntax::ast::UnionMemberParts,
    ) -> Self {
        let array_depth = parts.array_depth();
        let is_optional = parts.is_optional();

        // Wrap in array layers
        let mut result = base;
        for _ in 0..array_depth {
            result = TypeRef::List(Box::new(result));
        }

        // Wrap in optional if needed
        if is_optional {
            result = TypeRef::Optional(Box::new(result));
        }

        result
    }

    /// Create a `TypeRef` from a type name string (primitive or user-defined).
    fn from_type_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "int" => TypeRef::Int,
            "float" => TypeRef::Float,
            "string" => TypeRef::String,
            "bool" => TypeRef::Bool,
            "null" => TypeRef::Null,
            "image" => TypeRef::Media(baml_base::MediaKind::Image),
            "audio" => TypeRef::Media(baml_base::MediaKind::Audio),
            "video" => TypeRef::Media(baml_base::MediaKind::Video),
            "pdf" => TypeRef::Media(baml_base::MediaKind::Pdf),
            _ => TypeRef::Path(Path::single(Name::new(name))),
        }
    }
}
