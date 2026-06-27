//! Semantic tokens for BAML files (compiler2 / `lsp2_actions` version).
//!
//! `semantic_tokens(db, file) -> Vec<SemanticToken>` is a single document-order
//! walk of the CST. Classification follows rust-analyzer's model:
//!
//! - **Structural tokens** (keywords, punctuation, strings, comments, numbers)
//!   are classified syntactically by token kind — the syntax tree only supplies
//!   positions and these non-name tokens.
//!
//! - **Identifiers inside expression bodies** are classified by what they
//!   *resolve to*, via a pre-built resolution index ([`index`]) keyed by exact
//!   name spans. The type system is never used to pick a tag; only resolution
//!   facts (`MemberResolution`, `ResolvedName`, `DefinitionKind`) are. There is
//!   no substring scanning.
//!
//! - **Declaration names** are classified by their declaring node and carry the
//!   `declaration` modifier; a reference is classified the same way as its
//!   definition.

use baml_base::{Name, SourceFile};
use baml_compiler_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};
use baml_compiler2_tir::resolve::{resolve_name_at, resolve_path_at};
use rowan::NodeOrToken;
use text_size::{TextRange, TextSize};

use crate::Db;

mod classify;
mod index;

// ── SemanticTokenType ─────────────────────────────────────────────────────────

/// The semantic token type for a BAML file.
///
/// Copied from `baml_lsp_actions::semantic_tokens::SemanticTokenType` — the
/// same enum values, same `TOKEN_TYPES` legend ordering. The v2 crate owns
/// this type so there is no dependency on the v1 compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticTokenType {
    Namespace,
    Type,
    Class,
    Enum,
    Interface,
    Struct,
    TypeParameter,
    Parameter,
    Variable,
    Property,
    EnumMember,
    Event,
    Function,
    Method,
    Macro,
    Keyword,
    Modifier,
    Comment,
    String,
    Number,
    Regexp,
    Operator,
    Decorator,
    EscapeSequence,
    /// Boolean literal (`true` / `false`) — a custom type beyond the standard
    /// LSP legend, mirroring rust-analyzer's `boolean`.
    Boolean,
}

/// Token type legend — order determines the LSP legend index.
///
/// The order MUST match what is advertised in `server_capabilities()`.
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::Namespace,
    SemanticTokenType::Type,
    SemanticTokenType::Class,
    SemanticTokenType::Enum,
    SemanticTokenType::Interface,
    SemanticTokenType::Struct,
    SemanticTokenType::TypeParameter,
    SemanticTokenType::Parameter,
    SemanticTokenType::Variable,
    SemanticTokenType::Property,
    SemanticTokenType::EnumMember,
    SemanticTokenType::Event,
    SemanticTokenType::Function,
    SemanticTokenType::Method,
    SemanticTokenType::Macro,
    SemanticTokenType::Keyword,
    SemanticTokenType::Modifier,
    SemanticTokenType::Comment,
    SemanticTokenType::String,
    SemanticTokenType::Number,
    SemanticTokenType::Regexp,
    SemanticTokenType::Operator,
    SemanticTokenType::Decorator,
    SemanticTokenType::EscapeSequence,
    SemanticTokenType::Boolean,
];

impl SemanticTokenType {
    /// Get the index of this token type in the `TOKEN_TYPES` legend.
    ///
    /// The index is the `token_type` field in the LSP `SemanticToken` struct.
    #[allow(clippy::cast_possible_truncation)]
    pub fn legend_index(self) -> u32 {
        TOKEN_TYPES
            .iter()
            .position(|t| *t == self)
            .expect("SemanticTokenType missing in legend") as u32
    }

    /// String representation matching the LSP semantic token type names.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Namespace => "namespace",
            Self::Type => "type",
            Self::Class => "class",
            Self::Enum => "enum",
            Self::Interface => "interface",
            Self::Struct => "struct",
            Self::TypeParameter => "typeParameter",
            Self::Parameter => "parameter",
            Self::Variable => "variable",
            Self::Property => "property",
            Self::EnumMember => "enumMember",
            Self::Event => "event",
            Self::Function => "function",
            Self::Method => "method",
            Self::Macro => "macro",
            Self::Keyword => "keyword",
            Self::Modifier => "modifier",
            Self::Comment => "comment",
            Self::String => "string",
            Self::Number => "number",
            Self::Regexp => "regexp",
            Self::Operator => "operator",
            Self::Decorator => "decorator",
            Self::EscapeSequence => "escapeSequence",
            Self::Boolean => "boolean",
        }
    }
}

