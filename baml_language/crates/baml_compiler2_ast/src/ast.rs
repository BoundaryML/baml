//! Concrete AST structs for BAML — full structural data in memory.
//!
//! Every node carries all its content as owned Rust data (names, type trees,
//! expression trees) with `TextRange` alongside for source mapping. A single
//! `lower_file` function converts the CST to `Vec<Item>`. This isolates all
//! CST `Option` handling in one layer so everything downstream gets clean
//! typed data and can be constructed directly in tests without parsing.

use baml_base::Name;
use la_arena::{Arena, Idx};
use text_size::TextRange;

// ── Attributes ──────────────────────────────────────────────────

/// Raw attribute from CST — not yet validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawAttribute {
    pub name: Name,
    pub args: Vec<RawAttributeArg>,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
pub enum TypeExpr {
    /// Named type path: `User`, `baml.http.Request`
    Path(Vec<Name>),
    /// Primitive types
    Int,
    Float,
    String,
    Bool,
    Null,
    Never,
    /// Media types
    Media(baml_base::MediaKind),
    /// T?
    Optional(Box<TypeExpr>),
    /// T[]
    List(Box<TypeExpr>),
    /// map<K, V>
    Map {
        key: Box<TypeExpr>,
        value: Box<TypeExpr>,
    },
    /// A | B | C
    Union(Vec<TypeExpr>),
    /// Literal types in unions: `"user"`, `200`, `3.14`, `true`.
    Literal(baml_base::Literal),
    /// Function type: (params) -> return
    Function {
        params: Vec<FunctionTypeParam>,
        ret: Box<TypeExpr>,
    },
    /// The `unknown` keyword type
    BuiltinUnknown,
    /// The `type` meta-type keyword
    Type,
    /// `$rust_type` — opaque Rust-managed state field type.
    Rust,
    /// Error recovery sentinel
    Error,
    /// Unknown/missing type
    Unknown,
}

/// A parameter in a function type expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionTypeParam {
    pub name: Option<Name>,
    pub ty: TypeExpr,
}

/// Recursive spanned type expression kind — mirrors `TypeExpr` but children
/// are `SpannedTypeExpr` so every sub-expression carries its own source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpannedTypeExprKind {
    Path(Vec<Name>),
    Int,
    Float,
    String,
    Bool,
    Null,
    Never,
    Media(baml_base::MediaKind),
    Optional(Box<SpannedTypeExpr>),
    List(Box<SpannedTypeExpr>),
    Map {
        key: Box<SpannedTypeExpr>,
        value: Box<SpannedTypeExpr>,
    },
    Union(Vec<SpannedTypeExpr>),
    Literal(baml_base::Literal),
    Function {
        params: Vec<SpannedFunctionTypeParam>,
        ret: Box<SpannedTypeExpr>,
    },
    BuiltinUnknown,
    Type,
    Rust,
    Error,
    Unknown,
}

/// A parameter in a spanned function type expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpannedFunctionTypeParam {
    pub name: Option<Name>,
    pub ty: SpannedTypeExpr,
}

/// A type expression with its source span — used in item definitions
/// where we need both the type data and the source location.
///
/// Recursive: children are also `SpannedTypeExpr`, so every sub-expression
/// (e.g. each union member) carries its own span for precise diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpannedTypeExpr {
    pub kind: SpannedTypeExprKind,
    pub span: TextRange,
}

