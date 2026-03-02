//! PPIR type representation and field representation.
//!
//! `PpirTy` carries type-level stream annotations (`PpirTypeAttrs`) on every variant.
//! `PpirField` represents a parsed field with field-level annotations.
//! The `Ty` classification enum is removed — `stream_expand` does inline name
//! classification via `PpirNames` lookups.

use baml_base::Name;
use baml_compiler_syntax::SyntaxNode;
use rowan::ast::AstNode as _;
use smol_str::SmolStr;

//
// ──────────────────────────────────────────────── TYPE ATTRS ─────
//

/// Type-level attributes captured from the CST.
/// Carried on every `PpirTy` variant via named `attrs` field.
///
/// Default is "no annotations" — most types have no stream attrs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct PpirTypeAttrs {
    /// `@stream.type(D)` — explicit streaming type override.
    /// When `Some`, `stream_expand` uses D instead of recursing into this type.
    pub stream_type: Option<Box<PpirTy>>,

    /// `@stream.done` — indicates `@sap.in_progress(never)`.
    /// When true and `stream_type` is `None`, desugars to `stream_type = Some(T)`
    /// where T is the type this attr is on.
    pub stream_done: bool,

    /// `@stream.with_state` — wrap final stream type in `StreamState<T>`.
    /// Consumed during TIR stream type generation.
    pub stream_with_state: bool,
}

impl PpirTypeAttrs {
    /// Returns true if no type-level stream annotations are set.
    pub fn is_empty(&self) -> bool {
        self.stream_type.is_none() && !self.stream_done && !self.stream_with_state
    }
}

//
// ──────────────────────────────────────────────── PPIR TY ─────
//

/// PPIR's type reference — carries type-level stream annotations on every variant.
///
/// Structurally parallel to `hir::TypeRef` but defined independently to avoid
/// a circular dependency (PPIR does not depend on HIR).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PpirTy {
    /// Named type reference (user-defined class, enum, type alias, or `stream_*` name).
    Named {
        name: Name,
        attrs: PpirTypeAttrs,
    },

    /// Primitives.
    Int {
        attrs: PpirTypeAttrs,
    },
    Float {
        attrs: PpirTypeAttrs,
    },
    String {
        attrs: PpirTypeAttrs,
    },
    Bool {
        attrs: PpirTypeAttrs,
    },

    /// Null and never.
    Null {
        attrs: PpirTypeAttrs,
    },
    Never {
        attrs: PpirTypeAttrs,
    },

    /// Type constructors.
    Optional {
        inner: Box<PpirTy>,
        attrs: PpirTypeAttrs,
    },
    List {
        inner: Box<PpirTy>,
        attrs: PpirTypeAttrs,
    },
    Map {
        key: Box<PpirTy>,
        value: Box<PpirTy>,
        attrs: PpirTypeAttrs,
    },
    Union {
        variants: Vec<PpirTy>,
        attrs: PpirTypeAttrs,
    },

    /// Literal types.
    StringLiteral {
        value: std::string::String,
        attrs: PpirTypeAttrs,
    },
    IntLiteral {
        value: i64,
        attrs: PpirTypeAttrs,
    },
    BoolLiteral {
        value: bool,
        attrs: PpirTypeAttrs,
    },

    /// Media types.
    Media {
        kind: baml_base::MediaKind,
        attrs: PpirTypeAttrs,
    },

    /// Error recovery / unknown.
    Unknown {
        attrs: PpirTypeAttrs,
    },
}

impl PpirTy {
    /// Get a reference to this type's stream attributes.
    pub fn attrs(&self) -> &PpirTypeAttrs {
        match self {
            Self::Named { attrs, .. }
            | Self::Int { attrs }
            | Self::Float { attrs }
            | Self::String { attrs }
            | Self::Bool { attrs }
            | Self::Null { attrs }
            | Self::Never { attrs }
            | Self::Optional { attrs, .. }
            | Self::List { attrs, .. }
            | Self::Map { attrs, .. }
            | Self::Union { attrs, .. }
            | Self::StringLiteral { attrs, .. }
            | Self::IntLiteral { attrs, .. }
            | Self::BoolLiteral { attrs, .. }
            | Self::Media { attrs, .. }
            | Self::Unknown { attrs } => attrs,
        }
    }

