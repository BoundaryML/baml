//! Concrete AST structs for BAML — full structural data in memory.
//!
//! Every node carries all its content as owned Rust data (names, type trees,
//! expression trees) with `TextRange` alongside for source mapping. A single
//! `lower_file` function converts the CST to `Vec<Item>`. This isolates all
//! CST `Option` handling in one layer so everything downstream gets clean
//! typed data and can be constructed directly in tests without parsing.

use std::collections::{HashMap, HashSet};

use baml_base::{Name, TypePath};
use la_arena::{Arena, Idx};
use text_size::TextRange;

// ── Attributes ──────────────────────────────────────────────────

/// Raw attribute from CST — not yet validated.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RawAttribute {
    pub name: Name,
    pub args: Vec<RawAttributeArg>,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RawAttributeArg {
    pub key: Option<Name>,
    pub value: String,
    pub span: TextRange,
}

// ── Type Expressions ────────────────────────────────────────────

/// Full recursive type expression — all structural content in memory.
///
/// Corresponds to `TypeRef` in `baml_compiler_hir/src/type_ref.rs` but lives
/// in the AST layer (before any name resolution). CST → `TypeExpr` conversion
/// happens once during `lower_file` and is never repeated.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeExprKind {
    /// A runtime type atom. Body-owned occurrences carry the carrier expression
    /// in the enclosing body's arena. Declaration-owned occurrences have no
    /// body arena and keep `None`; the declaration checker diagnoses them.
    Unreflect {
        operand: Option<ExprId>,
        attrs: Vec<RawAttribute>,
    },
    /// Named type path: `User`, `baml.http.Request`, `Stream<T>`
    Path {
        segments: Vec<Name>,
        /// Generic type arguments (e.g., `<T>` in `Stream<T>`). Empty for non-generic paths.
        generic_args: Vec<TypeExpr>,
        /// Named associated type bindings in type positions, e.g. `Iterator<Item = int>`.
        associated_type_bindings: Vec<AssociatedTypeBinding>,
        attrs: Vec<RawAttribute>,
    },
    /// Associated type projection: `Base.Item` or `(Base as Interface).Item`.
    AssociatedTypeProjection {
        base: Box<TypeExpr>,
        interface: Option<Box<TypeExpr>>,
        member: Name,
        attrs: Vec<RawAttribute>,
    },
    /// Primitive types
    Int {
        attrs: Vec<RawAttribute>,
    },
    Bigint {
        attrs: Vec<RawAttribute>,
    },
    Float {
        attrs: Vec<RawAttribute>,
    },
    String {
        attrs: Vec<RawAttribute>,
    },
    Bool {
        attrs: Vec<RawAttribute>,
    },
    Null {
        attrs: Vec<RawAttribute>,
    },
    Never {
        attrs: Vec<RawAttribute>,
    },
    /// The `void` type — valid only as a function return type.
    Void {
        attrs: Vec<RawAttribute>,
    },
    /// `Uint8Array` (binary data) type
    Uint8Array {
        attrs: Vec<RawAttribute>,
    },
    /// Media types
    Media {
        kind: baml_base::MediaKind,
        attrs: Vec<RawAttribute>,
    },
    /// T?
    Optional {
        inner: Box<TypeExpr>,
        attrs: Vec<RawAttribute>,
    },
    /// T[]
    List {
        inner: Box<TypeExpr>,
        attrs: Vec<RawAttribute>,
    },
    /// map<K, V>
    Map {
        key: Box<TypeExpr>,
        value: Box<TypeExpr>,
        attrs: Vec<RawAttribute>,
    },
    /// A | B | C
    Union {
        variants: Vec<TypeExpr>,
        attrs: Vec<RawAttribute>,
    },
    /// Literal types in unions: `"user"`, `200`, `3.14`, `true`.
    Literal {
        value: baml_base::Literal,
        attrs: Vec<RawAttribute>,
    },
    /// Function type: `(params) -> return throws E`. Function *values* are
    /// realized, so a function type carries no generic parameters of its own —
    /// it may only reference type variables from the enclosing context.
    Function {
        params: Vec<FunctionTypeParam>,
        ret: Box<TypeExpr>,
        throws: Option<Box<TypeExpr>>,
        attrs: Vec<RawAttribute>,
    },
    /// The `unknown` keyword type
    Unknown {
        attrs: Vec<RawAttribute>,
    },
    /// The `type` meta-type keyword
    Type {
        attrs: Vec<RawAttribute>,
    },
    /// `$rust_type` — opaque Rust-managed state field type.
    Rust {
        attrs: Vec<RawAttribute>,
    },
    /// Error recovery sentinel
    Error {
        attrs: Vec<RawAttribute>,
    },
    /// No type was written at this slot (an omitted annotation), as distinct
    /// from the written `unknown` keyword above.
    Missing {
        attrs: Vec<RawAttribute>,
    },
    /// The wildcard `_` — an inference hole. Valid only where the type at this
    /// slot can be inferred from context (a generic type argument whose binding
    /// is fixed by an initializer, or a `throws`-clause member). Lowered to
    /// `Ty::Infer` and filled during TIR checking.
    Infer {
        attrs: Vec<RawAttribute>,
    },
}

/// A type expression node paired with its source span. Every node in the tree
/// is spanned (recursively, via the `Box<TypeExpr>`/`Vec<TypeExpr>` children of
/// [`TypeExprKind`]), so a diagnostic about any sub-type (e.g. an unresolved
/// member of a union/map) can point exactly at it.
///
/// Equality and hashing are **structural** — they ignore `span`. Two occurrences
/// of the same type compare equal regardless of source position; `span` is
/// diagnostic metadata, not part of type identity (this matches the pre-spanning
/// behavior and keeps Salsa early-cutoff / dedup position-insensitive).
#[derive(Debug, Clone)]
pub struct TypeExpr {
    pub kind: TypeExprKind,
    pub span: TextRange,
}

impl PartialEq for TypeExpr {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

impl Eq for TypeExpr {}

impl std::hash::Hash for TypeExpr {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
    }
}

impl std::ops::Deref for TypeExpr {
    type Target = TypeExprKind;
    fn deref(&self) -> &TypeExprKind {
        &self.kind
    }
}

impl std::ops::DerefMut for TypeExpr {
    fn deref_mut(&mut self) -> &mut TypeExprKind {
        &mut self.kind
    }
}

impl TypeExprKind {
    /// Pair this node with its source span. `TypeExprKind::Int { .. }.at(span)`.
    pub fn at(self, span: TextRange) -> TypeExpr {
        TypeExpr { kind: self, span }
    }
}

impl TypeExpr {
    /// Override this node's top-level span (its children keep their own spans).
    /// Used where an item carries a separate annotation span for the whole type.
    #[must_use]
    pub fn with_span(mut self, span: TextRange) -> Self {
        self.span = span;
        self
    }

    /// Append every runtime carrier nested in this type, in source order.
    pub fn unreflect_operands(&self, out: &mut Vec<ExprId>) {
        match &self.kind {
            TypeExprKind::Unreflect {
                operand: Some(operand),
                ..
            } => out.push(*operand),
            TypeExprKind::Unreflect { operand: None, .. } => {}
            TypeExprKind::Path {
                generic_args,
                associated_type_bindings,
                ..
            } => {
                for arg in generic_args {
                    arg.unreflect_operands(out);
                }
                for binding in associated_type_bindings {
                    binding.ty.unreflect_operands(out);
                }
            }
            TypeExprKind::AssociatedTypeProjection {
                base, interface, ..
            } => {
                base.unreflect_operands(out);
                if let Some(interface) = interface {
                    interface.unreflect_operands(out);
                }
            }
            TypeExprKind::Optional { inner, .. } | TypeExprKind::List { inner, .. } => {
                inner.unreflect_operands(out);
            }
            TypeExprKind::Map { key, value, .. } => {
                key.unreflect_operands(out);
                value.unreflect_operands(out);
            }
            TypeExprKind::Union { variants, .. } => {
                for variant in variants {
                    variant.unreflect_operands(out);
                }
            }
            TypeExprKind::Function {
                params,
                ret,
                throws,
                ..
            } => {
                for param in params {
                    param.ty.unreflect_operands(out);
                }
                ret.unreflect_operands(out);
                if let Some(throws) = throws {
                    throws.unreflect_operands(out);
                }
            }
            _ => {}
        }
    }
}

