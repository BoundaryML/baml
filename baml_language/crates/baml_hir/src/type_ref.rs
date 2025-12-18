//! Unresolved type references in the HIR.
//!
//! These are type references before name resolution.
//! `TypeRef` -> Ty happens during THIR construction.

use baml_base::Name;

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
    Image,
    Audio,
    Video,
    Pdf,

    /// Type constructors.
    Optional(Box<TypeRef>),
    List(Box<TypeRef>),
    Map {
        key: Box<TypeRef>,
        value: Box<TypeRef>,
    },
    Union(Vec<TypeRef>),

    /// Literal types for exhaustiveness checking.
    ///
    /// From a type-theoretic perspective, singleton types require decidable
    /// equality to support pattern matching and exhaustiveness checking. Floats are
    /// intentionally excluded because floating-point equality is not decidable
    /// (NaN ≠ NaN, precision issues like 0.1 + 0.2 ≠ 0.3, etc.).
    StringLiteral(String),
    IntLiteral(i64),
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
    /// This properly handles complex types including:
    /// - Primitives: int, string, bool, etc.
    /// - Named types: User, MyClass
    /// - Optional types: string?
    /// - List types: string[]
    /// - Union types: Success | Failure
    /// - String literal types: "user" | "assistant"
    pub fn from_ast(type_expr: &baml_syntax::ast::TypeExpr) -> Self {
        use baml_syntax::SyntaxKind;
        use rowan::{NodeOrToken, ast::AstNode};

        let syntax = type_expr.syntax();

        // Collect all the parts of the type expression
        // For union types, we'll find PIPE tokens that separate the members
        let mut parts: Vec<String> = Vec::new();
        let mut current_part = String::new();
        let mut has_pipe = false;

        for child in syntax.children_with_tokens() {
            match child {
                NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::PIPE {
                        // This is a union separator - save the current part and start a new one
                        let trimmed = current_part.trim().to_string();
                        if !trimmed.is_empty() {
                            parts.push(trimmed);
                        }
                        current_part = String::new();
                        has_pipe = true;
                    } else {
                        // Append token text to current part
                        current_part.push_str(token.text());
                    }
                }
                NodeOrToken::Node(child_node) => {
                    // For nested nodes (like TYPE_ARGS), include their full text
                    current_part.push_str(&child_node.text().to_string());
                }
            }
        }

        // Don't forget the last part
        let trimmed = current_part.trim().to_string();
        if !trimmed.is_empty() {
            parts.push(trimmed);
        }

        // If we found pipes, this is a union type
        if has_pipe && parts.len() > 1 {
            let members: Vec<TypeRef> = parts.iter().map(|p| Self::from_type_text(p)).collect();
            return TypeRef::Union(members);
        }

        // Otherwise, lower as a single type
        let text = syntax.text().to_string();
        let text = text.trim();
        Self::from_type_text(text)
    }

    /// Create a `TypeRef` from a single type text (not a union).
    fn from_type_text(text: &str) -> Self {
        // Check for string literal types like "user" or "assistant"
        if (text.starts_with('"') && text.ends_with('"'))
            || (text.starts_with('\'') && text.ends_with('\''))
        {
            let inner = &text[1..text.len() - 1];
            return TypeRef::StringLiteral(inner.to_string());
        }

        // Check for array type (e.g., "int[]")
        if text.ends_with("[]") {
            let inner_text = &text[..text.len() - 2];
            let inner = Self::from_type_text(inner_text);
            return TypeRef::List(Box::new(inner));
        }

        // Check for optional type (e.g., "int?")
        if text.ends_with('?') {
            let inner_text = &text[..text.len() - 1];
            let inner = Self::from_type_text(inner_text);
            return TypeRef::Optional(Box::new(inner));
        }

        // Check for boolean literal types
        if text == "true" {
            return TypeRef::BoolLiteral(true);
        }
        if text == "false" {
            return TypeRef::BoolLiteral(false);
        }

        // Check for integer literal types (for exhaustiveness like 200 | 201)
        if let Ok(int_val) = text.parse::<i64>() {
            return TypeRef::IntLiteral(int_val);
        }

        Self::from_type_name(text)
    }

    /// Create a `TypeRef` from a type name string.
    fn from_type_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "int" => TypeRef::Int,
            "float" => TypeRef::Float,
            "string" => TypeRef::String,
            "bool" => TypeRef::Bool,
            "null" => TypeRef::Null,
            "image" => TypeRef::Image,
            "audio" => TypeRef::Audio,
            "video" => TypeRef::Video,
            "pdf" => TypeRef::Pdf,
            _ => TypeRef::Path(Path::single(Name::new(name))),
        }
    }
}