impl SpannedTypeExpr {
    /// Recursively strip spans to produce a span-free `TypeExpr` for Salsa queries.
    pub fn to_type_expr(&self) -> TypeExpr {
        match &self.kind {
            SpannedTypeExprKind::Path(segments) => TypeExpr::Path(segments.clone()),
            SpannedTypeExprKind::Int => TypeExpr::Int,
            SpannedTypeExprKind::Float => TypeExpr::Float,
            SpannedTypeExprKind::String => TypeExpr::String,
            SpannedTypeExprKind::Bool => TypeExpr::Bool,
            SpannedTypeExprKind::Null => TypeExpr::Null,
            SpannedTypeExprKind::Never => TypeExpr::Never,
            SpannedTypeExprKind::Media(kind) => TypeExpr::Media(*kind),
            SpannedTypeExprKind::Optional(inner) => {
                TypeExpr::Optional(Box::new(inner.to_type_expr()))
            }
            SpannedTypeExprKind::List(inner) => TypeExpr::List(Box::new(inner.to_type_expr())),
            SpannedTypeExprKind::Map { key, value } => TypeExpr::Map {
                key: Box::new(key.to_type_expr()),
                value: Box::new(value.to_type_expr()),
            },
            SpannedTypeExprKind::Union(members) => {
                TypeExpr::Union(members.iter().map(SpannedTypeExpr::to_type_expr).collect())
            }
            SpannedTypeExprKind::Literal(lit) => TypeExpr::Literal(lit.clone()),
            SpannedTypeExprKind::Function { params, ret } => TypeExpr::Function {
                params: params
                    .iter()
                    .map(|p| FunctionTypeParam {
                        name: p.name.clone(),
                        ty: p.ty.to_type_expr(),
                    })
                    .collect(),
                ret: Box::new(ret.to_type_expr()),
            },
            SpannedTypeExprKind::BuiltinUnknown => TypeExpr::BuiltinUnknown,
            SpannedTypeExprKind::Type => TypeExpr::Type,
            SpannedTypeExprKind::Rust => TypeExpr::Rust,
            SpannedTypeExprKind::Error => TypeExpr::Error,
            SpannedTypeExprKind::Unknown => TypeExpr::Unknown,
        }
    }
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
    /// Parallel to `type_annotation_spans` — full `SpannedTypeExpr` tree for
    /// per-node diagnostics (e.g. underline only the bad union member).
    pub type_annotation_spanned_exprs: Vec<SpannedTypeExpr>,
    pub catch_arm_spans: Arena<TextRange>,
}

impl AstSourceMap {
    pub fn new() -> Self {
        Self {
            expr_spans: Arena::new(),
            stmt_spans: Arena::new(),
            pattern_spans: Arena::new(),
            match_arm_spans: Arena::new(),
            type_annotation_spans: Arena::new(),
            type_annotation_spanned_exprs: Vec::new(),
            catch_arm_spans: Arena::new(),
        }
    }

    /// Look up the source span of a statement by its `StmtId`.
    ///
    /// The `stmt_spans` arena is parallel to `ExprBody::stmts` — same indices,
    /// different element type. We convert via raw index.
    pub fn stmt_span(&self, id: StmtId) -> TextRange {
        let raw: u32 = id.into_raw().into_u32();
        self.stmt_spans
            .iter()
            .nth(raw as usize)
            .map(|(_, &span)| span)
            .unwrap_or_default()
    }

    /// Look up the source span of an expression by its `ExprId`.
    pub fn expr_span(&self, id: ExprId) -> TextRange {
        let raw: u32 = id.into_raw().into_u32();
        self.expr_spans
            .iter()
            .nth(raw as usize)
            .map(|(_, &span)| span)
            .unwrap_or_default()
    }

    /// Look up the source span of a pattern by its `PatId`.
    pub fn pattern_span(&self, id: PatId) -> TextRange {
        let raw: u32 = id.into_raw().into_u32();
        self.pattern_spans
            .iter()
            .nth(raw as usize)
            .map(|(_, &span)| span)
            .unwrap_or_default()
    }

    /// Look up the source span of a type annotation by its `TypeAnnotId`.
    pub fn type_annotation_span(&self, id: TypeAnnotId) -> TextRange {
        let raw: u32 = id.into_raw().into_u32();
        self.type_annotation_spans
            .iter()
            .nth(raw as usize)
            .map(|(_, &span)| span)
            .unwrap_or_default()
    }

    /// Look up the full `SpannedTypeExpr` for a type annotation (for per-node diagnostics).
    pub fn type_annotation_spanned(&self, id: TypeAnnotId) -> Option<&SpannedTypeExpr> {
        let raw: u32 = id.into_raw().into_u32();
        self.type_annotation_spanned_exprs.get(raw as usize)
    }

    /// Look up the source span of a catch arm by its `CatchArmId`.
    pub fn catch_arm_span(&self, id: CatchArmId) -> TextRange {
        let raw: u32 = id.into_raw().into_u32();
        self.catch_arm_spans
            .iter()
            .nth(raw as usize)
            .map(|(_, &span)| span)
            .unwrap_or_default()
    }
}