impl TypeExprKind {
    /// Access the type-level attributes on this type expression.
    pub fn attrs(&self) -> &[RawAttribute] {
        match self {
            Self::Unreflect { attrs, .. }
            | Self::Path { attrs, .. }
            | Self::AssociatedTypeProjection { attrs, .. }
            | Self::Int { attrs }
            | Self::Bigint { attrs }
            | Self::Float { attrs }
            | Self::String { attrs }
            | Self::Bool { attrs }
            | Self::Null { attrs }
            | Self::Never { attrs }
            | Self::Void { attrs }
            | Self::Uint8Array { attrs }
            | Self::Media { attrs, .. }
            | Self::Optional { attrs, .. }
            | Self::List { attrs, .. }
            | Self::Map { attrs, .. }
            | Self::Union { attrs, .. }
            | Self::Literal { attrs, .. }
            | Self::Function { attrs, .. }
            | Self::Unknown { attrs }
            | Self::Type { attrs }
            | Self::Rust { attrs }
            | Self::Error { attrs }
            | Self::Missing { attrs }
            | Self::Infer { attrs } => attrs,
        }
    }

    /// Mutable access to the type-level attributes on this type expression.
    pub fn attrs_mut(&mut self) -> &mut Vec<RawAttribute> {
        match self {
            Self::Unreflect { attrs, .. }
            | Self::Path { attrs, .. }
            | Self::AssociatedTypeProjection { attrs, .. }
            | Self::Int { attrs }
            | Self::Bigint { attrs }
            | Self::Float { attrs }
            | Self::String { attrs }
            | Self::Bool { attrs }
            | Self::Null { attrs }
            | Self::Never { attrs }
            | Self::Void { attrs }
            | Self::Uint8Array { attrs }
            | Self::Media { attrs, .. }
            | Self::Optional { attrs, .. }
            | Self::List { attrs, .. }
            | Self::Map { attrs, .. }
            | Self::Union { attrs, .. }
            | Self::Literal { attrs, .. }
            | Self::Function { attrs, .. }
            | Self::Unknown { attrs }
            | Self::Type { attrs }
            | Self::Rust { attrs }
            | Self::Error { attrs }
            | Self::Missing { attrs }
            | Self::Infer { attrs } => attrs,
        }
    }
}

impl std::fmt::Display for TypeExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl std::fmt::Display for TypeExprKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn needs_parens(ty: &TypeExpr) -> bool {
            matches!(
                ty.kind,
                TypeExprKind::Union { .. } | TypeExprKind::Function { .. }
            )
        }

        fn write_postfix_base(f: &mut std::fmt::Formatter<'_>, ty: &TypeExpr) -> std::fmt::Result {
            if needs_parens(ty) {
                write!(f, "({ty})")
            } else {
                write!(f, "{ty}")
            }
        }

        match self {
            TypeExprKind::Unreflect { .. } => write!(f, "unreflect(…)"),
            TypeExprKind::Path {
                segments,
                generic_args,
                associated_type_bindings,
                ..
            } => {
                let path = segments
                    .iter()
                    .map(smol_str::SmolStr::as_str)
                    .collect::<Vec<_>>()
                    .join(".");
                write!(f, "{path}")?;
                if !generic_args.is_empty() || !associated_type_bindings.is_empty() {
                    write!(f, "<")?;
                    let mut first = true;
                    for arg in generic_args {
                        if !first {
                            write!(f, ", ")?;
                        }
                        first = false;
                        write!(f, "{arg}")?;
                    }
                    for binding in associated_type_bindings {
                        if !first {
                            write!(f, ", ")?;
                        }
                        first = false;
                        write!(f, "{} = {}", binding.name, binding.ty)?;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
            TypeExprKind::AssociatedTypeProjection {
                base,
                interface,
                member,
                ..
            } => {
                if let Some(interface) = interface {
                    write!(f, "({base} as {interface}).{member}")
                } else {
                    write_postfix_base(f, base)?;
                    write!(f, ".{member}")
                }
            }
            TypeExprKind::Int { .. } => write!(f, "int"),
            TypeExprKind::Bigint { .. } => write!(f, "bigint"),
            TypeExprKind::Float { .. } => write!(f, "float"),
            TypeExprKind::String { .. } => write!(f, "string"),
            TypeExprKind::Bool { .. } => write!(f, "bool"),
            TypeExprKind::Null { .. } => write!(f, "null"),
            TypeExprKind::Never { .. } => write!(f, "never"),
            TypeExprKind::Void { .. } => write!(f, "void"),
            TypeExprKind::Uint8Array { .. } => write!(f, "uint8array"),
            TypeExprKind::Media { kind, .. } => write!(f, "{}", format!("{kind:?}").to_lowercase()),
            TypeExprKind::Optional { inner, .. } => {
                write_postfix_base(f, inner)?;
                write!(f, "?")
            }
            TypeExprKind::List { inner, .. } => {
                write_postfix_base(f, inner)?;
                write!(f, "[]")
            }
            TypeExprKind::Map { key, value, .. } => write!(f, "map<{key}, {value}>"),
            TypeExprKind::Union { variants, .. } => {
                for (i, v) in variants.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    if matches!(v.kind, TypeExprKind::Function { .. }) {
                        write!(f, "({v})")?;
                    } else {
                        write!(f, "{v}")?;
                    }
                }
                Ok(())
            }
            TypeExprKind::Literal { value, .. } => write!(f, "{value}"),
            TypeExprKind::Function {
                params,
                ret,
                throws,
                ..
            } => {
                write!(f, "(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    if let Some(name) = &p.name {
                        let optional = if p.optional { "?" } else { "" };
                        write!(f, "{}{}: {}", name.as_str(), optional, p.ty)?;
                    } else {
                        write!(f, "{}", p.ty)?;
                    }
                }
                write!(f, ") -> ")?;
                if matches!(ret.kind, TypeExprKind::Function { .. }) {
                    write!(f, "({ret})")?;
                } else {
                    write!(f, "{ret}")?;
                }
                if let Some(throws) = throws {
                    write!(f, " throws {throws}")?;
                }
                Ok(())
            }
            TypeExprKind::Unknown { .. } => write!(f, "unknown"),
            TypeExprKind::Type { .. } => write!(f, "reflect.Type"),
            TypeExprKind::Rust { .. } => write!(f, "$rust_type"),
            TypeExprKind::Error { .. } => write!(f, "error"),
            TypeExprKind::Missing { .. } => write!(f, "?"),
            TypeExprKind::Infer { .. } => write!(f, "_"),
        }
    }
}

/// A parameter in a function type expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionTypeParam {
    pub name: Option<Name>,
    pub optional: bool,
    pub ty: TypeExpr,
}

/// Named associated type binding used inside type applications:
/// `Iterator<Item = int>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssociatedTypeBinding {
    pub name: Name,
    pub ty: Box<TypeExpr>,
}

/// A generic type parameter declaration, paired with the `&`-separated bounds
/// it was declared with (`<T>` → `bounds = []`; `<T extends A & B>` → `bounds =
/// [A, B]`).
///
/// The bound set is a **conjunction**: an argument for this parameter must
/// satisfy every entry. Holding the name and its bounds together makes a length
/// mismatch between the two unrepresentable.
///
/// Bounds are `TypeExpr`s so generic parents like `Container<int>` round-trip;
/// that each must denote an *interface* — never an interface-existential type,
/// see `TYPE_SYSTEM.md` "Generics on Functions" — is enforced where they are
/// lowered to constraints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericParam {
    pub name: Name,
    pub bounds: Vec<TypeExpr>,
}

// ── Expression Bodies ───────────────────────────────────────────
//
// Full expression/statement arena — modeled after the existing
// `ExprBody` in `body.rs`. All structural content is owned;
// spans are stored in a parallel `AstSourceMap`.

pub type ExprId = Idx<Expr>;
pub type StmtId = Idx<Stmt>;
pub type PatId = Idx<Pattern>;
pub type MatchArmId = Idx<MatchArm>;
pub type CatchArmId = Idx<CatchArm>;
pub type TypeAnnotId = Idx<TypeExpr>;

/// Full expression body — owned arena of expressions, statements,
/// and patterns. Modeled after `ExprBody` in `body.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprBody {
    pub exprs: Arena<Expr>,
    pub stmts: Arena<Stmt>,
    pub patterns: Arena<Pattern>,
    pub match_arms: Arena<MatchArm>,
    pub catch_arms: Arena<CatchArm>,
    /// Type annotations on let bindings etc.
    pub type_annotations: Arena<TypeExpr>,
    /// Root expression of the function body.
    pub root_expr: Option<ExprId>,
}