    /// Get a mutable reference to this type's stream attributes.
    pub fn attrs_mut(&mut self) -> &mut PpirTypeAttrs {
        match self {
            Self::Named { attrs, .. }
            | Self::Int { attrs }
            | Self::Float { attrs }
            | Self::String { attrs }
            | Self::Bool { attrs }
            | Self::Null { attrs }
            | Self::Never { attrs }
            | Self::Optional { attrs, .. }
            | Self::List { attrs, .. }
            | Self::Map { attrs, .. }
            | Self::Union { attrs, .. }
            | Self::StringLiteral { attrs, .. }
            | Self::IntLiteral { attrs, .. }
            | Self::BoolLiteral { attrs, .. }
            | Self::Media { attrs, .. }
            | Self::Unknown { attrs } => attrs,
        }
    }

    /// Clone this type with default (empty) attrs.
    /// Used by `stream_expand` to strip annotations after consuming them.
    #[must_use]
    pub fn clone_without_attrs(&self) -> Self {
        let d = PpirTypeAttrs::default();
        match self {
            Self::Named { name, .. } => Self::Named {
                name: name.clone(),
                attrs: d,
            },
            Self::Int { .. } => Self::Int { attrs: d },
            Self::Float { .. } => Self::Float { attrs: d },
            Self::String { .. } => Self::String { attrs: d },
            Self::Bool { .. } => Self::Bool { attrs: d },
            Self::Null { .. } => Self::Null { attrs: d },
            Self::Never { .. } => Self::Never { attrs: d },
            Self::Optional { inner, .. } => Self::Optional {
                inner: inner.clone(),
                attrs: d,
            },
            Self::List { inner, .. } => Self::List {
                inner: inner.clone(),
                attrs: d,
            },
            Self::Map { key, value, .. } => Self::Map {
                key: key.clone(),
                value: value.clone(),
                attrs: d,
            },
            Self::Union { variants, .. } => Self::Union {
                variants: variants.clone(),
                attrs: d,
            },
            Self::StringLiteral { value, .. } => Self::StringLiteral {
                value: value.clone(),
                attrs: d,
            },
            Self::IntLiteral { value, .. } => Self::IntLiteral {
                value: *value,
                attrs: d,
            },
            Self::BoolLiteral { value, .. } => Self::BoolLiteral {
                value: *value,
                attrs: d,
            },
            Self::Media { kind, .. } => Self::Media {
                kind: *kind,
                attrs: d,
            },
            Self::Unknown { .. } => Self::Unknown { attrs: d },
        }
    }

    //
    // ──────────────────────────── CONSTRUCTORS ─────
    //

    /// Create a simple named type reference.
    pub fn named(name: impl Into<Name>) -> Self {
        PpirTy::Named {
            name: name.into(),
            attrs: PpirTypeAttrs::default(),
        }
    }

    /// Create a list type.
    pub fn list(inner: PpirTy) -> Self {
        PpirTy::List {
            inner: Box::new(inner),
            attrs: PpirTypeAttrs::default(),
        }
    }

    /// Create an optional type.
    pub fn optional(inner: PpirTy) -> Self {
        PpirTy::Optional {
            inner: Box::new(inner),
            attrs: PpirTypeAttrs::default(),
        }
    }

    /// Create a union type.
    pub fn union(types: Vec<PpirTy>) -> Self {
        PpirTy::Union {
            variants: types,
            attrs: PpirTypeAttrs::default(),
        }
    }