// ── Semantic token modifiers ────────────────────────────────────────────────────

bitflags::bitflags! {
    /// LSP semantic token modifiers as a bitset (the `tokenModifiers` bitset).
    ///
    /// Modifiers decorate a token's base type with facts derived from what the
    /// name resolves to — never from syntax. Each flag's bit is its index in
    /// [`TOKEN_MODIFIERS`], which is what `server_capabilities()` advertises.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct ModifierSet: u32 {
        /// The token is the definition site (`function foo` -> `foo`).
        const DECLARATION = 1 << 0;
        /// The entity comes from the `baml` standard library.
        const DEFAULT_LIBRARY = 1 << 1;
        /// The entity is marked deprecated.
        const DEPRECATED = 1 << 2;
        /// The binding cannot be reassigned (e.g. a `const`).
        const READONLY = 1 << 3;
        /// The entity is asynchronous (an `await`/`spawn` target).
        const ASYNC = 1 << 4;
    }
}

/// Modifier legend — the LSP `tokenModifiers` names in bit order.
///
/// The order MUST match the bit positions in [`ModifierSet`] and what is
/// advertised in `server_capabilities()`.
pub const TOKEN_MODIFIERS: &[&str] = &[
    "declaration",
    "defaultLibrary",
    "deprecated",
    "readonly",
    "async",
];

impl ModifierSet {
    /// The set modifiers' LSP names, in legend order.
    pub fn names(self) -> impl Iterator<Item = &'static str> {
        TOKEN_MODIFIERS
            .iter()
            .enumerate()
            .filter(move |(i, _)| self.bits() & (1 << i) != 0)
            .map(|(_, name)| *name)
    }
}

// ── SemanticToken ─────────────────────────────────────────────────────────────

/// A classified token ready for LSP encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticToken {
    pub range: TextRange,
    pub token_type: SemanticTokenType,
    pub modifiers: ModifierSet,
}

/// A token type paired with its modifiers — the unit a classifier produces.
type Class = (SemanticTokenType, ModifierSet);

/// The class for a declaration name: `ty` + the `declaration` modifier.
fn decl(ty: SemanticTokenType) -> Class {
    (ty, ModifierSet::DECLARATION)
}

/// A plain (modifier-free) class.
fn plain(ty: SemanticTokenType) -> Class {
    (ty, ModifierSet::empty())
}

/// Whether a `FUNCTION_DEF` / `METHOD_SIG` node is declared inside a class,
/// interface, or implements block — i.e. it is a method, not a free function.
fn in_method_context(node: &SyntaxNode) -> bool {
    node.ancestors().skip(1).any(|a| {
        matches!(
            a.kind(),
            SyntaxKind::CLASS_DEF
                | SyntaxKind::INTERFACE_DEF
                | SyntaxKind::IMPLEMENTS_BLOCK
                | SyntaxKind::IMPLEMENTS_FOR
        )
    })
}

/// Push a classified token.
fn emit(range: TextRange, class: Class, out: &mut Vec<SemanticToken>) {
    out.push(SemanticToken {
        range,
        token_type: class.0,
        modifiers: class.1,
    });
}

/// Emit a string/byte-string literal, splitting backslash escape sequences
/// (`\n`, `\t`, `\xNN`, `\u{..}`, `\"`, ...) out as `EscapeSequence` while the
/// surrounding text stays `String`. The lexer leaves escapes undecoded, so we
/// scan the literal text ourselves (rust-analyzer does the same).
fn string_with_escapes(node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
    let base: usize = node.text_range().start().into();
    let text = node.text().to_string();
    let bytes = text.as_bytes();
    let span = |a: usize, b: usize| {
        TextRange::new(
            TextSize::new(u32::try_from(a).unwrap_or(u32::MAX)),
            TextSize::new(u32::try_from(b).unwrap_or(u32::MAX)),
        )
    };

    let mut i = 0;
    let mut text_start = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            if i > text_start {
                emit(span(base + text_start, base + i), plain(SemanticTokenType::String), out);
            }
            let len = escape_len(&text[i..]);
            emit(
                span(base + i, base + i + len),
                plain(SemanticTokenType::EscapeSequence),
                out,
            );
            i += len;
            text_start = i;
        } else {
            i += 1;
        }
    }
    if text_start < bytes.len() {
        emit(
            span(base + text_start, base + bytes.len()),
            plain(SemanticTokenType::String),
            out,
        );
    }
}