impl Default for ExprBody {
    fn default() -> Self {
        Self {
            exprs: Arena::new(),
            stmts: Arena::new(),
            patterns: Arena::new(),
            match_arms: Arena::new(),
            catch_arms: Arena::new(),
            type_annotations: Arena::new(),
            root_expr: None,
        }
    }
}

impl ExprBody {
    /// Render a short, human-readable representation of an expression.
    /// Used in diagnostic messages to show the user what they wrote.
    pub fn display_expr(&self, id: ExprId) -> String {
        self.display_expr_inner(id, 0)
    }

    fn display_expr_inner(&self, id: ExprId, depth: usize) -> String {
        if depth > 10 {
            return "...".to_string();
        }
        match &self.exprs[id] {
            Expr::Path(segments) => segments
                .iter()
                .map(smol_str::SmolStr::as_str)
                .collect::<Vec<_>>()
                .join("."),
            Expr::GenericApply { base, type_args } => {
                let base = self.display_expr_inner(*base, depth + 1);
                let tys: Vec<String> = type_args.iter().map(ToString::to_string).collect();
                format!("{base}<{}>", tys.join(", "))
            }
            Expr::MemberAccess { base, member } => {
                format!("{}.{member}", self.display_expr_inner(*base, depth + 1))
            }
            Expr::OptionalMemberAccess { base, member } => {
                format!("{}?.{member}", self.display_expr_inner(*base, depth + 1))
            }
            Expr::Upcast { base, target } => {
                format!("{}.as<{target}>", self.display_expr_inner(*base, depth + 1))
            }
            Expr::QualifiedPath {
                qself,
                interface,
                member,
            } => format!("({qself} as {interface}).{member}"),
            Expr::Index { base, index } => {
                format!(
                    "{}[{}]",
                    self.display_expr_inner(*base, depth + 1),
                    self.display_expr_inner(*index, depth + 1)
                )
            }
            Expr::OptionalIndex { base, index } => {
                format!(
                    "{}?.[{}]",
                    self.display_expr_inner(*base, depth + 1),
                    self.display_expr_inner(*index, depth + 1)
                )
            }
            Expr::Call {
                callee,
                type_args,
                args,
            } => {
                let ty_args_str = if type_args.is_empty() {
                    String::new()
                } else {
                    let tys: Vec<_> = type_args.iter().map(ToString::to_string).collect();
                    format!("<{}>", tys.join(", "))
                };
                let args_str: Vec<_> = args
                    .iter()
                    .map(|a| {
                        let value = self.display_expr_inner(a.expr, depth + 1);
                        match &a.label {
                            Some(label) => format!("{label} = {value}"),
                            None => value,
                        }
                    })
                    .collect();
                format!(
                    "{}{}({})",
                    self.display_expr_inner(*callee, depth + 1),
                    ty_args_str,
                    args_str.join(", ")
                )
            }
            Expr::OptionalCall { callee, args } => {
                let args_str: Vec<_> = args
                    .iter()
                    .map(|a| {
                        let value = self.display_expr_inner(a.expr, depth + 1);
                        match &a.label {
                            Some(label) => format!("{label} = {value}"),
                            None => value,
                        }
                    })
                    .collect();
                format!(
                    "{}?.({})",
                    self.display_expr_inner(*callee, depth + 1),
                    args_str.join(", ")
                )
            }
            Expr::Binary { op, lhs, rhs } => {
                format!(
                    "{} {op} {}",
                    self.display_expr_inner(*lhs, depth + 1),
                    self.display_expr_inner(*rhs, depth + 1)
                )
            }
            Expr::OptionalChain { expr } => self.display_expr_inner(*expr, depth + 1),
            Expr::Block {
                tail_expr: Some(tail),
                ..
            } => self.display_expr_inner(*tail, depth + 1),
            Expr::Template { tag, .. } => match tag {
                TemplateTag::Default { .. } => "`…`".to_string(),
                TemplateTag::Custom { tag, .. } => {
                    format!("{}`…`", self.display_expr_inner(*tag, depth + 1))
                }
            },
            Expr::Literal(lit) => lit.to_string(),
            Expr::Null => "null".to_string(),
            _ => "...".to_string(),
        }
    }
}

/// Parallel span storage for an `ExprBody` — maps arena IDs to source ranges.
/// Separated so semantic queries (type checking) can ignore spans and get
/// Salsa early-cutoff on whitespace changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstSourceMap {
    pub expr_spans: Arena<TextRange>,
    pub stmt_spans: Arena<TextRange>,
    pub pattern_spans: Arena<TextRange>,
    pub match_arm_spans: Arena<TextRange>,
    pub type_annotation_spans: Arena<TextRange>,
    pub catch_arm_spans: Arena<TextRange>,
    /// For `MemberAccess` expressions, the span of just the member name (after the dot).
    pub member_access_member_spans: HashMap<ExprId, TextRange>,
    /// For multi-segment `Path` expressions, per-segment spans.
    /// `path_segment_spans[expr_id][i]` is the `TextRange` of `segments[i]`.
    pub path_segment_spans: HashMap<ExprId, Vec<TextRange>>,
    /// For labeled call arguments, the span of the label name keyed by
    /// `(call_expr_id, argument_expr_id)`.
    pub call_arg_label_spans: HashMap<(ExprId, ExprId), TextRange>,
    /// For object-constructor fields, the span of the field name keyed by
    /// `(object_expr_id, value_expr_id)`.
    pub object_field_name_spans: HashMap<(ExprId, ExprId), TextRange>,
    /// For `unreflect(value)` type-argument slots, the span of the WHOLE slot
    /// (marker, parens and all), keyed by the carrier expression inside it.
    /// The carrier's own span covers only `value`, so diagnostics about the
    /// slot itself would otherwise have no range to point at.
    pub unreflect_arg_spans: HashMap<ExprId, TextRange>,
    /// Ids of compiler-synthesized nodes — desugarings that have no
    /// user-written source of their own (e.g. the `string.from(${…})` wrapper
    /// and the concat accumulator that backtick interpolation lowers to). Their
    /// spans still point at the originating source (the `${…}` template) so
    /// diagnostics land sensibly, but consumers like inlay hints use these sets
    /// to tell "the user wrote this" from "the compiler generated it" — a
    /// uniform replacement for fragile structural heuristics (e.g. comparing a
    /// call's span to its callee's). Populated at the `alloc_*` chokepoints
    /// during lowering, via a scoped "synthesizing" flag.
    pub synthetic_exprs: HashSet<ExprId>,
    pub synthetic_stmts: HashSet<StmtId>,
    pub synthetic_patterns: HashSet<PatId>,
}

impl AstSourceMap {
    pub fn new() -> Self {
        Self {
            expr_spans: Arena::new(),
            stmt_spans: Arena::new(),
            pattern_spans: Arena::new(),
            match_arm_spans: Arena::new(),
            type_annotation_spans: Arena::new(),
            catch_arm_spans: Arena::new(),
            member_access_member_spans: HashMap::new(),
            path_segment_spans: HashMap::new(),
            call_arg_label_spans: HashMap::new(),
            object_field_name_spans: HashMap::new(),
            unreflect_arg_spans: HashMap::new(),
            synthetic_exprs: HashSet::new(),
            synthetic_stmts: HashSet::new(),
            synthetic_patterns: HashSet::new(),
        }
    }

    /// Whether `id` names a compiler-synthesized expression (see `synthetic_exprs`).
    pub fn is_synthetic_expr(&self, id: ExprId) -> bool {
        self.synthetic_exprs.contains(&id)
    }

    /// Whether `id` names a compiler-synthesized statement (see `synthetic_stmts`).
    pub fn is_synthetic_stmt(&self, id: StmtId) -> bool {
        self.synthetic_stmts.contains(&id)
    }

    /// Look up a span in an arena that is index-parallel to the arena `id`
    /// indexes.
    ///
    /// Every `alloc_*` in lowering pushes the node and its span together, so the
    /// two arenas always have matching indices and this is a direct index rather
    /// than a search. An out-of-range id means `id` came from a *different*
    /// arena — a parameter-default id used against a body's map, say. That
    /// yields an empty range rather than a panic: this runs in the LSP, which is
    /// compiled to wasm, where a panic aborts the whole runtime.
    fn span_at<U>(spans: &Arena<TextRange>, id: Idx<U>) -> TextRange {
        let raw = id.into_raw();
        if (raw.into_u32() as usize) < spans.len() {
            spans[Idx::from_raw(raw)]
        } else {
            TextRange::default()
        }
    }