    /// Create a `PpirTy` from a type name string (primitive or user-defined).
    pub fn from_type_name(name: &str) -> Self {
        let d = PpirTypeAttrs::default();
        match name {
            "int" => PpirTy::Int { attrs: d },
            "float" => PpirTy::Float { attrs: d },
            "string" => PpirTy::String { attrs: d },
            "bool" => PpirTy::Bool { attrs: d },
            "null" => PpirTy::Null { attrs: d },
            "never" => PpirTy::Never { attrs: d },
            "image" => PpirTy::Media {
                kind: baml_base::MediaKind::Image,
                attrs: d,
            },
            "audio" => PpirTy::Media {
                kind: baml_base::MediaKind::Audio,
                attrs: d,
            },
            "video" => PpirTy::Media {
                kind: baml_base::MediaKind::Video,
                attrs: d,
            },
            "pdf" => PpirTy::Media {
                kind: baml_base::MediaKind::Pdf,
                attrs: d,
            },
            _ => PpirTy::Named {
                name: SmolStr::new(name),
                attrs: d,
            },
        }
    }

    //
    // ──────────────────────────── CST PARSING ─────
    //

    /// Parse a CST `TypeExpr` into a `PpirTy`.
    ///
    /// Parses the type structure and captures type-level `@stream.*` annotations
    /// from ATTRIBUTE children of the `TYPE_EXPR`.
    pub fn from_ast(type_expr: &baml_compiler_syntax::ast::TypeExpr) -> Self {
        // Parse the type structure
        let mut result = Self::from_ast_structure(type_expr);

        // Capture type-level annotations from ATTRIBUTE children
        for attr in type_expr.attributes() {
            if let Some(attr_name) = attr.full_name() {
                match attr_name.as_str() {
                    "stream.done" => {
                        result.attrs_mut().stream_done = true;
                    }
                    "stream.type" => {
                        if let Some(type_arg) = attr.string_arg() {
                            result.attrs_mut().stream_type =
                                Some(Box::new(Self::from_type_name(&type_arg)));
                        }
                    }
                    "stream.with_state" => {
                        result.attrs_mut().stream_with_state = true;
                    }
                    _ => {
                        // Other type-level attrs (e.g., @assert, @check) ignored by PPIR
                    }
                }
            }
        }

        result
    }

    /// Parse the type structure (handling optional, union, array).
    fn from_ast_structure(type_expr: &baml_compiler_syntax::ast::TypeExpr) -> Self {
        // Handle optional modifier (outermost)
        if type_expr.is_optional() {
            let inner = Self::from_ast_without_optional(type_expr);
            return PpirTy::Optional {
                inner: Box::new(inner),
                attrs: PpirTypeAttrs::default(),
            };
        }

        Self::from_ast_without_optional(type_expr)
    }

    /// Parse a `TypeExpr` assuming the optional modifier has been handled.
    fn from_ast_without_optional(type_expr: &baml_compiler_syntax::ast::TypeExpr) -> Self {
        // Handle union FIRST (top-level PIPE)
        if type_expr.is_union() {
            let member_parts = type_expr.union_member_parts();
            let members: Vec<PpirTy> = member_parts.iter().map(Self::from_union_member).collect();
            return PpirTy::Union {
                variants: members,
                attrs: PpirTypeAttrs::default(),
            };
        }

        // Handle array modifier
        if type_expr.is_array() {
            let element = Self::from_ast_array_element(type_expr);
            return PpirTy::List {
                inner: Box::new(element),
                attrs: PpirTypeAttrs::default(),
            };
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
            result = PpirTy::List {
                inner: Box::new(result),
                attrs: PpirTypeAttrs::default(),
            };
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
                let members: Vec<PpirTy> = params
                    .iter()
                    .filter_map(baml_compiler_syntax::FunctionTypeParam::ty)
                    .map(|t| Self::from_ast(&t))
                    .collect();
                if !members.is_empty() {
                    return PpirTy::Union {
                        variants: members,
                        attrs: PpirTypeAttrs::default(),
                    };
                }
            }
        }

