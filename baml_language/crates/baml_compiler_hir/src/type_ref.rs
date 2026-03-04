//! Unresolved type references in the HIR.
//!
//! These are type references before name resolution.
//! `TypeRef` -> Ty happens during THIR construction.

use baml_base::{Name, TyAttr};
use baml_compiler_syntax::ast::TypePostFixModifier;
use rowan::ast::AstNode;

use crate::path::Path;

/// A parameter in a function type reference.
///
/// Parameter names are optional and for documentation only - they do not
/// affect type equality or type checking.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionTypeParamRef {
    /// Optional parameter name (documentation only).
    pub name: Option<Name>,
    /// The parameter type.
    pub ty: TypeRef,
}

impl FunctionTypeParamRef {
    /// Create a new function type parameter with a name.
    pub fn named(name: Name, ty: TypeRef) -> Self {
        Self {
            name: Some(name),
            ty,
        }
    }

    /// Create a new function type parameter without a name.
    pub fn unnamed(ty: TypeRef) -> Self {
        Self { name: None, ty }
    }
}

/// A type reference before name resolution — with type-level attributes on every variant.
///
/// Every variant carries a `TyAttr` (or trailing `TyAttr` for tuple variants),
/// matching the convention used by `baml_type::Ty` and `baml_compiler_tir::Ty`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeRef {
    // --- Named types ---
    /// Named type (with path for future module support).
    /// Examples:
    ///   `Path::single("User")` -> User
    ///   `Path::new(["users", "User"])` -> `users::User` (future)
    Path(Path, TyAttr),

    // --- Primitives ---
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

    // --- Media ---
    Media(baml_base::MediaKind, TyAttr),

    // --- Type constructors ---
    Optional(Box<TypeRef>, TyAttr),
    List(Box<TypeRef>, TyAttr),
    Map {
        key: Box<TypeRef>,
        value: Box<TypeRef>,
        attr: TyAttr,
    },
    Union(Vec<TypeRef>, TyAttr),

    // --- Literals ---
    StringLiteral(String, TyAttr),
    IntLiteral(i64, TyAttr),
    /// Float literal stored as string to avoid f64's lack of Eq/Hash.
    FloatLiteral(String, TyAttr),
    /// Boolean literal for pattern matching (true/false as types).
    BoolLiteral(bool, TyAttr),

    // --- Function types ---
    /// Function type: `(x: int, y: int) -> bool` or `(int, int) -> bool`.
    ///
    /// Parameter names are optional and for documentation only - they do not
    /// affect type equality or type checking.
    Function {
        params: Vec<FunctionTypeParamRef>,
        ret: Box<TypeRef>,
        attr: TyAttr,
    },

    // --- Future ---
    /// Future: Generic type application.
    /// Example: Result<User, string>
    #[allow(dead_code)]
    Generic {
        base: Box<TypeRef>,
        args: Vec<TypeRef>,
        attr: TyAttr,
    },
    /// Future: Type parameter reference.
    /// Example: T in `function<T>(x: T) -> T`
    #[allow(dead_code)]
    TypeParam(Name, TyAttr),

    // --- Sentinels ---
    /// Error sentinel.
    Error {
        attr: TyAttr,
    },
    /// Unknown/inferred (internal use for error recovery).
    Unknown {
        attr: TyAttr,
    },
    /// The `unknown` type keyword - accepts any value.
    /// Used in builtin functions like `render_prompt(args: map<string, unknown>)`.
    /// Maps to `Ty::BuiltinUnknown` in TIR.
    BuiltinUnknown {
        attr: TyAttr,
    },
    /// The bottom type — uninhabited (no values).
    /// Maps to `tir::Ty::Never` during lowering.
    Never {
        attr: TyAttr,
    },
    /// The `type` type keyword — the meta-type for type values.
    /// Used in type annotations like `let t: type = ...`.
    /// Maps to `tir::Ty::Type` during lowering.
    Type {
        attr: TyAttr,
    },
}

impl TypeRef {
    // --- TyAttr accessors ---