    /// Look up the source span of a statement by its `StmtId`.
    ///
    /// The `stmt_spans` arena is parallel to `ExprBody::stmts` — same indices,
    /// different element type. We convert via raw index.
    pub fn stmt_span(&self, id: StmtId) -> TextRange {
        Self::span_at(&self.stmt_spans, id)
    }

    /// Look up the source span of an expression by its `ExprId`.
    pub fn expr_span(&self, id: ExprId) -> TextRange {
        Self::span_at(&self.expr_spans, id)
    }

    /// Look up the member-name span for a `MemberAccess` expression.
    /// Returns the full expression span as fallback if no member span was recorded.
    pub fn member_access_member_span(&self, id: ExprId) -> TextRange {
        self.member_access_member_spans
            .get(&id)
            .copied()
            .unwrap_or_else(|| self.expr_span(id))
    }

    /// Look up the per-segment spans for a multi-segment `Path` expression.
    /// `path_segment_span(id, i)` returns the `TextRange` of `segments[i]`.
    /// Returns the full expression span as fallback if no segment span was recorded.
    pub fn path_segment_span(&self, id: ExprId, segment_idx: usize) -> TextRange {
        self.path_segment_spans
            .get(&id)
            .and_then(|spans| spans.get(segment_idx).copied())
            .unwrap_or_else(|| self.expr_span(id))
    }

    /// Look up the field-name span for an object-constructor field.
    /// Returns the value-expression span as fallback.
    pub fn object_field_name_span(&self, object_id: ExprId, value_id: ExprId) -> TextRange {
        self.object_field_name_spans
            .get(&(object_id, value_id))
            .copied()
            .unwrap_or_else(|| self.expr_span(value_id))
    }

    /// Look up the span of the `unreflect(...)` type-argument slot whose
    /// carrier expression is `id`. Falls back to the carrier's own span when
    /// the slot was not recorded (a synthesized marker, for instance).
    pub fn unreflect_arg_span(&self, id: ExprId) -> TextRange {
        self.unreflect_arg_spans
            .get(&id)
            .copied()
            .unwrap_or_else(|| self.expr_span(id))
    }

    /// Look up the source span of a pattern by its `PatId`.
    pub fn pattern_span(&self, id: PatId) -> TextRange {
        Self::span_at(&self.pattern_spans, id)
    }

    /// Look up the source span of a match arm by its `MatchArmId`.
    pub fn match_arm_span(&self, id: MatchArmId) -> TextRange {
        Self::span_at(&self.match_arm_spans, id)
    }

    /// Look up the source span of a type annotation by its `TypeAnnotId`.
    pub fn type_annotation_span(&self, id: TypeAnnotId) -> TextRange {
        Self::span_at(&self.type_annotation_spans, id)
    }

    /// Look up the source span of a catch arm by its `CatchArmId`.
    pub fn catch_arm_span(&self, id: CatchArmId) -> TextRange {
        Self::span_at(&self.catch_arm_spans, id)
    }
}

impl Default for AstSourceMap {
    fn default() -> Self {
        Self::new()
    }
}

/// How a property value was written in source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertySyntax {
    /// An explicit key/value pair such as `{ "name": value }` or
    /// `Config { name: value }`.
    Explicit,
    /// A shorthand property such as `{ name }` or `Config { name }`.
    Shorthand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectExprField {
    pub name: Name,
    pub value: ExprId,
    pub syntax: PropertySyntax,
}

impl ObjectExprField {
    pub fn explicit(name: Name, value: ExprId) -> Self {
        Self {
            name,
            value,
            syntax: PropertySyntax::Explicit,
        }
    }