impl Default for AstSourceMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Expressions — modeled after `Expr` in `body.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Literal(Literal),
    Null,
    /// Path expression: `x`, `user.name`, `Status.Active`
    Path(Vec<Name>),
    If {
        condition: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
    },
    Match {
        scrutinee: ExprId,
        scrutinee_type: Option<TypeAnnotId>,
        arms: Vec<MatchArmId>,
    },
    Catch {
        base: ExprId,
        clauses: Vec<CatchClause>,
    },
    Throw {
        value: ExprId,
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
        args: Vec<ExprId>,
    },
    Object {
        type_name: Option<Name>,
        fields: Vec<(Name, ExprId)>,
        spreads: Vec<SpreadField>,
    },
    Array {
        elements: Vec<ExprId>,
    },
    Map {
        entries: Vec<(ExprId, ExprId)>,
    },
    Block {
        stmts: Vec<StmtId>,
        tail_expr: Option<ExprId>,
    },
    FieldAccess {
        base: ExprId,
        field: Name,
    },
    Index {
        base: ExprId,
        index: ExprId,
    },
    Missing,
}

/// Statements — modeled after `Stmt` in `body.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Expr(ExprId),
    Let {
        pattern: PatId,
        type_annotation: Option<TypeAnnotId>,
        initializer: Option<ExprId>,
        is_watched: bool,
        origin: LetOrigin,
    },
    While {
        condition: ExprId,
        body: ExprId,
        after: Option<StmtId>,
        origin: LoopOrigin,
    },
    Return(Option<ExprId>),
    Throw {
        value: ExprId,
    },
    Break,
    Continue,
    Assign {
        target: ExprId,
        value: ExprId,
    },
    AssignOp {
        target: ExprId,
        op: AssignOp,
        value: ExprId,
    },
    Assert {
        condition: ExprId,
    },
    Missing,
    HeaderComment {
        name: Name,
        level: usize,
    },
}

/// Patterns — modeled after `Pattern` in `body.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    Binding(Name),
    TypedBinding { name: Name, ty: SpannedTypeExpr },
    Literal(Literal),
    Null,
    EnumVariant { enum_name: Name, variant: Name },
    Union(Vec<PatId>),
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
    Compiler,
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
    Instanceof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
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
    TypeAlias(TypeAliasDef),
    Client(ClientDef),
    Test(TestDef),
    Generator(GeneratorDef),
    TemplateString(TemplateStringDef),
    RetryPolicy(RetryPolicyDef),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDef {
    pub name: Name,
    /// Generic type parameters (e.g., `["T", "U"]`). Empty for non-generic functions.
    pub generic_params: Vec<Name>,
    pub params: Vec<Param>,
    pub return_type: Option<SpannedTypeExpr>,
    pub throws: Option<SpannedTypeExpr>,
    pub body: Option<FunctionBodyDef>,
    pub attributes: Vec<RawAttribute>,
    pub span: TextRange,
    pub name_span: TextRange,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionBodyDef {
    Llm(LlmBodyDef),
    Expr(ExprBody, AstSourceMap),
    /// Body is `$rust_function` or `$rust_io_function` — Rust-bound implementation.
    Builtin(BuiltinKind),
}

/// What kind of builtin a function is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinKind {
    /// VM instruction — fast, synchronous, no I/O.
    Vm,
    /// I/O operation — may be async, may fail with I/O errors.
    Io,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmBodyDef {
    pub client: Option<Name>,
    pub prompt: Option<RawPrompt>,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPrompt {
    pub text: std::string::String,
    /// Interpolation locations within the template.
    pub interpolations: Vec<Interpolation>,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interpolation {
    pub content: std::string::String,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: Name,
    pub type_expr: Option<SpannedTypeExpr>,
    pub span: TextRange,
    pub name_span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDef {
    pub name: Name,
    /// Generic type parameters (e.g., `["T"]` for `Array<T>`). Empty for non-generic classes.
    pub generic_params: Vec<Name>,
    pub fields: Vec<FieldDef>,
    pub methods: Vec<FunctionDef>,
    pub attributes: Vec<RawAttribute>,
    pub span: TextRange,
    pub name_span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDef {
    pub name: Name,
    pub type_expr: Option<SpannedTypeExpr>,
    pub attributes: Vec<RawAttribute>,
    pub span: TextRange,
    pub name_span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDef {
    pub name: Name,
    pub variants: Vec<VariantDef>,
    pub attributes: Vec<RawAttribute>,
    pub span: TextRange,
    pub name_span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantDef {
    pub name: Name,
    pub attributes: Vec<RawAttribute>,
    pub span: TextRange,
    pub name_span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAliasDef {
    pub name: Name,
    pub type_expr: Option<SpannedTypeExpr>,
    pub span: TextRange,
    pub name_span: TextRange,
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
pub struct TestDef {
    pub name: Name,
    pub config_items: Vec<ConfigItemDef>,
    pub span: TextRange,
    pub name_span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratorDef {
    pub name: Name,
    pub config_items: Vec<ConfigItemDef>,
    pub span: TextRange,
    pub name_span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateStringDef {
    pub name: Name,
    pub params: Vec<Param>,
    pub body: Option<RawPrompt>,
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
