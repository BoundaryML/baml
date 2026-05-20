//! Concrete AST structs for BAML — full structural data in memory.
//!
//! Every node carries all its content as owned Rust data (names, type trees,
//! expression trees) with `TextRange` alongside for source mapping. A single
//! `lower_file` function converts the CST to `Vec<Item>`. This isolates all
//! CST `Option` handling in one layer so everything downstream gets clean
//! typed data and can be constructed directly in tests without parsing.

use std::collections::HashMap;

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
        throws: Option<Box<TypeExpr>>,
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

impl std::fmt::Display for TypeExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn needs_parens(ty: &TypeExpr) -> bool {
            matches!(ty, TypeExpr::Union { .. } | TypeExpr::Function { .. })
        }

        fn write_postfix_base(f: &mut std::fmt::Formatter<'_>, ty: &TypeExpr) -> std::fmt::Result {
            if needs_parens(ty) {
                write!(f, "({ty})")
            } else {
                write!(f, "{ty}")
            }
        }

        match self {
            TypeExpr::Path {
                segments,
                generic_args,
                ..
            } => {
                let path = segments
                    .iter()
                    .map(smol_str::SmolStr::as_str)
                    .collect::<Vec<_>>()
                    .join(".");
                write!(f, "{path}")?;
                if !generic_args.is_empty() {
                    write!(f, "<")?;
                    for (i, arg) in generic_args.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{arg}")?;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
            TypeExpr::Int { .. } => write!(f, "int"),
            TypeExpr::Float { .. } => write!(f, "float"),
            TypeExpr::String { .. } => write!(f, "string"),
            TypeExpr::Bool { .. } => write!(f, "bool"),
            TypeExpr::Null { .. } => write!(f, "null"),
            TypeExpr::Never { .. } => write!(f, "never"),
            TypeExpr::Void { .. } => write!(f, "void"),
            TypeExpr::Uint8Array { .. } => write!(f, "uint8array"),
            TypeExpr::Media { kind, .. } => write!(f, "{}", format!("{kind:?}").to_lowercase()),
            TypeExpr::Optional { inner, .. } => {
                write_postfix_base(f, inner)?;
                write!(f, "?")
            }
            TypeExpr::List { inner, .. } => {
                write_postfix_base(f, inner)?;
                write!(f, "[]")
            }
            TypeExpr::Map { key, value, .. } => write!(f, "map<{key}, {value}>"),
            TypeExpr::Union { variants, .. } => {
                for (i, v) in variants.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    if matches!(v, TypeExpr::Function { .. }) {
                        write!(f, "({v})")?;
                    } else {
                        write!(f, "{v}")?;
                    }
                }
                Ok(())
            }
            TypeExpr::Literal { value, .. } => write!(f, "{value}"),
            TypeExpr::Function {
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
                if matches!(**ret, TypeExpr::Function { .. }) {
                    write!(f, "({ret})")?;
                } else {
                    write!(f, "{ret}")?;
                }
                if let Some(throws) = throws {
                    write!(f, " throws {throws}")?;
                }
                Ok(())
            }
            TypeExpr::BuiltinUnknown { .. } => write!(f, "unknown"),
            TypeExpr::Type { .. } => write!(f, "type"),
            TypeExpr::Rust { .. } => write!(f, "$rust_type"),
            TypeExpr::Error { .. } => write!(f, "error"),
            TypeExpr::Unknown { .. } => write!(f, "?"),
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

    /// Look up a labeled call argument's label span.
    pub fn call_arg_label_span(&self, call: ExprId, arg_expr: ExprId) -> TextRange {
        self.call_arg_label_spans
            .get(&(call, arg_expr))
            .copied()
            .unwrap_or_else(|| self.expr_span(call))
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
    /// BEP-034 `spawn name_expr? { body }`. The body is always a block
    /// expression that runs on a freshly-spawned green thread; the
    /// optional `name` is any expression that evaluates to a string and
    /// surfaces in debug / stack traces.
    Spawn {
        /// Optional human-readable label for the spawn.
        name: Option<ExprId>,
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
        type_name: Option<TypePath>,
        /// Explicit generic type args from syntax like `Foo<int> { ... }`.
        /// Empty when no `<...>` was written (e.g. bare `Foo { ... }`).
        type_args: Vec<TypeExpr>,
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
        args: Vec<CallArg>,
    },
    /// Wraps an expression chain containing `?.` operators.
    /// Delimits the scope of null short-circuiting.
    /// If any `?.` inside encounters null, the entire `OptionalChain` evaluates to null.
    OptionalChain {
        expr: ExprId,
    },
    Missing,
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
    Let {
        /// The binding pattern. A `: T` annotation lives inside the pattern
        /// as the bind's sub-pattern slot, not as a separate field on
        /// `Stmt::Let` — see [`Pattern::Bind`].
        pattern: PatId,
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
            Pattern::Wildcard | Pattern::Type(_) => {}
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
    Compiler,
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
    pub defaults: FunctionDefaults,
    pub return_type: Option<SpannedTypeExpr>,
    pub throws: Option<SpannedTypeExpr>,
    pub body: Option<FunctionBodyDef>,
    pub declarative_meta: Option<DeclarativeMeta>,
    pub origin: FunctionOrigin,
    pub attributes: Vec<RawAttribute>,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<std::string::String>,
    pub span: TextRange,
    pub name_span: TextRange,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinKind {
    /// VM instruction — fast, synchronous, no I/O.
    Vm,
    /// I/O operation — may be async, may fail with I/O errors.
    Io,
    /// Compiler intrinsic — lowered to `StatementKind::Intrinsic` in MIR,
    /// not compiled as a callable function.
    Intrinsic,
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
    /// Span of the full interpolation, including delimiters.
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: Name,
    pub type_expr: Option<SpannedTypeExpr>,
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
    pub generic_params: Vec<Name>,
    pub fields: Vec<FieldDef>,
    pub methods: Vec<FunctionDef>,
    pub attributes: Vec<RawAttribute>,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<std::string::String>,
    pub span: TextRange,
    pub name_span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDef {
    pub name: Name,
    pub type_expr: Option<SpannedTypeExpr>,
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