    pub fn shorthand(name: Name, value: ExprId) -> Self {
        Self {
            name,
            value,
            syntax: PropertySyntax::Shorthand,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapExprEntry {
    pub key: ExprId,
    pub value: ExprId,
    pub syntax: PropertySyntax,
}

impl MapExprEntry {
    pub fn explicit(key: ExprId, value: ExprId) -> Self {
        Self {
            key,
            value,
            syntax: PropertySyntax::Explicit,
        }
    }

    pub fn shorthand(key: ExprId, value: ExprId) -> Self {
        Self {
            key,
            value,
            syntax: PropertySyntax::Shorthand,
        }
    }
}

/// Expressions — modeled after `Expr` in `body.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Literal(Literal),
    /// Byte string literal: `b"hello"`, `b"\x00\xFF"`.
    /// Stores the fully-resolved bytes (escape sequences already processed).
    ByteStringLiteral(Vec<u8>),
    Null,
    /// Path expression: `x`, `user.name`, `Status.Active`
    Path(Vec<Name>),
    /// Generic instantiation as a value: `foo<int>` — a generic callable
    /// referenced with explicit type arguments but NOT called. The result is
    /// the specialized function value (`(int) -> int`). Distinct from
    /// `Call { type_args, .. }`, which applies type args *and* invokes.
    GenericApply {
        base: ExprId,
        /// Explicit type arguments, e.g. the `<int>` in `foo<int>`. Never empty
        /// (a bare path lowers to `Path`, not `GenericApply`).
        type_args: Vec<TypeExpr>,
    },
    If {
        condition: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
    },
    /// `if let PATTERN = SCRUTINEE { THEN } else { ELSE }` — refutable
    /// pattern match in condition position. Bindings introduced by `pattern`
    /// are in scope inside `then_branch` only (never in `else_branch` and
    /// never after the `if let`). Unlike `Stmt::Let`, the pattern is
    /// expected to be *refutable*; an irrefutable pattern earns a warning.
    IfLet {
        pattern: PatId,
        scrutinee: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
    },
    Match {
        scrutinee: ExprId,
        scrutinee_type: Option<TypeAnnotId>,
        arms: Vec<MatchArmId>,
    },
    /// `<expr> is <pattern>` — Rust `matches!`-style pattern test.
    ///
    /// Always evaluates to `bool`: `true` if the scrutinee matches the
    /// pattern, `false` otherwise. Unlike `Match`, a pattern that cannot
    /// match the scrutinee's static type is **not** a compile error here —
    /// it just always evaluates to `false`. Treat it as a one-arm
    /// pattern-test, not as an exhaustive match.
    Is {
        scrutinee: ExprId,
        pattern: PatId,
    },
    Catch {
        base: ExprId,
        clauses: Vec<CatchClause>,
    },
    Throw {
        value: ExprId,
    },
    /// `return expr?` in expression position — a diverging expression of type
    /// `never`, mirroring [`Expr::Throw`]. Lets `return` be a `catch`/`match`
    /// arm value. The value is optional (`None` for a bare `return`), matching
    /// [`Stmt::Return`]. Control transfer is to the enclosing function's exit,
    /// not the surrounding `catch`.
    Return {
        value: Option<ExprId>,
    },
    /// BEP-034 `spawn name_expr? (with expr (, expr)*)? { body }`. The body is
    /// always a block expression that runs on a freshly-spawned green thread;
    /// the optional `name` is any expression that evaluates to a string and
    /// surfaces in debug / stack traces.
    Spawn {
        /// Optional human-readable label for the spawn.
        name: Option<ExprId>,
        /// BEP-034 spawn options: the `with expr (, expr)*` clause. Each entry
        /// is an arbitrary expression; in v1 TIR requires exactly one, a call
        /// to `baml.spawn.options(...)`. Empty when there is no `with` clause.
        with_exprs: Vec<ExprId>,
        /// Body of the spawn (`{...}`) — always an `Expr::Block` after
        /// CST lowering.
        body: ExprId,
    },
    /// BEP-034 `await expr` — prefix form. Suspends the current thread
    /// until `expr`'s future settles, then unwraps the value or re-throws
    /// the future's error.
    Await {
        future: ExprId,
    },
    Binary {
        op: BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
    },
    Unary {
        op: UnaryOp,
        expr: ExprId,
    },
    Call {
        callee: ExprId,
        /// Explicit type arguments at the call site, e.g. `foo<int, string>(x)`.
        /// Empty vec when no `<...>` was written.
        type_args: Vec<TypeExpr>,
        args: Vec<CallArg>,
    },
    Object {
        /// The constructed type's name. Always present: the parser only emits an
        /// object literal when a type name precedes the brace
        /// (`looks_like_object_constructor`), so a name-less `{ .. }` is a map or
        /// block, never an object literal.
        type_name: TypePath,
        /// Explicit generic type args from syntax like `Foo<int> { ... }`.
        /// Empty when no `<...>` was written (e.g. bare `Foo { ... }`).
        type_args: Vec<TypeExpr>,
        fields: Vec<ObjectExprField>,
        spreads: Vec<SpreadField>,
    },
    Array {
        elements: Vec<ExprId>,
    },
    Map {
        entries: Vec<MapExprEntry>,
    },
    Block {
        stmts: Vec<StmtId>,
        tail_expr: Option<ExprId>,
    },
    // These nodes are constructed purely in the HIR layer AFTER
    // name resolution as we can't know if it's a member access
    // until we know how to resolve the path
    MemberAccess {
        base: ExprId,
        member: Name,
    },
    /// Explicit static projection/upcast: `expr.as<T>`.
    Upcast {
        base: ExprId,
        target: TypeExpr,
    },
    /// Fully-qualified item reference: `(Base as Interface).item`.
    ///
    /// The one spelling that pins BOTH halves of the `(Self type, interface,
    /// item)` triple. `Base.item` and `Interface.item` denote the same triple
    /// with one half left to inference and stay ordinary [`Expr::Path`]s —
    /// the three forms unify in resolution, not in syntax, exactly as
    /// rustc's `<T as Trait>::item` / `T::item` / `Trait::item` do.
    ///
    /// Neither half is an expression: `qself` is a type and `interface` names
    /// an interface, so there is no base [`ExprId`] to traverse.
    QualifiedPath {
        qself: TypeExpr,
        interface: TypeExpr,
        member: Name,
    },
    /// Optional member access: `obj?.member` — short-circuits to null if base is null.
    OptionalMemberAccess {
        base: ExprId,
        member: Name,
    },
    Index {
        base: ExprId,
        index: ExprId,
    },
    /// Lambda expression: anonymous function in expression position.
    /// Reuses `FunctionDef` with synthetic name `"<anonymous function>"`.
    /// The lambda's body gets its own `ExprBody` via `FunctionBodyDef::Expr`.
    Lambda(Box<LambdaDef>),
    /// Optional index: `obj?.[expr]` — short-circuits to null if base is null.
    OptionalIndex {
        base: ExprId,
        index: ExprId,
    },
    /// Optional call: `func?.(args)` — short-circuits to null if callee is null.
    OptionalCall {
        callee: ExprId,
        args: Vec<CallArg>,
    },
    /// Wraps an expression chain containing `?.` operators.
    /// Delimits the scope of null short-circuiting.
    /// If any `?.` inside encounters null, the entire `OptionalChain` evaluates to null.
    OptionalChain {
        expr: ExprId,
    },
    /// Backtick template literal site (BEP-049). Held as a first-class HIR
    /// node through TIR so type checking applies template-aware rules with
    /// errors pointing at the original `${…}` spans, and MIR owns the
    /// lowering. The `tag` discriminates the two BEP forms:
    ///
    /// - [`TemplateTag::Default`] — an untagged `` `…` `` literal (§11):
    ///   each `${expr}` is implicitly `.to_string()`-coerced (strict — a
    ///   nullable / non-stringable value is a compile error), and the whole
    ///   template evaluates to `string`. MIR lowers it to a concat chain.
    /// - [`TemplateTag::Custom`] — a tagged `` tag`…` `` literal (§10): the
    ///   tag's body parameter brings extra bindings into scope, values are
    ///   passed to the tag *verbatim* with their original types, and the
    ///   result is the tag fn's return type. MIR lowers it to
    ///   `tag(body = (...) -> TaggedString { TaggedString { parts, values } })`.
    Template {
        /// Which BEP form this is — see [`TemplateTag`].
        tag: TemplateTag,
        /// Structured template body. Mirrors `BacktickSegment` from the
        /// CST but each interp/condition/collection is already a lowered
        /// `ExprId`, and for-bindings are lowered `PatId`s. Lets TIR walk
        /// the tree without re-touching the CST. Interp payloads are the
        /// *raw* inner expressions (no `.to_string()` wrapping); coercion
        /// is MIR's job for the `Default` form.
        segments: Vec<TemplateSegment>,
    },
    Missing,
}

/// Which BEP-049 backtick form an [`Expr::Template`] is, plus the per-form
/// payload needed to realize it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateTag {
    /// Untagged `` `…` `` (BEP §11): implicit per-value `.to_string()`,
    /// result type `string`.
    ///
    /// `elaborated` is the desugared realization — a left-folded `+` concat
    /// of the segments (text literals, `${expr}.to_string()`, `${for}`
    /// accumulator blocks, `${if}` chains), built from the *same* lowered
    /// `ExprId`s the `segments` hold. TIR types this for codegen and HIR/MIR
    /// consume it directly; the structured `segments` exist only so TIR can
    /// emit per-`${…}` strict-stringify diagnostics (BEP §11) on the original
    /// spans rather than on the synthetic `.to_string()` calls.
    Default { elaborated: ExprId },
    /// Tagged `` tag`…` `` (BEP §10): `tag` is the tag expression — usually a
    /// bare identifier referring to a fn marked `//baml:tagged_string`. Stored
    /// as an `ExprId` so paths and future curry forms compose without grammar
    /// changes. Values pass to the tag verbatim with their original types.
    ///
    /// `body` is the desugared closure body the tag is invoked with — a block
    /// that flattens the segments into `baml.TaggedString { parts, values }`
    /// (text runs concatenated into `parts`, each `${expr}` pushed raw into
    /// `values`, with `${for}`/`${if}` driving runtime array growth). Built
    /// from the *same* lowered `ExprId`s the `segments` hold. TIR types it (so
    /// MIR has the `push`/aggregate resolutions) and MIR lowers it as the
    /// hand-rolled `body` closure — except when the template is purely static
    /// (text + interp, no `${for}`/`${if}`), where MIR keeps a fixed-array
    /// fast-path off `segments` instead.
    Custom { tag: ExprId, body: ExprId },
}

/// One segment of an [`Expr::Template`] body. Parallel to `BacktickSegment`
/// in the CST layer, but every sub-expression is already lowered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateSegment {
    /// Literal text between interpolations / block tags.
    Text(std::string::String),
    /// A `${expr}` interpolation. The wrapped `ExprId` is the lowered
    /// inner expression (already a block expression per BEP §4).
    Interp(ExprId),
    /// A `${for (let p in c)}...${endfor}` block (iterator form).
    For {
        binding: PatId,
        collection: ExprId,
        body: Vec<TemplateSegment>,
    },
    /// A C-style `${for (let i = 0; cond; step)}...${endfor}` block (BEP §4 —
    /// the host `for` headers are reused verbatim, so the template form accepts
    /// the C-style header too). `init` declares the loop variable (a
    /// `Stmt::Let`); `step` is the per-iteration update (an assignment stmt),
    /// absent only for `for (init; cond; )`. Elaborates to the same
    /// `{ init; while cond { body } after { step } }` shape the host C-style
    /// `for` lowers to (`lower_c_style_for`).
    CStyleFor {
        init: StmtId,
        cond: ExprId,
        step: Option<StmtId>,
        body: Vec<TemplateSegment>,
    },
    /// A `${if (c)}...${else if (c)}...${else}...${endif}` chain.
    If {
        branches: Vec<TemplateIfBranch>,
        else_body: Option<Vec<TemplateSegment>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateIfBranch {
    pub condition: ExprId,
    pub body: Vec<TemplateSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallArg {
    pub label: Option<Name>,
    pub expr: ExprId,
}

impl CallArg {
    pub fn positional(expr: ExprId) -> Self {
        Self { label: None, expr }
    }

    pub fn named(label: impl Into<Name>, expr: ExprId) -> Self {
        Self {
            label: Some(label.into()),
            expr,
        }
    }
}

/// Statements — modeled after `Stmt` in `body.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Expr(ExprId),
    /// Evaluate a runtime `type` value once and bind its exact identity to a
    /// lexical type parameter for the remainder of the enclosing block.
    TypeBinding {
        name: Name,
        value: TypeExpr,
    },
    Let {
        /// The binding pattern. A `: T` annotation lives inside the pattern
        /// as the bind's sub-pattern slot, not as a separate field on
        /// `Stmt::Let` — see [`Pattern::Bind`].
        pattern: PatId,
        initializer: Option<ExprId>,
        origin: LetOrigin,
        /// `let PATTERN = init else { … };` — refutable binding with a
        /// diverging else clause. `Some` activates let-else semantics:
        /// the pattern may be refutable, and the else expression is
        /// required to have type `Ty::Never`. Pattern bindings flow into
        /// the enclosing scope on a successful match.
        else_branch: Option<ExprId>,
    },
    While {
        condition: ExprId,
        body: ExprId,
        after: Option<StmtId>,
        origin: LoopOrigin,
    },
    /// `while let PATTERN = SCRUTINEE { BODY }` — loops as long as the
    /// refutable `pattern` matches `scrutinee`. Bindings introduced by
    /// `pattern` are in scope inside `body` only and are re-bound each
    /// iteration. Exits when the pattern fails to match. Like `Stmt::While`
    /// it produces no value (unit) and supports `break`/`continue`. Unlike
    /// `Stmt::Let`, the pattern is expected to be *refutable*; an irrefutable
    /// pattern earns a downstream warning. Has no `else` clause and (unlike
    /// `Stmt::While`) no `after`/`origin` — those exist only for desugared
    /// C-style `for` loops.
    WhileLet {
        pattern: PatId,
        scrutinee: ExprId,
        body: ExprId,
    },
    /// For-in loop: `for let <binding> in <collection> { <body> }`.
    ///
    /// Kept as a first-class node (not desugared to While) so that:
    /// - TIR can produce for-loop-specific diagnostics ("cannot iterate over X")
    /// - Codegen can emit native for-loops in target languages (TS/Python/Rust)
    /// - The user's intent is preserved through the pipeline
    ///
    /// Desugaring to index-based iteration happens at MIR lowering time.
    For {
        /// The loop variable binding pattern (e.g. `i` in `for let i in xs`).
        binding: PatId,
        /// The collection expression to iterate over.
        collection: ExprId,
        /// The loop body expression.
        body: ExprId,
    },
    Return(Option<ExprId>),
    Throw {
        value: ExprId,
    },
    Break,
    Continue,
    /// `defer { BODY }` (BEP-042). Schedules `body` (an [`Expr::Block`]) to run
    /// on every exit of the enclosing block — normal completion, `return`,
    /// `break`/`continue`, and error unwinding — in LIFO order. Block-scoped.
    /// The body reads the live enclosing scope at exit (it is NOT a closure
    /// capturing values at the `defer` site). `return`/`break`/`continue` that
    /// would escape the body are rejected in TIR; `throw` is allowed.
    Defer {
        body: ExprId,
    },
    Assign {
        target: ExprId,
        value: ExprId,
    },
    AssignOp {
        target: ExprId,
        op: AssignOp,
        value: ExprId,
    },
    Missing,
    HeaderComment {
        name: Name,
        level: usize,
    },
}

/// A pattern in the AST.
///
/// One flat enum. Atoms (`Wildcard`, `Bind`, `Class`, `Type`) describe a
/// single shape; the only combinator (`Or`) combines other patterns.
///
/// Lowering invariants:
/// - `_` always lowers to [`Pattern::Wildcard`] — never `Bind { name: "_" }`.
/// - 1-element `Or` is NOT allocated; it collapses to the inner pattern.
///   So if you see `Or(parts)`, `parts.len() >= 2`.
/// - The `: T` annotation in `let x: T` is carried as the bind's `subpat`
///   slot (see [`Pattern::Bind`]) and likewise as `Pattern::Array`'s
///   `ascription` field for `[…]: T`. `:` is only valid after `let x` or
///   `[…]` — it is rejected on `_`, `Class { … }`, bare types, and
///   Or-patterns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    // ── Atoms (single-shape patterns) ────────────────────────────────────
    /// `_` — wildcard. Always irrefutable. Binds nothing. Cannot carry a
    /// type ascription.
    Wildcard,
    /// `let x`, `let x: T`, `let x: [a, b]`, `let x: let y: T` — name
    /// binding, optionally with a sub-pattern attached via `:`. The
    /// sub-pattern can be any pattern: a type ascription
    /// (`Pattern::Type`), another binding (`Pattern::Bind` — chains of
    /// aliases like `let x: let y`), a structural destructure
    /// (`Pattern::Array`, `Pattern::Class`), or anything else.
    /// Progressive widening like `let x: int: float` is naturally
    /// impossible because `Pattern::Type` doesn't itself have a sub-
    /// pattern slot.
    Bind { name: Name, subpat: Option<PatId> },
    /// `pkg.Foo { a, b: <pat>, ... }` — class destructure. `class` is the
    /// dotted path as segments (single-element vec for unqualified names).
    /// Class destructures cannot carry `: T` ascriptions.
    Class {
        class: Vec<Name>,
        generic_args: Vec<TypeExpr>,
        associated_type_bindings: Vec<AssociatedTypeBinding>,
        fields: Vec<FieldPat>,
    },
    /// `[prefix..., ..rest?, suffix...]` or `[…]: T` — array destructure
    /// optionally with a type ascription. Each element is a normal
    /// pattern; `rest` binds the copied middle slice when present. The
    /// `: T` ascription is captured as a `TypeExpr` (not a sub-pattern),
    /// so deeper chains like `[…]: T1: T2` and exotic shapes like
    /// `[…]: let xs` are syntactically rejected at AST lowering.
    Array {
        prefix: Vec<PatId>,
        rest: Option<ArrayRestPat>,
        suffix: Vec<PatId>,
        ascription: Option<TypeExpr>,
    },
    /// Bare type expression in pattern position. Subsumes literal patterns
    /// (`42`, `"hi"`, `true`), `null`, enum variants (`Status.Active`),
    /// path types, generics, function types, etc. — anything in `TypeExpr`.
    /// Refutability is decided by TIR using the scrutinee type; `Type(int)`
    /// is irrefutable against scrutinee `int` but refutable against `int|str`.
    /// Cannot carry a `: T` ascription.
    Type(TypeExpr),
    /// `unreflect(expr)` — identity-filter against a runtime minted type.
    /// This pattern narrows no static shape; its operand is checked as `type`.
    Unreflect(ExprId),

    // ── Combinators (combine other patterns) ─────────────────────────────
    /// `p1 | p2 | ...` — alternation. Length always `>= 2`. Every alternative
    /// must bind the same names (TIR enforces). Cannot carry a `: T`
    /// ascription.
    Or(Vec<PatId>),
}

/// Single field inside a class destructure pattern.
///
/// Shorthand `{ f }` lowers to `FieldPat { field: f, pat: <Bind { name: f }> }`,
/// so consumers never see the missing-pattern shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldPat {
    pub field: Name,
    pub field_span: text_size::TextRange,
    pub pat: PatId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayRestPat {
    pub pat: Option<PatId>,
}

impl Pattern {
    /// First name introduced by this pattern, if any. Convenience wrapper
    /// around [`Pattern::bound_names`] for callers that just need a single
    /// representative — e.g. inlay-hint anchors, debug names, "does this
    /// pattern introduce *some* binding?" checks.
    ///
    /// For patterns with multiple bindings (like `let x: let y = 1` or
    /// destructures), use [`Pattern::bound_names`] instead.
    pub fn binding_name<'a>(&'a self, patterns: &'a la_arena::Arena<Pattern>) -> Option<&'a Name> {
        self.bound_names(patterns).into_iter().next()
    }