        Self::from_ast_base_type(type_expr)
    }

    /// Parse a base type (no modifiers, not a union).
    fn from_ast_base_type(type_expr: &baml_compiler_syntax::ast::TypeExpr) -> Self {
        let d = PpirTypeAttrs::default();

        if let Some(s) = type_expr.string_literal() {
            return PpirTy::StringLiteral { value: s, attrs: d };
        }

        if let Some(i) = type_expr.integer_literal() {
            return PpirTy::IntLiteral { value: i, attrs: d };
        }

        if let Some(b) = type_expr.bool_literal() {
            return PpirTy::BoolLiteral { value: b, attrs: d };
        }

        if let Some(name) = type_expr.dotted_name() {
            if name == "map" {
                let args = type_expr.type_arg_exprs();
                if args.len() == 2 {
                    let key = Self::from_ast(&args[0]);
                    let value = Self::from_ast(&args[1]);
                    return PpirTy::Map {
                        key: Box::new(key),
                        value: Box::new(value),
                        attrs: d,
                    };
                }
            }

            return Self::from_type_name(&name);
        }

        PpirTy::Unknown { attrs: d }
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
            let base = PpirTy::StringLiteral {
                value: s,
                attrs: PpirTypeAttrs::default(),
            };
            return Self::apply_modifiers_from_parts(base, parts);
        }

        if let Some(i) = parts.integer_literal() {
            let base = PpirTy::IntLiteral {
                value: i,
                attrs: PpirTypeAttrs::default(),
            };
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
                        let base = PpirTy::Map {
                            key: Box::new(key),
                            value: Box::new(value),
                            attrs: PpirTypeAttrs::default(),
                        };
                        return Self::apply_modifiers_from_parts(base, parts);
                    }
                }
            }

            let base = match name.as_str() {
                "true" => PpirTy::BoolLiteral {
                    value: true,
                    attrs: PpirTypeAttrs::default(),
                },
                "false" => PpirTy::BoolLiteral {
                    value: false,
                    attrs: PpirTypeAttrs::default(),
                },
                _ => Self::from_type_name(&name),
            };
            return Self::apply_modifiers_from_parts(base, parts);
        }

        PpirTy::Unknown {
            attrs: PpirTypeAttrs::default(),
        }
    }

    /// Apply array and optional modifiers from `UnionMemberParts` to a base type.
    fn apply_modifiers_from_parts(
        base: Self,
        parts: &baml_compiler_syntax::ast::UnionMemberParts,
    ) -> Self {
        use baml_compiler_syntax::ast::TypePostFixModifier;

        let mut result = base;
        for modifier in parts.postfix_modifiers() {
            match modifier {
                TypePostFixModifier::Array => {
                    result = PpirTy::List {
                        inner: Box::new(result),
                        attrs: PpirTypeAttrs::default(),
                    };
                }
                TypePostFixModifier::Optional => {
                    result = PpirTy::Optional {
                        inner: Box::new(result),
                        attrs: PpirTypeAttrs::default(),
                    };
                }
            }
        }

        result
    }
}

//
// ──────────────────────────────────────────────────────── FIELD ─────
//

/// A PPIR field with parsed type and field-level stream annotations.
///
/// This is an intermediate representation — the expansion step
/// processes these into `PpirDesugaredField`s.
/// Carry-through attributes (alias, description, skip) are preserved by
/// cloning the original CST FIELD node during `expand_cst`.
#[derive(Debug, Clone)]
pub struct PpirField {
    pub name: Name,
    /// The parsed type (carries type-level attrs from CST).
    /// `stream_expand` operates directly on this with inline `PpirNames` lookups.
    pub ty: PpirTy,

    // Field-level annotations only:
    /// `@stream.starts_as(<arg>)` — raw `SyntaxNode` cloned from the CST, or None if not specified.
    /// Parsing of `<arg>` is deferred to PPIR → HIR lowering.
    pub starts_as: Option<SyntaxNode>,
    /// `@stream.not_null` → desugared to `starts_as = "never"`.
    pub not_null: bool,
}
