//! PPIR type classification, type references, and field representation.

use baml_base::Name;
use rowan::ast::AstNode as _;
use smol_str::SmolStr;

use crate::PpirNames;

//
// ──────────────────────────────────────────────────────── TYPE REF ─────
//

/// PPIR's type reference — the output type for stream-expanded field types.
/// Structurally parallel to `hir::TypeRef` but defined independently to avoid
/// a circular dependency (PPIR does not depend on HIR).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PpirTypeRef {
    /// Named type reference (user-defined class, enum, type alias, or stream_* name).
    Named(Name),

    /// Primitives.
    Int,
    Float,
    String,
    Bool,

    /// Null and never.
    Null,
    Never,

    /// Type constructors.
    Optional(Box<PpirTypeRef>),
    List(Box<PpirTypeRef>),
    Map {
        key: Box<PpirTypeRef>,
        value: Box<PpirTypeRef>,
    },
    Union(Vec<PpirTypeRef>),

    /// Literal types.
    StringLiteral(std::string::String),
    IntLiteral(i64),
    BoolLiteral(bool),

    /// Media types.
    Media(baml_base::MediaKind),

    /// Anything else (error recovery, unknown).
    Unknown,
}

impl PpirTypeRef {
    /// Create a simple named type reference.
    pub fn named(name: impl Into<Name>) -> Self {
        PpirTypeRef::Named(name.into())
    }

    /// Create a list type.
    pub fn list(inner: PpirTypeRef) -> Self {
        PpirTypeRef::List(Box::new(inner))
    }

    /// Create an optional type.
    pub fn optional(inner: PpirTypeRef) -> Self {
        PpirTypeRef::Optional(Box::new(inner))
    }

    /// Create a union type.
    pub fn union(types: Vec<PpirTypeRef>) -> Self {
        PpirTypeRef::Union(types)
    }

    /// Create a `PpirTypeRef` from a type name string (primitive or user-defined).
    pub fn from_type_name(name: &str) -> Self {
        match name {
            "int" => PpirTypeRef::Int,
            "float" => PpirTypeRef::Float,
            "string" => PpirTypeRef::String,
            "bool" => PpirTypeRef::Bool,
            "null" => PpirTypeRef::Null,
            "never" => PpirTypeRef::Never,
            "image" => PpirTypeRef::Media(baml_base::MediaKind::Image),
            "audio" => PpirTypeRef::Media(baml_base::MediaKind::Audio),
            "video" => PpirTypeRef::Media(baml_base::MediaKind::Video),
            "pdf" => PpirTypeRef::Media(baml_base::MediaKind::Pdf),
            _ => PpirTypeRef::Named(SmolStr::new(name)),
        }
    }

    /// Parse a CST `TypeExpr` into a `PpirTypeRef`.
    ///
    /// Uses the same CST accessor methods as `hir::PpirTypeRef::from_ast`, mapping
    /// to `PpirTypeRef` variants instead.
    pub fn from_ast(type_expr: &baml_compiler_syntax::ast::TypeExpr) -> Self {
        // Handle optional modifier (outermost)
        if type_expr.is_optional() {
            let inner = Self::from_ast_without_optional(type_expr);
            return PpirTypeRef::Optional(Box::new(inner));
        }

        Self::from_ast_without_optional(type_expr)
    }

    /// Parse a `TypeExpr` assuming the optional modifier has been handled.
    fn from_ast_without_optional(type_expr: &baml_compiler_syntax::ast::TypeExpr) -> Self {
        // Handle union FIRST (top-level PIPE)
        if type_expr.is_union() {
            let member_parts = type_expr.union_member_parts();
            let members: Vec<PpirTypeRef> = member_parts.iter().map(Self::from_union_member).collect();
            return PpirTypeRef::Union(members);
        }

        // Handle array modifier
        if type_expr.is_array() {
            let element = Self::from_ast_array_element(type_expr);
            return PpirTypeRef::List(Box::new(element));
        }

        Self::from_ast_base(type_expr)
    }

    /// Get the element type for an array `TypeExpr`.
    fn from_ast_array_element(type_expr: &baml_compiler_syntax::ast::TypeExpr) -> Self {
        if let Some(inner) = type_expr.inner_type_expr() {
            return Self::from_ast(&inner);
        }

        let depth = type_expr.array_depth();
        let base = Self::from_ast_base_type(type_expr);

        let mut result = base;
        for _ in 0..depth.saturating_sub(1) {
            result = PpirTypeRef::List(Box::new(result));
        }
        result
    }

    /// Parse the base type (no optional, array, or union modifiers).
    fn from_ast_base(type_expr: &baml_compiler_syntax::ast::TypeExpr) -> Self {
        // Handle parenthesized types like `(int | string)`
        if let Some(inner) = type_expr.inner_type_expr() {
            return Self::from_ast(&inner);
        }

        // Handle parenthesized unions
        if type_expr.is_parenthesized() && !type_expr.is_function_type() {
            let params = type_expr.function_type_params();
            if params.len() > 1 {
                let members: Vec<PpirTypeRef> = params
                    .iter()
                    .filter_map(baml_compiler_syntax::FunctionTypeParam::ty)
                    .map(|t| Self::from_ast(&t))
                    .collect();
                if !members.is_empty() {
                    return PpirTypeRef::Union(members);
                }
            }
        }

        Self::from_ast_base_type(type_expr)
    }

