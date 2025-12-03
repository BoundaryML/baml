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

    /// Literal types in unions.
    StringLiteral(String),
    IntLiteral(i64),
    /// Float literal stored as string to avoid f64's lack of Eq/Hash.
    FloatLiteral(String),

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
    pub fn from_ast(type_expr: &baml_syntax::ast::TypeExpr) -> Self {
        use rowan::ast::AstNode;

        // Get the text content of the type expression
        let text = type_expr.syntax().text().to_string().trim().to_string();

        // Check for array type (e.g., "int[]")
        if text.ends_with("[]") {
            let inner_text = &text[..text.len() - 2];
            let inner = Self::from_type_name(inner_text);
            return TypeRef::List(Box::new(inner));
        }

        // Check for optional type (e.g., "int?")
        if text.ends_with('?') {
            let inner_text = &text[..text.len() - 1];
            let inner = Self::from_type_name(inner_text);
            return TypeRef::Optional(Box::new(inner));
        }

        Self::from_type_name(&text)
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