/// Byte length of the escape sequence at the start of `s` (which begins `\`).
fn escape_len(s: &str) -> usize {
    let bytes = s.as_bytes();
    match bytes.get(1) {
        // `\xNN` — backslash, x, two hex digits.
        Some(b'x') => 4.min(s.len()),
        // `\u{...}` — through the closing brace.
        Some(b'u') if bytes.get(2) == Some(&b'{') => s.find('}').map_or(2, |i| i + 1),
        // `\n`, `\t`, `\r`, `\0`, `\\`, `\"`, `\'`, `\u` (no brace), ...
        Some(_) => 2,
        // A trailing backslash (shouldn't occur before the closing quote).
        None => 1,
    }
}

/// Emit every non-trivia leaf under `node` with one type (comments/strings).
fn emit_node(node: &SyntaxNode, token_type: SemanticTokenType, out: &mut Vec<SemanticToken>) {
    for child in node.descendants_with_tokens() {
        if let NodeOrToken::Token(t) = child {
            if !t.kind().is_whitespace() {
                emit(t.text_range(), plain(token_type), out);
            }
        }
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Compute semantic tokens for a file.
///
/// Always returns tokens in document order (required by the LSP
/// `textDocument/semanticTokens/full` contract).
///
/// A Salsa query: the result is memoized per file and recomputed only when an
/// input it reads changes (`syntax_tree`, `file_semantic_index`,
/// `infer_scope_types`, `function_body`, source maps). It therefore cannot go
/// stale, and a repeated request for an unchanged file is served from cache
/// without re-walking the CST.
#[salsa::tracked(returns(clone))]
pub fn semantic_tokens(db: &dyn Db, file: SourceFile) -> Vec<SemanticToken> {
    let root = baml_compiler_parser::syntax_tree(db, file);
    // Full document: classify every token, so build the merged whole-file index
    // (itself a merge of per-scope salsa-cached indices) and resolve from it.
    let index = index::build(db, file);
    let walk = Walk {
        db,
        file,
        resolve: Box::new(move |range| index.get(&range).copied()),
        range: None,
    };
    let mut out = Vec::new();
    walk.node(&root, &mut out);
    out
}

/// Semantic tokens for a viewport `range` only — rust-analyzer's
/// `highlight_range`. Names are resolved on demand through
/// [`index::resolve_token_class`], so only the scopes the viewport touches are
/// indexed (the rest of the file is never resolved). Not a Salsa query — keying
/// on the range would blow the cache; the underlying per-scope indices and name
/// resolution it calls *are* memoized.
pub fn semantic_tokens_in_range(
    db: &dyn Db,
    file: SourceFile,
    range: TextRange,
) -> Vec<SemanticToken> {
    let root = baml_compiler_parser::syntax_tree(db, file);
    let walk = Walk {
        db,
        file,
        resolve: Box::new(move |r| index::resolve_token_class(db, file, r)),
        range: Some(range),
    };
    let mut out = Vec::new();
    walk.node(&root, &mut out);
    // The range gate is per-subtree; trim the boundary tokens to exactly `range`.
    out.retain(|t| range.intersect(t.range).is_some());
    out
}

// ── Walker ─────────────────────────────────────────────────────────────────────

/// A document-order CST walk that classifies each token exactly once.
///
/// `resolve` resolves an identifier inside an expression body to its
/// classification; a declaration name is classified by its declaring node;
/// every other token is syntactic. For a full-document walk `resolve` is the
/// merged whole-file index; a range walk resolves on demand per scope
/// (rust-analyzer's `Semantics::resolve` model — only visited scopes pay).
struct Walk<'db> {
    db: &'db dyn Db,
    file: SourceFile,
    resolve: Box<dyn Fn(TextRange) -> Option<Class> + 'db>,
    /// For a viewport request: subtrees that don't intersect this range are
    /// skipped entirely, so their scopes are never resolved.
    range: Option<TextRange>,
}

impl Walk<'_> {
    /// Dispatch a node to its classifier.
    fn node(&self, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
        // Range gate: a subtree disjoint from the viewport is skipped wholesale,
        // so its tokens are never classified and its scope never resolved.
        if let Some(r) = self.range {
            if r.intersect(node.text_range()).is_none() {
                return;
            }
        }
        match node.kind() {
            ref n if n.is_comment() => emit_node(node, SemanticTokenType::Comment, out),
            // Escape-processing literals: split out `\n`, `\xNN`, `\u{..}`, ...
            SyntaxKind::STRING_LITERAL | SyntaxKind::BYTE_STRING_LITERAL => {
                string_with_escapes(node, out);
            }
            // Raw / unquoted strings do not process escapes.
            SyntaxKind::RAW_STRING_LITERAL | SyntaxKind::UNQUOTED_STRING => {
                emit_node(node, SemanticTokenType::String, out);
            }
            SyntaxKind::BACKTICK_STRING_LITERAL => self.backtick_string(node, out),
            SyntaxKind::ATTRIBUTE | SyntaxKind::BLOCK_ATTRIBUTE => self.tokens(node, out, |t| {
                (matches!(t.kind(), SyntaxKind::AT_AT | SyntaxKind::AT | SyntaxKind::WORD)
                    || t.kind().is_keyword())
                .then_some(plain(SemanticTokenType::Decorator))
            }),
            SyntaxKind::TYPE_ALIAS_DEF | SyntaxKind::ASSOCIATED_TYPE_DECL => {
                self.type_decl(node, out);
            }
            SyntaxKind::ENUM_DEF => self.decl_name(node, SemanticTokenType::Enum, out),
            SyntaxKind::ENUM_VARIANT => self.decl_name(node, SemanticTokenType::EnumMember, out),
            SyntaxKind::CLASS_DEF => self.decl_name(node, SemanticTokenType::Class, out),
            SyntaxKind::INTERFACE_DEF => self.decl_name(node, SemanticTokenType::Interface, out),
            SyntaxKind::FIELD => self.decl_name(node, SemanticTokenType::Property, out),
            SyntaxKind::FUNCTION_DEF | SyntaxKind::METHOD_SIG => self.function_def(node, out),
            SyntaxKind::PARAMETER => self.decl_name(node, SemanticTokenType::Parameter, out),
            SyntaxKind::TYPE_EXPR => self.type_expr(node, out),
            // `let x`, match-arm bindings, etc.: the bound name is a declaration.
            SyntaxKind::BINDING_PATTERN => self.decl_name(node, SemanticTokenType::Variable, out),
            SyntaxKind::CLIENT_TYPE => self.tokens(node, out, |t| {
                (t.kind() == SyntaxKind::WORD).then_some(plain(SemanticTokenType::Type))
            }),
            SyntaxKind::CONFIG_ITEM => self.tokens(node, out, |t| {
                (t.kind().is_keyword() || t.kind() == SyntaxKind::WORD)
                    .then_some(plain(SemanticTokenType::Property))
            }),
            SyntaxKind::CLIENT_DEF | SyntaxKind::RETRY_POLICY_DEF => {
                self.decl_name(node, SemanticTokenType::Struct, out);
            }
            SyntaxKind::TEST_DEF => self.decl_name(node, SemanticTokenType::Struct, out),
            SyntaxKind::TEMPLATE_STRING_DEF => {
                self.decl_name(node, SemanticTokenType::Function, out);
            }
            SyntaxKind::PROMPT_FIELD => self.decl_name(node, SemanticTokenType::Property, out),
            SyntaxKind::OBJECT_LITERAL => self.object_literal(node, out),
            SyntaxKind::OBJECT_FIELD => self.object_field(node, out),
            SyntaxKind::CLIENT_FIELD => self.tokens(node, out, |t| {
                (t.kind() == SyntaxKind::KW_CLIENT).then_some(plain(SemanticTokenType::Property))
            }),
            // `as` is a contextual keyword (lexed as a WORD) in `.as<T>` casts
            // and `field as field` interface field links.
            SyntaxKind::UPCAST_EXPR | SyntaxKind::INTERFACE_FIELD_LINK => self.tokens(node, out, |t| {
                (t.kind() == SyntaxKind::WORD && t.text() == "as")
                    .then_some(plain(SemanticTokenType::Keyword))
            }),
            SyntaxKind::GENERATOR_DEF => self.generator_def(node, out),
            // A generic parameter declaration (`T` in `class Box<T>`,
            // `function f<T>()`, `<T: Bound>`): the name as a `TypeParameter`
            // declaration, any bound dispatched as a type.
            SyntaxKind::GENERIC_PARAM => {
                let mut named = false;
                self.tokens(node, out, |t| {
                    (!named && t.kind() == SyntaxKind::WORD).then(|| {
                        named = true;
                        decl(SemanticTokenType::TypeParameter)
                    })
                });
            }
            _ => self.children(node, out),
        }
    }

    /// Walk all children with no special token classification.
    fn children(&self, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
        self.tokens(node, out, |_| None);
    }

    /// Classify a single leaf token. A WORD consults the resolution index (an
    /// unresolved one is left neutral, never guessed); every other token is
    /// purely syntactic.
    fn token(&self, token: &SyntaxToken, out: &mut Vec<SemanticToken>) {
        let kind = token.kind();
        if kind.is_whitespace() {
            return;
        }
        // Boolean / null literals: a dedicated `KW_TRUE`/`KW_FALSE`/`KW_NULL`
        // token (value position, re-lexed by the parser). `true`/`false` ->
        // `boolean`, `null` -> `keyword`.
        match kind {
            SyntaxKind::KW_TRUE | SyntaxKind::KW_FALSE => {
                emit(token.text_range(), plain(SemanticTokenType::Boolean), out);
                return;
            }
            SyntaxKind::KW_NULL => {
                emit(token.text_range(), plain(SemanticTokenType::Keyword), out);
                return;
            }
            _ => {}
        }
        if kind == SyntaxKind::WORD {
            if let Some(class) = (self.resolve)(token.text_range()) {
                emit(token.text_range(), class, out);
            } else {
                // Transitional: type/config-position `true`/`false`/`null` are
                // still bare WORDs until those parse sites are re-lexed too; give
                // them the same classification as the `KW_*` value-position form.
                match token.text() {
                    "true" | "false" => {
                        emit(token.text_range(), plain(SemanticTokenType::Boolean), out);
                    }
                    "null" => emit(token.text_range(), plain(SemanticTokenType::Keyword), out),
                    _ => {}
                }
            }
            return;
        }
        let token_type = if kind.is_keyword() {
            SemanticTokenType::Keyword
        } else if kind.is_operator() {
            SemanticTokenType::Operator
        } else if kind.is_comment() {
            SemanticTokenType::Comment
        } else if matches!(kind, SyntaxKind::INTEGER_LITERAL | SyntaxKind::FLOAT_LITERAL) {
            SemanticTokenType::Number
        } else {
            return;
        };
        emit(token.text_range(), plain(token_type), out);
    }

    /// The one structural primitive: walk children, dispatching each node and
    /// classifying each token by `classify`. A `None` result falls back to the
    /// token's own syntactic classification ([`Self::token`]). Every node handler
    /// below is a thin wrapper over this.
    fn tokens(
        &self,
        node: &SyntaxNode,
        out: &mut Vec<SemanticToken>,
        mut classify: impl FnMut(&SyntaxToken) -> Option<Class>,
    ) {
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(n) => self.node(&n, out),
                NodeOrToken::Token(t) => match classify(&t) {
                    Some(class) => emit(t.text_range(), class, out),
                    None => self.token(&t, out),
                },
            }
        }
    }

    /// Direct WORD children as a declaration of `ty` (class/enum/field/... names,
    /// which never appear in the expression index).
    fn decl_name(&self, node: &SyntaxNode, ty: SemanticTokenType, out: &mut Vec<SemanticToken>) {
        self.tokens(node, out, |t| (t.kind() == SyntaxKind::WORD).then_some(decl(ty)));
    }

    /// A `TYPE_ALIAS_DEF` (`type X = …`), an associated-type *declaration*
    /// (`type Item [extends Bound] [= Default]` in an interface/impl), or an
    /// associated-type *binding* (`Item = string` inside `Iterator<…>`). All
    /// three share `ASSOCIATED_TYPE_DECL`/`TYPE_ALIAS_DEF`. The leading `type`
    /// keyword (a WORD in the grammar) is `Keyword`; the type name is `Type` —
    /// a declaration when introduced by `type`, otherwise a reference (a
    /// binding names an existing associated type). Bounds / values are child
    /// `TYPE_EXPR`s and dispatch on their own.
    fn type_decl(&self, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
        let mut saw_type_kw = false;
        let mut named = false;
        self.tokens(node, out, |t| {
            if t.kind() != SyntaxKind::WORD {
                return None;
            }
            if !named && t.text() == "type" {
                saw_type_kw = true;
                return Some(plain(SemanticTokenType::Keyword));
            }
            if !named {
                named = true;
                let ty = SemanticTokenType::Type;
                return Some(if saw_type_kw { decl(ty) } else { plain(ty) });
            }
            None
        });
    }

    /// A `TYPE_EXPR` — each (possibly dotted) type name resolved to what it
    /// names. A qualified name like `baml.iter.Iterator` resolves the whole path:
    /// the leaf is the resolved type, earlier segments are namespaces. Builtins
    /// (stdlib types, primitives) are marked `defaultLibrary`.
    fn type_expr(&self, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
        let children: Vec<_> = node.children_with_tokens().collect();
        let mut i = 0;
        while i < children.len() {
            // A type name starts at a WORD; gather any dotted continuation.
            if let NodeOrToken::Token(t) = &children[i] {
                if t.kind() == SyntaxKind::WORD {
                    let mut segments = vec![t.clone()];
                    let mut j = i + 1;
                    while let (Some(NodeOrToken::Token(dot)), Some(NodeOrToken::Token(word))) =
                        (children.get(j), children.get(j + 1))
                    {
                        if dot.kind() != SyntaxKind::DOT || word.kind() != SyntaxKind::WORD {
                            break;
                        }
                        emit(dot.text_range(), plain(SemanticTokenType::Operator), out);
                        segments.push(word.clone());
                        j += 2;
                    }
                    self.type_run(&segments, out);
                    i = j;
                    continue;
                }
            }
            match &children[i] {
                NodeOrToken::Node(n) => self.node(n, out),
                NodeOrToken::Token(t) => self.token(t, out),
            }
            i += 1;
        }
    }

    /// Classify one (possibly dotted) type name run.
    fn type_run(&self, segments: &[SyntaxToken], out: &mut Vec<SemanticToken>) {
        if let [single] = segments {
            let class =
                classify_type_token(self.db, self.file, single.text(), single.text_range().start());
            emit(single.text_range(), class, out);
            return;
        }
        // Qualified `a.b.Type`: prefix segments are namespaces; the leaf is the
        // type the whole path resolves to.
        let (leaf, prefix) = segments.split_last().expect("non-empty run");
        for seg in prefix {
            emit(seg.text_range(), plain(SemanticTokenType::Namespace), out);
        }
        let names: Vec<Name> = segments.iter().map(|t| Name::new(t.text())).collect();
        let resolved = resolve_path_at(self.db, self.file, leaf.text_range().start(), &names, None);
        // An unresolved qualified leaf is still a type, not a namespace.
        let class =
            classify::classify_resolved(&resolved).unwrap_or_else(|| plain(SemanticTokenType::Type));
        emit(leaf.text_range(), class, out);
    }

    /// A `FUNCTION_DEF` or `METHOD_SIG` — the name as a `Function` (or `Method`,
    /// inside a class/interface/implements) declaration; parameters, the return
    /// type, and the body are dispatched as child nodes (the body's identifiers
    /// resolve through the index).
    fn function_def(&self, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
        let name_type = if in_method_context(node) {
            SemanticTokenType::Method
        } else {
            SemanticTokenType::Function
        };
        self.decl_name(node, name_type, out);
    }

    /// An `OBJECT_LITERAL` — the constructed type name as a `Class` reference,
    /// then the body dispatched (field keys + value expressions). The name is a
    /// bare WORD for `Foo { … }` but a leading `PATH_EXPR` (`Foo<int>`) when the
    /// construction is generic.
    fn object_literal(&self, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
        let mut typed = false; // classified the constructed type name yet
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Token(t) if !typed && t.kind() == SyntaxKind::WORD => {
                    typed = true;
                    emit(t.text_range(), plain(SemanticTokenType::Class), out);
                }
                // `Foo<int> { … }` — name + generic args are a leading `PATH_EXPR`.
                NodeOrToken::Node(n) if !typed && n.kind() == SyntaxKind::PATH_EXPR => {
                    typed = true;
                    self.object_type_path(&n, out);
                }
                NodeOrToken::Node(n) => self.node(&n, out),
                NodeOrToken::Token(t) => self.token(&t, out),
            }
        }
    }

    /// The leading `Foo<...>` of a generic object construction: the type name as
    /// a `Class`; the `GENERIC_ARGS` dispatched so their type args resolve.
    fn object_type_path(&self, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
        let mut named = false;
        self.tokens(node, out, |t| {
            (!named && t.kind() == SyntaxKind::WORD).then(|| {
                named = true;
                plain(SemanticTokenType::Class)
            })
        });
    }

    /// A `BACKTICK_STRING_LITERAL` (BEP-049 interpolated string) — literal content
    /// (delimiters, text, punctuation) as `String`; interpolations (`${ expr }`)
    /// and block tags (`${for ...}`, `${if ...}`) are child nodes holding real
    /// code, dispatched so their identifiers resolve through the index.
    fn backtick_string(&self, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
        self.tokens(node, out, |t| {
            (!t.kind().is_whitespace()).then_some(plain(SemanticTokenType::String))
        });
    }

    /// A `GENERATOR_DEF` — the name as a `Struct` declaration. The `{ … }` body
    /// is parsed opaquely (generators are deprecated) into raw tokens, so the
    /// value strings aren't `STRING_LITERAL` nodes; classify by shape — `key:`
    /// words as `Property`, value tokens as `String`.
    fn generator_def(&self, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
        let mut body = false; // past the opening `{`
        let mut value = false; // after `:`, before `,` / `}`
        let mut named = false;
        self.tokens(node, out, |t| match t.kind() {
            SyntaxKind::KW_GENERATOR => Some(plain(SemanticTokenType::Keyword)),
            SyntaxKind::L_BRACE => {
                body = true;
                None
            }
            SyntaxKind::R_BRACE => {
                value = false;
                None
            }
            SyntaxKind::COLON if body => {
                value = true;
                None
            }
            SyntaxKind::COMMA if body => {
                value = false;
                None
            }
            SyntaxKind::WORD if !body && !named => {
                named = true;
                Some(decl(SemanticTokenType::Struct))
            }
            SyntaxKind::WORD if body && !value => Some(plain(SemanticTokenType::Property)),
            k if body && value && !k.is_whitespace() && !k.is_comment() => {
                Some(plain(SemanticTokenType::String))
            }
            _ => None,
        });
    }

    /// An `OBJECT_FIELD` (`a: expr` or `"a": expr`) — a bare-word key (before the
    /// `:`) as a `Property`; a string key and the value expression dispatched (so
    /// a value like `null` / `true` isn't mistaken for the key).
    fn object_field(&self, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
        let mut seen_colon = false;
        self.tokens(node, out, |t| match t.kind() {
            SyntaxKind::COLON => {
                seen_colon = true;
                None
            }
            SyntaxKind::WORD if !seen_colon => Some(plain(SemanticTokenType::Property)),
            _ => None,
        });
    }
}