    /// Get the `TyAttr` for this type reference.
    pub fn attr(&self) -> &TyAttr {
        match self {
            TypeRef::Int { attr }
            | TypeRef::Float { attr }
            | TypeRef::String { attr }
            | TypeRef::Bool { attr }
            | TypeRef::Null { attr }
            | TypeRef::Error { attr }
            | TypeRef::Unknown { attr }
            | TypeRef::BuiltinUnknown { attr }
            | TypeRef::Never { attr }
            | TypeRef::Type { attr }
            | TypeRef::Map { attr, .. }
            | TypeRef::Function { attr, .. }
            | TypeRef::Generic { attr, .. } => attr,
            TypeRef::Path(_, attr)
            | TypeRef::Media(_, attr)
            | TypeRef::Optional(_, attr)
            | TypeRef::List(_, attr)
            | TypeRef::Union(_, attr)
            | TypeRef::StringLiteral(_, attr)
            | TypeRef::IntLiteral(_, attr)
            | TypeRef::FloatLiteral(_, attr)
            | TypeRef::BoolLiteral(_, attr)
            | TypeRef::TypeParam(_, attr) => attr,
        }
    }

    /// Replace the `TyAttr` on this type reference, returning a new `TypeRef`.
    /// Short-circuits if the attr is default.
    #[must_use]
    pub fn with_attr(self, attr: TyAttr) -> Self {
        if attr.is_default() {
            return self;
        }
        match self {
            TypeRef::Int { .. } => TypeRef::Int { attr },
            TypeRef::Float { .. } => TypeRef::Float { attr },
            TypeRef::String { .. } => TypeRef::String { attr },
            TypeRef::Bool { .. } => TypeRef::Bool { attr },
            TypeRef::Null { .. } => TypeRef::Null { attr },
            TypeRef::Error { .. } => TypeRef::Error { attr },
            TypeRef::Unknown { .. } => TypeRef::Unknown { attr },
            TypeRef::BuiltinUnknown { .. } => TypeRef::BuiltinUnknown { attr },
            TypeRef::Never { .. } => TypeRef::Never { attr },
            TypeRef::Type { .. } => TypeRef::Type { attr },
            TypeRef::Path(p, _) => TypeRef::Path(p, attr),
            TypeRef::Media(kind, _) => TypeRef::Media(kind, attr),
            TypeRef::Optional(inner, _) => TypeRef::Optional(inner, attr),
            TypeRef::List(inner, _) => TypeRef::List(inner, attr),
            TypeRef::Union(members, _) => TypeRef::Union(members, attr),
            TypeRef::StringLiteral(s, _) => TypeRef::StringLiteral(s, attr),
            TypeRef::IntLiteral(i, _) => TypeRef::IntLiteral(i, attr),
            TypeRef::FloatLiteral(f, _) => TypeRef::FloatLiteral(f, attr),
            TypeRef::BoolLiteral(b, _) => TypeRef::BoolLiteral(b, attr),
            TypeRef::TypeParam(n, _) => TypeRef::TypeParam(n, attr),
            TypeRef::Map { key, value, .. } => TypeRef::Map { key, value, attr },
            TypeRef::Function { params, ret, .. } => TypeRef::Function { params, ret, attr },
            TypeRef::Generic { base, args, .. } => TypeRef::Generic { base, args, attr },
        }
    }

    // ── Convenience constructors (default attr) ──

    pub fn int() -> Self {
        TypeRef::Int {
            attr: TyAttr::default(),
        }
    }
    pub fn float() -> Self {
        TypeRef::Float {
            attr: TyAttr::default(),
        }
    }
    pub fn string() -> Self {
        TypeRef::String {
            attr: TyAttr::default(),
        }
    }
    pub fn bool_() -> Self {
        TypeRef::Bool {
            attr: TyAttr::default(),
        }
    }
    pub fn null() -> Self {
        TypeRef::Null {
            attr: TyAttr::default(),
        }
    }
    pub fn never() -> Self {
        TypeRef::Never {
            attr: TyAttr::default(),
        }
    }
    pub fn error() -> Self {
        TypeRef::Error {
            attr: TyAttr::default(),
        }
    }
    pub fn unknown() -> Self {
        TypeRef::Unknown {
            attr: TyAttr::default(),
        }
    }
    pub fn builtin_unknown() -> Self {
        TypeRef::BuiltinUnknown {
            attr: TyAttr::default(),
        }
    }
    pub fn type_() -> Self {
        TypeRef::Type {
            attr: TyAttr::default(),
        }
    }
    pub fn media(kind: baml_base::MediaKind) -> Self {
        TypeRef::Media(kind, TyAttr::default())
    }
    pub fn path(p: Path) -> Self {
        TypeRef::Path(p, TyAttr::default())
    }
    pub fn string_literal(s: String) -> Self {
        TypeRef::StringLiteral(s, TyAttr::default())
    }
    pub fn int_literal(i: i64) -> Self {
        TypeRef::IntLiteral(i, TyAttr::default())
    }
    pub fn float_literal(f: String) -> Self {
        TypeRef::FloatLiteral(f, TyAttr::default())
    }
    pub fn bool_literal(b: bool) -> Self {
        TypeRef::BoolLiteral(b, TyAttr::default())
    }