    /// Parse a base type (no modifiers, not a union).
    fn from_ast_base_type(type_expr: &baml_compiler_syntax::ast::TypeExpr) -> Self {
        if let Some(s) = type_expr.string_literal() {
            return PpirTypeRef::StringLiteral(s);
        }

        if let Some(i) = type_expr.integer_literal() {
            return PpirTypeRef::IntLiteral(i);
        }

        if let Some(b) = type_expr.bool_literal() {
            return PpirTypeRef::BoolLiteral(b);
        }

        if let Some(name) = type_expr.dotted_name() {
            if name == "map" {
                let args = type_expr.type_arg_exprs();
                if args.len() == 2 {
                    let key = Self::from_ast(&args[0]);
                    let value = Self::from_ast(&args[1]);
                    return PpirTypeRef::Map {
                        key: Box::new(key),
                        value: Box::new(value),
                    };
                }
            }

            return Self::from_type_name(&name);
        }

        PpirTypeRef::Unknown
    }

    /// Parse a union member from its structured parts.
    fn from_union_member(parts: &baml_compiler_syntax::ast::UnionMemberParts) -> Self {
        if let Some(type_expr) = parts.type_expr() {
            let inner = Self::from_ast(&type_expr);
            return Self::apply_modifiers_from_parts(inner, parts);
        }

        if let Some(func_param) = parts.function_type_param() {
            if let Some(inner_type_expr) = func_param
                .children()
                .find(|n| n.kind() == baml_compiler_syntax::SyntaxKind::TYPE_EXPR)
            {
                if let Some(type_expr) = baml_compiler_syntax::ast::TypeExpr::cast(inner_type_expr)
                {
                    let inner = Self::from_ast(&type_expr);
                    return Self::apply_modifiers_from_parts(inner, parts);
                }
            }
        }

        if let Some(s) = parts.string_literal() {
            let base = PpirTypeRef::StringLiteral(s);
            return Self::apply_modifiers_from_parts(base, parts);
        }

        if let Some(i) = parts.integer_literal() {
            let base = PpirTypeRef::IntLiteral(i);
            return Self::apply_modifiers_from_parts(base, parts);
        }

        if let Some(name) = parts.dotted_name() {
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
                        let base = PpirTypeRef::Map {
                            key: Box::new(key),
                            value: Box::new(value),
                        };
                        return Self::apply_modifiers_from_parts(base, parts);
                    }
                }
            }

            let base = match name.as_str() {
                "true" => PpirTypeRef::BoolLiteral(true),
                "false" => PpirTypeRef::BoolLiteral(false),
                _ => Self::from_type_name(&name),
            };
            return Self::apply_modifiers_from_parts(base, parts);
        }

        PpirTypeRef::Unknown
    }

    /// Apply array and optional modifiers from `UnionMemberParts` to a base type.
    fn apply_modifiers_from_parts(
        base: Self,
        parts: &baml_compiler_syntax::ast::UnionMemberParts,
    ) -> Self {
        let array_depth = parts.array_depth();
        let is_optional = parts.is_optional();

        let mut result = base;
        for _ in 0..array_depth {
            result = PpirTypeRef::List(Box::new(result));
        }

        if is_optional {
            result = PpirTypeRef::Optional(Box::new(result));
        }

        result
    }
}

//
// ──────────────────────────────────────────────────────── CLASSIFIED TY ─────
//

/// PPIR's classified type representation.
///
/// Enriches `PpirTypeRef` with cross-file name resolution knowledge
/// needed for stream expansion. Each named type (`PpirTypeRef::Named`) is
/// resolved to its category (Class, Enum, `TypeAlias`, Unknown) using
/// the cross-file `PpirNames`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    /// Primitive types: int, float, bool, string.
    /// `stream_expand` → T (unchanged). default S → null.
    Primitive(PpirTypeRef),

    /// Literal types: "foo", 42, true, false.
    /// `stream_expand` → T (unchanged). default S → never.
    Literal(PpirTypeRef),

    /// Null type. `stream_expand` → null. default S → null.
    Null,

    /// Never type. `stream_expand` → never. default S → never.
    Never,

    /// Named type classified as a class.
    /// `stream_expand` → `stream_T`. default S → null.
    Class(Name),

    /// Named type classified as an enum.
    /// `stream_expand` → T (unchanged). default S → null.
    Enum(Name),

    /// Named type classified as a type alias.
    /// `stream_expand` → `stream_T`. default S → null.
    TypeAlias(Name),

    /// Unresolvable named type.
    /// Passes through unchanged. TIR will report the resolution error.
    Unknown(PpirTypeRef),

    /// List type with classified element.
    /// `stream_expand` → `stream_expand(elem)[]`. default S → `[]`.
    List(Box<Ty>),

    /// Map type with classified value.
    /// `stream_expand` → `map<key, stream_expand(value)>`. default S → `{}`.
    Map { key: PpirTypeRef, value: Box<Ty> },

    /// Union type with classified variants.
    /// `stream_expand` → union of `stream_expand(each)`. default S → null.
    Union(Vec<Ty>),

    /// Optional type (T?).
    /// `stream_expand` → `stream_expand(T) | null`. default S → null.
    Optional(Box<Ty>),

    /// Other types (media, function, etc.). Pass through unchanged.
    Other(PpirTypeRef),
}