// ── Type-name classification (annotations) ──────────────────────────────────────

/// Builtin primitive type keywords. These are not `Definition`s (so they don't
/// resolve to an item), but they are part of the language's standard surface, so
/// they are classified as `Type` with the `defaultLibrary` modifier.
const PRIMITIVE_TYPES: &[&str] = &[
    "int", "bigint", "float", "string", "bool", "bytes", "uint8array", "null", "image", "audio",
    "video", "pdf", "json", "map", "unknown", "never",
];

/// Classify a type-expression name token.
///
/// Resolves the name through the real resolver (the same path used by go-to-def),
/// so user types, dependency types, and `baml` stdlib types are all classified by
/// what they actually name. Builtins (resolved stdlib types and primitives) get
/// `defaultLibrary`. A name in type position that doesn't resolve is still a
/// type (e.g. a type parameter or an as-yet-undefined type) — only an explicit
/// path *prefix* is a namespace, which the caller handles.
fn classify_type_token(db: &dyn Db, file: SourceFile, name: &str, offset: TextSize) -> Class {
    if PRIMITIVE_TYPES.contains(&name) {
        return (SemanticTokenType::Type, ModifierSet::DEFAULT_LIBRARY);
    }
    let resolved = resolve_name_at(db, file, offset, &Name::new(name));
    classify::classify_resolved(&resolved).unwrap_or_else(|| plain(SemanticTokenType::Type))
}