    /// Create a simple named type reference.
    pub fn named(name: Name) -> Self {
        Self::path(Path::single(name))
    }

    /// Create an optional type.
    pub fn optional(inner: TypeRef) -> Self {
        TypeRef::Optional(Box::new(inner), TyAttr::default())
    }

    /// Create a list type.
    pub fn list(inner: TypeRef) -> Self {
        TypeRef::List(Box::new(inner), TyAttr::default())
    }

    /// Create a union type.
    pub fn union(types: Vec<TypeRef>) -> Self {
        TypeRef::Union(types, TyAttr::default())
    }

    /// Create a `TypeRef` from an AST `TypeExpr` node.
    ///
    /// Delegates to `from_ast_base` for the base type, then applies any
    /// top-level postfix modifiers (`[]`, `?`).
    pub fn from_ast(type_expr: &baml_compiler_syntax::ast::TypeExpr) -> Self {
        let base = Self::from_ast_base(type_expr);
        Self::apply_modifiers(base, &type_expr.postfix_modifiers())
    }

    /// Parse the base type (no optional, no array).
    fn from_ast_base(type_expr: &baml_compiler_syntax::ast::TypeExpr) -> Self {
        // Handle top-level unions like `int[] | string?`
        if type_expr.is_union() {
            let member_parts = type_expr.union_member_parts();
            let members: Vec<TypeRef> = member_parts.iter().map(Self::from_union_member).collect();
            return Self::union(members);
        }
        // Handle function types like `(x: int, y: int) -> bool`
        if type_expr.is_function_type() {
            let params = type_expr
                .function_type_params()
                .iter()
                .map(|p| {
                    let name = p.name().map(Name::new);
                    let ty = p
                        .ty()
                        .map(|t| Self::from_ast(&t))
                        .unwrap_or_else(Self::unknown);
                    FunctionTypeParamRef { name, ty }
                })
                .collect();
            let ret = type_expr
                .function_return_type()
                .map(|t| Self::from_ast(&t))
                .unwrap_or_else(Self::unknown);
            return TypeRef::Function {
                params,
                ret: Box::new(ret),
                attr: TyAttr::default(),
            };
        }

        // Handle parenthesized types like `(int | string)`
        if let Some(inner) = type_expr.inner_type_expr() {
            return Self::from_ast(&inner);
        }

        // Handle parenthesized unions: `(A | B)` where the union is inside parens
        // In the new parser structure, each union member is wrapped in FUNCTION_TYPE_PARAM.
        // If there are multiple FUNCTION_TYPE_PARAMs but no arrow (not a function type),
        // this is a parenthesized union.
        if type_expr.is_parenthesized() && !type_expr.is_function_type() {
            let params = type_expr.function_type_params();
            if params.len() > 1 {
                // This is a parenthesized union like `(A | B | C)`
                let members: Vec<TypeRef> = params
                    .iter()
                    .filter_map(baml_compiler_syntax::FunctionTypeParam::ty)
                    .map(|t| Self::from_ast(&t))
                    .collect();
                if !members.is_empty() {
                    return Self::union(members);
                }
            }
        }

        Self::from_ast_base_type(type_expr)
    }

    /// Parse a base type (no modifiers, not a union).
    fn from_ast_base_type(type_expr: &baml_compiler_syntax::ast::TypeExpr) -> Self {
        // Check for string literal types like `"user"`
        if let Some(s) = type_expr.string_literal() {
            return Self::string_literal(s);
        }

        // Check for integer literal types like `200`
        if let Some(i) = type_expr.integer_literal() {
            return Self::int_literal(i);
        }

        // Check for boolean literal types
        if let Some(b) = type_expr.bool_literal() {
            return Self::bool_literal(b);
        }

        // Check for map type with type args
        if let Some(name) = type_expr.dotted_name() {
            if name == "map" {
                let args = type_expr.type_arg_exprs();
                if args.len() == 2 {
                    let key = Self::from_ast(&args[0]);
                    let value = Self::from_ast(&args[1]);
                    return TypeRef::Map {
                        key: Box::new(key),
                        value: Box::new(value),
                        attr: TyAttr::default(),
                    };
                }
            }

            // Named type (primitive or user-defined)
            return Self::from_type_name(&name);
        }

        Self::unknown()
    }