impl Ty {
    /// Classify a `PpirTypeRef` into a PPIR `Ty` using cross-file name knowledge.
    pub fn classify(type_ref: &PpirTypeRef, names: &PpirNames<'_>, db: &dyn crate::Db) -> Self {
        match type_ref {
            PpirTypeRef::Int | PpirTypeRef::Float | PpirTypeRef::String | PpirTypeRef::Bool => {
                Ty::Primitive(type_ref.clone())
            }

            PpirTypeRef::Null => Ty::Null,
            PpirTypeRef::Never => Ty::Never,

            PpirTypeRef::StringLiteral(_) | PpirTypeRef::IntLiteral(_) | PpirTypeRef::BoolLiteral(_) => {
                Ty::Literal(type_ref.clone())
            }

            PpirTypeRef::Named(name) => {
                if names.class_names(db).contains(name) {
                    Ty::Class(name.clone())
                } else if names.enum_names(db).contains(name) {
                    Ty::Enum(name.clone())
                } else if names.type_alias_names(db).contains(name) {
                    Ty::TypeAlias(name.clone())
                } else {
                    Ty::Unknown(type_ref.clone())
                }
            }

            PpirTypeRef::List(inner) => Ty::List(Box::new(Ty::classify(inner, names, db))),

            PpirTypeRef::Map { key, value } => Ty::Map {
                key: (**key).clone(),
                value: Box::new(Ty::classify(value, names, db)),
            },

            PpirTypeRef::Union(variants) => Ty::Union(
                variants
                    .iter()
                    .map(|v| Ty::classify(v, names, db))
                    .collect(),
            ),

            PpirTypeRef::Optional(inner) => Ty::Optional(Box::new(Ty::classify(inner, names, db))),

            _ => Ty::Other(type_ref.clone()),
        }
    }

    /// Compute the stream-expanded `PpirTypeRef` (the D component).
    ///
    /// This is the recursive type rewriting that adds `stream_` prefixes
    /// to class and type alias references.
    pub fn stream_expand(&self) -> PpirTypeRef {
        match self {
            Ty::Primitive(t) | Ty::Literal(t) => t.clone(),
            Ty::Null => PpirTypeRef::Null,
            Ty::Never => PpirTypeRef::Never,
            Ty::Class(name) => PpirTypeRef::named(SmolStr::new(format!("stream_{name}"))),
            Ty::TypeAlias(name) => PpirTypeRef::named(SmolStr::new(format!("stream_{name}"))),
            Ty::Enum(name) => PpirTypeRef::named(name.clone()),
            Ty::Unknown(t) => t.clone(),
            Ty::List(elem) => PpirTypeRef::list(elem.stream_expand()),
            Ty::Map { key, value } => PpirTypeRef::Map {
                key: Box::new(key.clone()),
                value: Box::new(value.stream_expand()),
            },
            Ty::Union(variants) => PpirTypeRef::union(variants.iter().map(Ty::stream_expand).collect()),
            Ty::Optional(inner) => PpirTypeRef::union(vec![inner.stream_expand(), PpirTypeRef::Null]),
            Ty::Other(t) => t.clone(),
        }
    }
}

//
// ──────────────────────────────────────────────────────── FIELD ─────
//

/// A PPIR field with classified type and stream annotations.
/// Built by reading the CST for field data and @stream.* attributes.
///
/// Named `ClassifiedField` to distinguish from the output `expand::Field`
/// which represents a field in a generated class.
#[derive(Debug, Clone)]
pub struct ClassifiedField {
    pub name: Name,
    /// The classified type (enriched with cross-file knowledge).
    pub ty: Ty,
    /// The original `PpirTypeRef` (for desugaring @stream.done).
    pub type_ref: PpirTypeRef,

    // Stream annotations extracted from CST.
    /// @stream.type(...) — explicit stream type override.
    pub stream_type: Option<PpirTypeRef>,
    /// @`stream.starts_as`(...) — raw CST value expression, passed through to HIR.
    pub stream_starts_as: Option<std::string::String>,
    /// @`stream.with_state` — wrap in `StreamState`<T> (handled by codegen, not PPIR).
    pub stream_with_state: bool,
    /// @stream.done (legacy) — desugars to @stream.type(T) + `has_completed` flag.
    pub stream_done: bool,
    /// @`stream.not_null` (legacy) — desugars to @`stream.starts_as(never)`.
    pub stream_not_null: bool,

    // Carry-through attributes extracted from CST for generated fields.
    pub alias: Option<std::string::String>,
    pub description: Option<std::string::String>,
    pub skip: bool,
}