    /// Collect every name this pattern introduces into scope, in declaration
    /// order. Walks down through chains, fields, and Or-branches recursively.
    ///
    /// Used by HIR to:
    ///   - register bindings into the surrounding scope, and
    ///   - check that every alternative of an `Or` introduces the same name
    ///     set (otherwise the body would see a name that's only sometimes in
    ///     scope).
    ///
    /// All links of a `Chain` can bind. `let x: let y: let z = 1` is a valid
    /// pattern (pairwise `never <: never`), so all three names land in scope.
    ///
    /// For an `Or` pattern, this returns the names of the *first* alternative.
    /// HIR's uniformity check compares sibling alternatives' name lists; pick
    /// any branch as the reference and diff the rest.
    pub fn bound_names<'a>(&'a self, patterns: &'a la_arena::Arena<Pattern>) -> Vec<&'a Name> {
        let mut out = Vec::new();
        self.collect_bound_names(patterns, &mut out);
        out
    }

    fn collect_bound_names<'a>(
        &'a self,
        patterns: &'a la_arena::Arena<Pattern>,
        out: &mut Vec<&'a Name>,
    ) {
        match self {
            Pattern::Wildcard | Pattern::Type(_) | Pattern::Unreflect(_) => {}
            Pattern::Bind { name, subpat } => {
                out.push(name);
                if let Some(sp) = subpat {
                    patterns[*sp].collect_bound_names(patterns, out);
                }
            }
            Pattern::Class { fields, .. } => {
                for f in fields {
                    patterns[f.pat].collect_bound_names(patterns, out);
                }
            }
            Pattern::Array {
                prefix,
                rest,
                suffix,
                ascription: _,
            } => {
                for id in prefix {
                    patterns[*id].collect_bound_names(patterns, out);
                }
                if let Some(rest) = rest
                    && let Some(id) = rest.pat
                {
                    patterns[id].collect_bound_names(patterns, out);
                }
                for id in suffix {
                    patterns[*id].collect_bound_names(patterns, out);
                }
            }
            // Pick any one alternative to report — the uniformity check is
            // the caller's job.
            Pattern::Or(parts) => {
                if let Some(first) = parts.first() {
                    patterns[*first].collect_bound_names(patterns, out);
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: PatId,
    pub guard: Option<ExprId>,
    pub body: ExprId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CatchClauseKind {
    Catch,
    CatchAll,
    CatchAllPanics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatchClause {
    pub kind: CatchClauseKind,
    pub binding: PatId,
    /// Optional second binding for the stack trace: `catch (e, st) { ... }`
    pub stack_trace_binding: Option<PatId>,
    pub arms: Vec<CatchArmId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatchArm {
    pub pattern: PatId,
    pub body: ExprId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpreadField {
    pub expr: ExprId,
    pub position: usize,
}

/// Re-export `baml_base::Literal` as the canonical literal type.
pub type Literal = baml_base::Literal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LetOrigin {
    Source,
    Client,
    RetryPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionOrigin {
    UserDefined,
    Companion,
    Internal,
    /// Synthesized by the auto-derive pass (e.g. `to_json` / `from_json`
    /// methods generated on every user class).
    AutoDerive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FunctionMetadata {
    pub origin: FunctionOrigin,
    /// Marks compiler/runtime implementation details that are outside BAML's
    /// user-facing language surface.
    ///
    /// Language-internal functions are omitted from `baml describe` and other
    /// language-surface visibility views. This is independent of any future
    /// user-declared `pub`/`priv` access-control semantics.
    pub is_language_internal: bool,
}

impl FunctionMetadata {
    pub const fn user_facing(origin: FunctionOrigin) -> Self {
        Self {
            origin,
            is_language_internal: false,
        }
    }

    pub const fn language_internal(origin: FunctionOrigin) -> Self {
        Self {
            origin,
            is_language_internal: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopOrigin {
    While,
    For,
}

/// Binary operators — matches those supported in `body.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    /// Null coalescing: `a ?? b` — returns `a` if non-null, else `b`.
    NullCoalesce,
}

impl std::fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Mod => "%",
            BinaryOp::Eq => "==",
            BinaryOp::Ne => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Le => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::Ge => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
            BinaryOp::BitAnd => "&",
            BinaryOp::BitOr => "|",
            BinaryOp::BitXor => "^",
            BinaryOp::Shl => "<<",
            BinaryOp::Shr => ">>",
            BinaryOp::NullCoalesce => "??",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
}

impl std::fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            UnaryOp::Not => "!",
            UnaryOp::Neg => "-",
        };
        write!(f, "{s}")
    }
}

/// Compound assignment operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

// ── Top-Level Items ─────────────────────────────────────────────

/// Top-level item — the output unit of CST → AST lowering.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Function(FunctionDef),
    Class(ClassDef),
    Enum(EnumDef),
    Interface(InterfaceDef),
    TypeAlias(TypeAliasDef),
    Client(ClientDef),
    TemplateString(TemplateStringDef),
    RetryPolicy(RetryPolicyDef),
    Let(LetDef),
    ImplementsFor(ImplementsForDef),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclarativeMeta {
    /// LLM function metadata (client name, prompt template).
    /// Present only for functions declared with `{ client ...; prompt ... }` syntax.
    /// The body is desugared to a synthetic `Expr` that constructs an
    /// `ai.FunctionSpec` and runs it through `ai.Agent`, while this field
    /// preserves the original declaration metadata.
    Llm(LlmBodyDef),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDef {
    pub name: Name,
    /// Generic type parameters (e.g., `["T", "U"]`). Empty for non-generic functions.
    pub generic_params: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub defaults: FunctionDefaults,
    pub return_type: Option<TypeExpr>,
    pub throws: Option<TypeExpr>,
    pub body: Option<FunctionBodyDef>,
    pub declarative_meta: Option<DeclarativeMeta>,
    pub metadata: FunctionMetadata,
    pub attributes: Vec<RawAttribute>,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<std::string::String>,
    /// True when this fn is preceded by a `//baml:tagged_string` marker
    /// comment. BEP-049 §10: such fns are callable as tagged template
    /// tags (a tag name immediately followed by a backtick literal), and
    /// their first parameter must be `body: (...) -> TaggedString`.
    pub is_tagged_template_tag: bool,
    pub span: TextRange,
    pub name_span: TextRange,
}

/// What produced a [`LambdaDef`], where that changes how TIR types it.
///
/// Replaces matching on a synthetic `name` string, which could not distinguish
/// the cases without agreeing on a magic constant at a distance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LambdaKind {
    /// Written in source as `(x) -> { … }`, or synthesized to behave exactly
    /// like one — the wrappers `lower_cst` builds around `test` / `testset`
    /// bodies so they can be passed to a registration call.
    Anonymous,
    /// The body wrapper `lower_spawn_expr` synthesizes for `spawn { … }`.
    /// Its throws surface is left open rather than defaulting to `never`,
    /// because the spawned body's errors surface through the `Future`.
    Spawn,
}

/// An anonymous function *value*, written inside an expression body.
///
/// Distinct from [`FunctionDef`], which describes a declared item. A lambda has
/// no name, no generic parameters (the parser rejects them), no attributes, no
/// docstring and no declarative metadata, and its body is always an expression
/// body — never a `$rust_function` builtin. Carrying only those fields keeps
/// those states unrepresentable rather than filling them with synthetic values
/// that every reader then has to know to ignore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LambdaDef {
    pub kind: LambdaKind,
    pub params: Vec<Param>,
    /// The lambda's parameter-default expressions, in their own arena.
    pub defaults: FunctionDefaults,
    pub return_type: Option<TypeExpr>,
    pub throws: Option<TypeExpr>,
    /// The lambda's body, as an expression in the *enclosing* body's arena.
    ///
    /// A lambda does not own an arena: its body is lowered into the body that
    /// contains it, exactly as rust-analyzer's `Expr::Closure { body: ExprId }`
    /// does. `None` when the lambda has no `BLOCK_EXPR` child (a parse failure).
    pub body: Option<ExprId>,
    /// The lambda's *declaration* span, which is not always the span its
    /// enclosing body records for the `Expr::Lambda` node: the synthetic
    /// lambdas that `lower_cst` builds for top-level `test` / `testset`
    /// registration carry an empty range here while their expression node
    /// carries the test block's real range.
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDefaults {
    pub exprs: ExprBody,
    pub source_map: AstSourceMap,
}

impl FunctionDefaults {
    pub fn empty() -> Self {
        Self {
            exprs: ExprBody::default(),
            source_map: AstSourceMap::new(),
        }
    }

    pub fn expr(&self, id: DefaultExprId) -> &Expr {
        &self.exprs.exprs[id.expr()]
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionBodyDef {
    Expr(ExprBody, AstSourceMap),
    /// Body is `$rust_function` or `$rust_io_function` — Rust-bound implementation.
    Builtin(BuiltinKind),
}

/// What kind of builtin a function is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum BuiltinKind {
    /// VM instruction — fast, synchronous, no I/O.
    Vm,
    /// I/O operation — may be async, may fail with I/O errors.
    Io,
    /// Compiler intrinsic — lowered to `StatementKind::Intrinsic` in MIR,
    /// not compiled as a callable function.
    Intrinsic,
    /// BEP-034 `baml.future.__await_any` — lowered to a `Terminator::AwaitAny`
    /// suspend point (like `await`), not a normal call. The single argument is
    /// the array of futures; the result is the `int` index of the first to
    /// settle.
    AwaitAny,
}

/// Source geometry of an LLM function's prompt literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmPromptSpans {
    /// The whole literal (backtick or quoted), delimiters included.
    pub literal: TextRange,
    /// Every `${…}` construct inside it — interpolations and
    /// `${for}`/`${if}`/`${end…}` block tags. Offsets outside these (and
    /// inside `literal`) are prompt prose.
    pub code: Vec<TextRange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmBodyDef {
    pub client: Option<Name>,
    /// Pre-lowered companion bodies keyed by target name. The single-path
    /// world stashes exactly one: `"spec"` — the `<Fn>$spec` body, built in
    /// `lower_cst` while the CST backtick is still in hand (the AST must stay
    /// CST-free for Salsa: a rowan node is `!Send`), and read back by
    /// `companions::llm_spec`. Absent when the prompt or client is unusable
    /// (a migration diagnostic was emitted instead).
    pub companion_bodies: Vec<(std::string::String, (ExprBody, AstSourceMap))>,
    /// The prompt literal's source geometry, recorded while the CST is in
    /// hand: hover/navigation classify prompt PROSE (addressed to the
    /// `ai.prompt` driver) versus `${…}` code without re-deriving the
    /// desugared spec body's aliased spans. `None` when the prompt was
    /// unusable.
    pub prompt_spans: Option<LlmPromptSpans>,
    /// True when the function's `tools` field can hold tools at runtime:
    /// any value other than an absent field or a literal empty list (`tools
    /// []`). A non-literal expression (`tools: shared()`) counts as `true`
    /// even if it evaluates empty — the compile-time signal is conservative.
    /// PPIR skips `$stream` synthesis when set (streaming does not run the
    /// tool loop); `ai.stream.from_spec`'s runtime empty-toolbox check covers the
    /// dynamic cases.
    pub has_tools: bool,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: Name,
    pub type_expr: Option<TypeExpr>,
    pub default: Option<DefaultExprId>,
    pub span: TextRange,
    pub name_span: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefaultExprId(ExprId);

impl DefaultExprId {
    pub fn new(expr: ExprId) -> Self {
        Self(expr)
    }

    pub fn expr(self) -> ExprId {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDef {
    pub name: Name,
    /// Generic type parameters (e.g., `["T"]` for `Array<T>`). Empty for non-generic classes.
    pub generic_params: Vec<GenericParam>,
    pub fields: Vec<FieldDef>,
    pub methods: Vec<FunctionDef>,
    /// `implements I { ... }` blocks declared inside the class body (BEP-044).
    pub implements: Vec<ImplementsBlockDef>,
    pub attributes: Vec<RawAttribute>,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<std::string::String>,
    pub span: TextRange,
    pub name_span: TextRange,
}

/// Definition of an `interface` declaration (BEP-044).
///
/// Interfaces declare a contract over fields and methods. Classes opt in to
/// the contract via [`ImplementsBlockDef`] inside the class body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceDef {
    pub name: Name,
    /// Generic type parameters (e.g., `["T"]` for `Container<T>`). Empty for non-generic interfaces.
    pub generic_params: Vec<GenericParam>,
    /// Required interfaces from `requires I1, I2, ...`. Each is parsed as a
    /// `TypeExpr` so we can accept generic requirements like `Container<int>`.
    pub requires: Vec<TypeExpr>,
    /// Field signatures declared on the interface. Interface fields cannot
    /// have default values — see BEP-044 §"Interface Fields".
    pub fields: Vec<FieldDef>,
    /// Associated type declarations on the interface (BEP-057).
    pub associated_types: Vec<AssociatedTypeDef>,
    /// Required methods (no body). Implementing classes must provide a body.
    pub required_methods: Vec<MethodSigDef>,
    /// Default methods (with body). Implementing classes inherit unless they override.
    pub default_methods: Vec<FunctionDef>,
    pub attributes: Vec<RawAttribute>,
    pub docstring: Option<std::string::String>,
    pub span: TextRange,
    pub name_span: TextRange,
}

/// Method signature declared in an interface body without a body — i.e., a
/// required method. Mirrors [`FunctionDef`] minus the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodSigDef {
    pub name: Name,
    pub generic_params: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub defaults: FunctionDefaults,
    pub return_type: Option<TypeExpr>,
    pub throws: Option<TypeExpr>,
    pub attributes: Vec<RawAttribute>,
    pub docstring: Option<std::string::String>,
    pub span: TextRange,
    pub name_span: TextRange,
}

/// One `implements I { ... }` block inside a class body (BEP-044).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplementsBlockDef {
    /// The target interface, captured as a `TypeExpr` so we can accept generic
    /// parameterization like `implements Container<int>`. The path's first
    /// segment is the interface name.
    pub target: TypeExpr,
    /// Explicit mappings from interface fields to class fields:
    /// `interface_field as class_field`.
    pub field_links: Vec<InterfaceFieldLinkDef>,
    /// Associated type bindings, e.g. `type Item = int`.
    pub associated_type_bindings: Vec<AssociatedTypeBindingDef>,
    /// Method overrides / definitions inside this `implements` block.
    pub methods: Vec<FunctionDef>,
    /// True when this block came from top-level `implements I for T`.
    pub is_out_of_body: bool,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceFieldLinkDef {
    pub interface_field: Name,
    pub class_field: Name,
    pub span: TextRange,
    pub interface_field_span: TextRange,
    pub class_field_span: TextRange,
}

/// Top-level `implements I for T { ... }` block (BEP-044).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplementsForDef {
    /// Generic type parameters on the implements block, each with its set of
    /// `&`-separated interface bounds (`<T>` → `(T, [])`; `<T extends A & B>` →
    /// `(T, [A, B])`). Empty bound list = unbounded.
    pub generic_params: Vec<GenericParam>,
    /// The interface being implemented.
    pub interface_target: TypeExpr,
    /// The type the interface is being implemented for.
    pub for_target: TypeExpr,
    /// Explicit mappings from interface fields to class fields.
    pub field_links: Vec<InterfaceFieldLinkDef>,
    /// Associated type bindings, e.g. `type Item = int`.
    pub associated_type_bindings: Vec<AssociatedTypeBindingDef>,
    /// Method definitions inside the block.
    pub methods: Vec<FunctionDef>,
    pub span: TextRange,
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociatedTypeDef {
    pub name: Name,
    pub bound: Option<TypeExpr>,
    pub default: Option<TypeExpr>,
    pub span: TextRange,
    pub name_span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociatedTypeBindingDef {
    pub name: Name,
    pub type_expr: Option<TypeExpr>,
    pub span: TextRange,
    pub name_span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDef {
    pub name: Name,
    /// Always present. A field written without a type is reported by the parser and
    /// recovers as [`TypeExprKind::Error`] — "no type" is not a kind of type, so it is
    /// not representable here.
    pub type_expr: TypeExpr,
    pub attributes: Vec<RawAttribute>,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<std::string::String>,
    pub span: TextRange,
    pub name_span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDef {
    pub name: Name,
    pub variants: Vec<VariantDef>,
    pub attributes: Vec<RawAttribute>,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<std::string::String>,
    pub span: TextRange,
    pub name_span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantDef {
    pub name: Name,
    pub attributes: Vec<RawAttribute>,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<std::string::String>,
    pub span: TextRange,
    pub name_span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAliasDef {
    pub name: Name,
    pub type_expr: Option<TypeExpr>,
    pub span: TextRange,
    pub name_span: TextRange,
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientDef {
    pub name: Name,
    pub config_items: Vec<ConfigItemDef>,
    pub span: TextRange,
    pub name_span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigItemDef {
    pub key: Name,
    pub value: std::string::String,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateStringDef {
    pub name: Name,
    pub params: Vec<Param>,
    pub span: TextRange,
    pub name_span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicyDef {
    pub name: Name,
    pub config_items: Vec<ConfigItemDef>,
    pub span: TextRange,
    pub name_span: TextRange,
}

/// A top-level let binding. Source `let` declarations and compiler-generated
/// client/retry-policy bindings share the same `$init` pipeline.
/// Carries an optional `ExprBody` initializer that flows through TIR type-checking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LetDef {
    pub name: Name,
    pub initializer: Option<(ExprBody, AstSourceMap)>,
    pub origin: LetOrigin,
    pub span: TextRange,
    pub name_span: TextRange,
}