    /// Parse a union member from its structured parts (tokens and child nodes).
    ///
    /// Uses token kinds and child node kinds directly instead of string manipulation.
    fn from_union_member(parts: &baml_compiler_syntax::ast::UnionMemberParts) -> Self {
        // Check for parenthesized type first (e.g., `(int | string)` in `A | (int | string)`)
        if let Some(type_expr) = parts.type_expr() {
            let base = Self::from_ast(&type_expr);
            return Self::apply_modifiers(base, &parts.postfix_modifiers());
        }

        // Check for FUNCTION_TYPE_PARAM child (new parser structure for parenthesized types)
        // e.g., `(Union | Union)` has L_PAREN, FUNCTION_TYPE_PARAM, R_PAREN as children
        if let Some(func_param) = parts.function_type_param() {
            // Get the TYPE_EXPR inside the FUNCTION_TYPE_PARAM
            if let Some(inner_type_expr) = func_param
                .children()
                .find(|n| n.kind() == baml_compiler_syntax::SyntaxKind::TYPE_EXPR)
            {
                if let Some(type_expr) = baml_compiler_syntax::ast::TypeExpr::cast(inner_type_expr)
                {
                    let base = Self::from_ast(&type_expr);
                    return Self::apply_modifiers(base, &parts.postfix_modifiers());
                }
            }
        }

        // Check for string literal (e.g., `"user"` in `"user" | "admin"`)
        if let Some(s) = parts.string_literal() {
            let base = Self::string_literal(s);
            return Self::apply_modifiers(base, &parts.postfix_modifiers());
        }

        // Check for integer literal (e.g., `200` in `200 | 201`)
        if let Some(i) = parts.integer_literal() {
            let base = Self::int_literal(i);
            return Self::apply_modifiers(base, &parts.postfix_modifiers());
        }

        // Check for named/primitive type or map type (e.g., `int`, `User`, `map<K,V>`, `baml.http.Request`)
        if let Some(name) = parts.dotted_name() {
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
                            attr: TyAttr::default(),
                        };
                        return Self::apply_modifiers(base, &parts.postfix_modifiers());
                    }
                }
            }

            // Check for boolean literals
            let base = match name.as_str() {
                "true" => Self::bool_literal(true),
                "false" => Self::bool_literal(false),
                _ => Self::from_type_name(&name),
            };
            return Self::apply_modifiers(base, &parts.postfix_modifiers());
        }

        Self::unknown()
    }

    /// Apply postfix modifiers (`[]` and `?`) to a base type, innermost first.
    fn apply_modifiers(base: Self, modifiers: &[TypePostFixModifier]) -> Self {
        let mut result = base;
        for modifier in modifiers {
            match modifier {
                TypePostFixModifier::Optional => {
                    result = Self::optional(result);
                }
                TypePostFixModifier::Array => {
                    result = Self::list(result);
                }
            }
        }
        result
    }

    /// Create a `TypeRef` from a type name string (primitive or user-defined).
    pub fn from_type_name(name: &str) -> Self {
        // Use case-sensitive matching for type keywords.
        // This ensures that `Unknown` is treated as a user-defined type name,
        // not as the `unknown` builtin keyword.
        match name {
            "int" => Self::int(),
            "float" => Self::float(),
            "string" => Self::string(),
            "bool" => Self::bool_(),
            "null" => Self::null(),
            "unknown" => Self::builtin_unknown(),
            "never" => Self::never(),
            "type" => Self::type_(),
            "image" => Self::media(baml_base::MediaKind::Image),
            "audio" => Self::media(baml_base::MediaKind::Audio),
            "video" => Self::media(baml_base::MediaKind::Video),
            "pdf" => Self::media(baml_base::MediaKind::Pdf),
            _ => {
                if name.contains('.') {
                    let segments: Vec<Name> = name.split('.').map(Name::new).collect();
                    Self::path(Path::new(segments))
                } else {
                    Self::path(Path::single(Name::new(name)))
                }
            }
        }
    }
}
