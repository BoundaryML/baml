//! Concrete AST structs for BAML — full structural data in memory.
//!
//! Every node carries all its content as owned Rust data (names, type trees,
//! expression trees) with `TextRange` alongside for source mapping. A single
//! `lower_file` function converts the CST to `Vec<Item>`. This isolates all
//! CST `Option` handling in one layer so everything downstream gets clean
//! typed data and can be constructed directly in tests without parsing.

use std::collections::HashMap;

use baml_base::Name;
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
pub enum TypeExpr {
    /// Named type path: `User`, `baml.http.Request`, `Stream<T>`
    Path {
        segments: Vec<Name>,
        /// Generic type arguments (e.g., `<T>` in `Stream<T>`). Empty for non-generic paths.
        generic_args: Vec<TypeExpr>,
        attrs: Vec<RawAttribute>,
    },
    /// Primitive types
    Int {
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
    /// Function type: (params) -> return
    Function {
        params: Vec<FunctionTypeParam>,
        ret: Box<TypeExpr>,
        attrs: Vec<RawAttribute>,
    },
    /// The `unknown` keyword type
    BuiltinUnknown {
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
    /// Unknown/missing type
    Unknown {
        attrs: Vec<RawAttribute>,
    },
}

impl TypeExpr {
    /// Access the type-level attributes on this type expression.
    pub fn attrs(&self) -> &[RawAttribute] {
        match self {
            Self::Path { attrs, .. }
            | Self::Int { attrs }
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
            | Self::BuiltinUnknown { attrs }
            | Self::Type { attrs }
            | Self::Rust { attrs }
            | Self::Error { attrs }
            | Self::Unknown { attrs } => attrs,
        }
    }

    /// Mutable access to the type-level attributes on this type expression.
    pub fn attrs_mut(&mut self) -> &mut Vec<RawAttribute> {
        match self {
            Self::Path { attrs, .. }
            | Self::Int { attrs }
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
            | Self::BuiltinUnknown { attrs }
            | Self::Type { attrs }
            | Self::Rust { attrs }
            | Self::Error { attrs }
            | Self::Unknown { attrs } => attrs,
        }
    }
}

/// A parameter in a function type expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionTypeParam {
    pub name: Option<Name>,
    pub ty: TypeExpr,
}

/// A type expression with its source span — used in item definitions
/// where we need both the type data and the source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpannedTypeExpr {
    pub expr: TypeExpr,
    pub span: TextRange,
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
            Expr::MemberAccess { base, member } => {
                format!("{}.{member}", self.display_expr_inner(*base, depth + 1))
            }
            Expr::OptionalMemberAccess { base, member } => {
                format!("{}?.{member}", self.display_expr_inner(*base, depth + 1))
            }
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
            Expr::Call { callee, args } => {
                let args_str: Vec<_> = args
                    .iter()
                    .map(|a| self.display_expr_inner(*a, depth + 1))
                    .collect();
                format!(
                    "{}({})",
                    self.display_expr_inner(*callee, depth + 1),
                    args_str.join(", ")
                )
            }
            Expr::OptionalCall { callee, args } => {
                let args_str: Vec<_> = args
                    .iter()
                    .map(|a| self.display_expr_inner(*a, depth + 1))
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

    /// Look up the source span of a pattern by its `PatId`.
    pub fn pattern_span(&self, id: PatId) -> TextRange {
        let raw: u32 = id.into_raw().into_u32();
        self.pattern_spans
            .iter()
            .nth(raw as usize)
            .map(|(_, &span)| span)
            .unwrap_or_default()
    }

    /// Look up the source span of a match arm by its `MatchArmId`.
    pub fn match_arm_span(&self, id: MatchArmId) -> TextRange {
        let raw: u32 = id.into_raw().into_u32();
        self.match_arm_spans
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
    /// Byte string literal: `b"hello"`, `b"\x00\xFF"`.
    /// Stores the fully-resolved bytes (escape sequences already processed).
    ByteStringLiteral(Vec<u8>),
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
    // These nodes are constructed purely in the HIR layer AFTER
    // name resolution as we can't know if it's a member access
    // until we know how to resolve the path
    MemberAccess {
        base: ExprId,
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
    Lambda(Box<FunctionDef>),
    /// Optional index: `obj?.[expr]` — short-circuits to null if base is null.
    OptionalIndex {
        base: ExprId,
        index: ExprId,
    },
    /// Optional call: `func?.(args)` — short-circuits to null if callee is null.
    OptionalCall {
        callee: ExprId,
        args: Vec<ExprId>,
    },
    /// Wraps an expression chain containing `?.` operators.
    /// Delimits the scope of null short-circuiting.
    /// If any `?.` inside encounters null, the entire `OptionalChain` evaluates to null.
    OptionalChain {
        expr: ExprId,
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

/// Patterns — modeled after `Pattern` in `body.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    Binding(Name),
    TypedBinding { name: Name, ty: TypeExpr },
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
    Compiler,
    Client,
    RetryPolicy,
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
    Let(LetDef),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclarativeMeta {
    /// LLM function metadata (client name, prompt template).
    /// Present only for functions declared with `{ client ...; prompt ... }` syntax.
    /// The body is desugared to a synthetic `Expr` calling `baml.llm.call_llm_function`,
    /// while this field preserves the original metadata for Jinja type-checking.
    Llm(LlmBodyDef),
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
    pub declarative_meta: Option<DeclarativeMeta>,
    pub attributes: Vec<RawAttribute>,
    pub span: TextRange,
    pub name_span: TextRange,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionBodyDef {
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

/// A top-level let binding — compiler-generated, not user syntax.
/// Carries an optional `ExprBody` initializer that flows through TIR type-checking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LetDef {
    pub name: Name,
    pub initializer: Option<(ExprBody, AstSourceMap)>,
    pub origin: LetOrigin,
    pub span: TextRange,
    pub name_span: TextRange,
}
