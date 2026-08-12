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
//!   *resolve to*, via a pre-built resolution index (`index`) keyed by exact
//!   name spans. The type system is never used to pick a tag; only resolution
//!   facts (`MemberResolution`, `ResolvedName`, `DefinitionKind`) are. There is
//!   no substring scanning.
//!
//! - **Declaration names** are classified by their declaring node and carry the
//!   `declaration` modifier; a reference is classified the same way as its
//!   definition.

use baml_base::{Name, SourceFile};
use baml_compiler_diagnostics::{HighlightAttributes, HighlightColor, HighlightStyle};
use baml_compiler_syntax::{
    SyntaxKind, SyntaxNode, SyntaxToken,
    ast::{
        CallArg, ClassDef, GenericParam, ImplementsBlock, ImplementsTarget, InterfaceFieldLink,
        ObjectField, TypeExpr,
    },
};
use baml_compiler2_ppir::resolve::{
    resolve_enum_variant, resolve_field, resolve_name_at, resolve_namespace_prefix, resolve_path_at,
};
use rowan::{NodeOrToken, WalkEvent, ast::AstNode};
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

/// Terminal style for a semantic token.
///
/// The named ANSI palette stays legible on light and dark backgrounds.
/// Modifiers overlay attributes the same way editor themes do.
pub fn semantic_highlight_style(
    token_type: SemanticTokenType,
    modifiers: ModifierSet,
) -> HighlightStyle {
    use SemanticTokenType as T;

    let (foreground, base_dim) = match token_type {
        T::Keyword | T::Modifier => (Some(HighlightColor::Magenta), false),
        T::Class | T::Struct | T::Interface | T::Enum | T::Type | T::TypeParameter => {
            (Some(HighlightColor::Yellow), false)
        }
        T::Function | T::Method | T::Macro => (Some(HighlightColor::BrightBlue), false),
        T::EnumMember | T::Property => (Some(HighlightColor::Cyan), false),
        T::Parameter => (Some(HighlightColor::Yellow), true),
        T::Namespace => (Some(HighlightColor::BrightCyan), false),
        T::String | T::Regexp => (Some(HighlightColor::Green), false),
        T::EscapeSequence => (Some(HighlightColor::BrightMagenta), false),
        T::Number | T::Boolean => (Some(HighlightColor::BrightYellow), false),
        T::Comment => (None, true),
        T::Decorator => (Some(HighlightColor::Magenta), true),
        T::Operator => (None, true),
        T::Variable | T::Event => (None, false),
    };
    let declaration = modifiers.contains(ModifierSet::DECLARATION);
    let mut attributes = HighlightAttributes::empty();
    if declaration {
        attributes.insert(HighlightAttributes::BOLD);
    }
    if base_dim && !declaration {
        attributes.insert(HighlightAttributes::DIM);
    }
    if modifiers.contains(ModifierSet::DEFAULT_LIBRARY) {
        attributes.insert(HighlightAttributes::ITALIC);
    }
    if modifiers.contains(ModifierSet::DEPRECATED) {
        attributes.insert(HighlightAttributes::STRIKETHROUGH);
    }
    HighlightStyle {
        foreground,
        attributes,
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
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
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
                emit(
                    span(base + text_start, base + i),
                    plain(SemanticTokenType::String),
                    out,
                );
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
/// Salsa tracked query: the CST walk re-classifies every token
/// against type inference, which measured 40–150ms on real projects — too
/// slow to recompute per request while the file is unchanged. Memoization
/// keys off the file revision; edits to *other* files reuse this file's
/// result if its inference inputs are unaffected. `returns(ref)` hands
/// borrowing callers the memoized vec without an O(tokens) clone per request.
#[salsa::tracked(returns(ref))]
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
    walk.run(&root, &mut out);
    out.sort_by_key(|token| (token.range.start(), token.range.end()));
    out
}

/// Semantic tokens for a viewport `range` only — rust-analyzer's
/// `highlight_range`. Names are resolved on demand through
/// `index::resolve_token_class`, so only the scopes the viewport touches are
/// indexed (the rest of the file is never resolved). Not a Salsa query — keying
/// on the range would blow the cache; the underlying per-scope indices and name
/// resolution it calls *are* memoized.
pub fn semantic_tokens_in_range(
    db: &dyn Db,
    file: SourceFile,
    start: u32,
    end: u32,
) -> Vec<SemanticToken> {
    let range = TextRange::new(start.into(), end.into());
    let root = baml_compiler_parser::syntax_tree(db, file);
    let walk = Walk {
        db,
        file,
        resolve: Box::new(move |r| index::resolve_token_class(db, file, r)),
        range: Some(range),
    };
    let mut out = Vec::new();
    walk.run(&root, &mut out);
    // The range gate is per-subtree; trim the boundary tokens to exactly `range`.
    out.retain(|t| range.intersect(t.range).is_some());
    out.sort_by_key(|token| (token.range.start(), token.range.end()));
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
    /// The single document-order driver: a flat `preorder_with_tokens()` walk
    /// (rust-analyzer's `traverse`). Each `Enter(Node)` either hands a complex
    /// subtree to its wholesale handler and skips it, or descends; each
    /// `Enter(Token)` is classified from its parent kind ([`classify_token`])
    /// with a syntactic fallback ([`Self::token`]).
    fn run(&self, root: &SyntaxNode, out: &mut Vec<SemanticToken>) {
        let mut preorder = root.preorder_with_tokens();
        while let Some(event) = preorder.next() {
            match event {
                WalkEvent::Enter(NodeOrToken::Node(node)) => {
                    // Range gate: a subtree disjoint from the viewport is skipped
                    // wholesale, so its tokens are never classified and its scope
                    // never resolved.
                    if let Some(r) = self.range {
                        if r.intersect(node.text_range()).is_none() {
                            preorder.skip_subtree();
                            continue;
                        }
                    }
                    // A node whose classification spans its whole subtree with
                    // custom traversal (strings, comments, type exprs, object
                    // literals, generators) is handled wholesale; skipping it
                    // prevents the flat loop from re-visiting its descendants.
                    if self.wholesale(&node, out) {
                        preorder.skip_subtree();
                    }
                }
                WalkEvent::Enter(NodeOrToken::Token(token)) => {
                    if token.kind().is_whitespace() {
                        continue;
                    }
                    match classify_token(&token) {
                        Some(class) => emit(token.text_range(), class, out),
                        None => self.token(&token, out),
                    }
                }
                WalkEvent::Leave(_) => {}
            }
        }
    }

    /// Classify a node whose logic spans its entire subtree, emitting all of its
    /// descendant tokens. Returns `true` if `node` was such a node (the caller
    /// then skips the subtree), `false` to let the flat loop descend normally.
    fn wholesale(&self, node: &SyntaxNode, out: &mut Vec<SemanticToken>) -> bool {
        if node.kind().is_comment() {
            emit_node(node, SemanticTokenType::Comment, out);
            return true;
        }
        match node.kind() {
            // Escape-processing literals: split out `\n`, `\xNN`, `\u{..}`, ...
            SyntaxKind::STRING_LITERAL | SyntaxKind::BYTE_STRING_LITERAL => {
                string_with_escapes(node, out);
            }
            // Raw / unquoted strings do not process escapes.
            SyntaxKind::RAW_STRING_LITERAL | SyntaxKind::UNQUOTED_STRING => {
                emit_node(node, SemanticTokenType::String, out);
            }
            SyntaxKind::BACKTICK_STRING_LITERAL => self.backtick_string(node, out),
            SyntaxKind::TYPE_EXPR => self.type_expr(node, out),
            SyntaxKind::OBJECT_LITERAL => self.object_literal(node, out),
            SyntaxKind::GENERATOR_DEF => self.generator_def(node, out),
            SyntaxKind::INTERFACE_FIELD_LINK => self.interface_field_link(node, out),
            _ => return false,
        }
        true
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
        // `boolean`. `null` is the null type's literal, so it goes through the
        // shared builtin classification (defaultLibrary `type`) — matching its
        // type position and every other builtin, instead of reading as a keyword.
        match kind {
            SyntaxKind::KW_TRUE | SyntaxKind::KW_FALSE => {
                emit(token.text_range(), plain(SemanticTokenType::Boolean), out);
                return;
            }
            SyntaxKind::KW_NULL => {
                let class = classify::classify_primitive(token.text())
                    .unwrap_or_else(|| plain(SemanticTokenType::Keyword));
                emit(token.text_range(), class, out);
                return;
            }
            _ => {}
        }
        if kind == SyntaxKind::WORD {
            if let Some(class) = (self.resolve)(token.text_range()) {
                emit(token.text_range(), class, out);
            }
            return;
        }
        if kind == SyntaxKind::DOT {
            if let Some(class) = self.path_namespace_separator(token) {
                emit(token.text_range(), class, out);
                return;
            }
        }
        let token_type = if kind.is_keyword() {
            SemanticTokenType::Keyword
        } else if kind.is_operator() {
            SemanticTokenType::Operator
        } else if kind.is_comment() {
            SemanticTokenType::Comment
        } else if matches!(
            kind,
            SyntaxKind::INTEGER_LITERAL | SyntaxKind::FLOAT_LITERAL | SyntaxKind::BIGINT_LITERAL
        ) {
            SemanticTokenType::Number
        } else {
            return;
        };
        emit(token.text_range(), plain(token_type), out);
    }

    /// Classify a `.` inside a value-position path as part of the namespace
    /// prefix only when everything to its left resolves as a real namespace.
    /// Member/enum/associated-type separators remain ordinary operators.
    fn path_namespace_separator(&self, token: &SyntaxToken) -> Option<Class> {
        let parent = token.parent()?;
        if parent.kind() != SyntaxKind::PATH_EXPR {
            return None;
        }

        let mut names = Vec::new();
        for element in parent.children_with_tokens() {
            match element {
                NodeOrToken::Token(t) if t.text_range() == token.text_range() => break,
                NodeOrToken::Token(t) if t.kind().is_trivia() || t.kind() == SyntaxKind::DOT => {}
                NodeOrToken::Token(t) if t.kind() == SyntaxKind::WORD => {
                    names.push(Name::new(t.text()));
                }
                // A generic argument list or any other structure before this
                // separator means its left side is a type/value, not a namespace.
                _ => return None,
            }
        }

        (!names.is_empty())
            .then(|| resolve_namespace_prefix(self.db, self.file, &names))
            .flatten()
            .map(classify::namespace_class)
    }

    /// A structural primitive for the wholesale handlers: walk a node's direct
    /// children, classifying each direct token by `classify` (with a syntactic
    /// fallback) and recursing each child node through [`Self::run`]. A `None`
    /// result falls back to the token's own classification ([`Self::token`]).
    fn tokens(
        &self,
        node: &SyntaxNode,
        out: &mut Vec<SemanticToken>,
        mut classify: impl FnMut(&SyntaxToken) -> Option<Class>,
    ) {
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(n) => self.run(&n, out),
                NodeOrToken::Token(t) => match classify(&t) {
                    Some(class) => emit(t.text_range(), class, out),
                    None => self.token(&t, out),
                },
            }
        }
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
                    let mut separators = Vec::new();
                    let mut j = i + 1;
                    while let (Some(NodeOrToken::Token(dot)), Some(NodeOrToken::Token(word))) =
                        (children.get(j), children.get(j + 1))
                    {
                        if dot.kind() != SyntaxKind::DOT || word.kind() != SyntaxKind::WORD {
                            break;
                        }
                        separators.push(dot.clone());
                        segments.push(word.clone());
                        j += 2;
                    }
                    self.type_run(&segments, &separators, out);
                    i = j;
                    continue;
                }
            }
            match &children[i] {
                NodeOrToken::Node(n) => self.run(n, out),
                NodeOrToken::Token(t) => self.token(t, out),
            }
            i += 1;
        }
    }

    /// Classify one (possibly dotted) type name run.
    fn type_run(
        &self,
        segments: &[SyntaxToken],
        separators: &[SyntaxToken],
        out: &mut Vec<SemanticToken>,
    ) {
        if let [single] = segments {
            let class = classify_type_token(
                self.db,
                self.file,
                single.text(),
                single.text_range().start(),
            );
            emit(single.text_range(), class, out);
            return;
        }
        // Qualified `a.b.Type`: resolve each prefix segment the same way the
        // value-position index does — a real namespace (builtin-flagged) or, if
        // not a namespace (e.g. the base type of an associated-type path), a
        // type. Never a blindly-guessed namespace.
        debug_assert_eq!(separators.len() + 1, segments.len());
        let names: Vec<Name> = segments.iter().map(|t| Name::new(t.text())).collect();
        let (leaf, prefix) = segments.split_last().expect("non-empty run");
        for (i, seg) in prefix.iter().enumerate() {
            let namespace = resolve_namespace_prefix(self.db, self.file, &names[0..=i]);
            let class = match namespace {
                Some(builtin) => classify::namespace_class(builtin),
                None => {
                    classify_type_token(self.db, self.file, seg.text(), seg.text_range().start())
                }
            };
            emit(seg.text_range(), class, out);
            emit(
                separators[i].text_range(),
                namespace
                    .map(classify::namespace_class)
                    .unwrap_or_else(|| plain(SemanticTokenType::Operator)),
                out,
            );
        }
        let resolved = resolve_path_at(self.db, self.file, leaf.text_range().start(), &names, None);
        let class = classify::classify_resolved(&resolved).unwrap_or_else(|| {
            // An unresolved qualified leaf: a verified enum variant (the prefix
            // resolves to an enum and the leaf is one of its variants) is an
            // `enumMember` — e.g. `Direction.North` in a match pattern, which is
            // a type expression. Otherwise it is still a type.
            if resolve_enum_variant(self.db, self.file, leaf.text_range().start(), &names) {
                plain(SemanticTokenType::EnumMember)
            } else {
                plain(SemanticTokenType::Type)
            }
        });
        emit(leaf.text_range(), class, out);
    }

    /// An `interface_field as class_field` link inside an `implements` block.
    /// The interface field resolves against the implemented interface and the
    /// class field against the enclosing class; each highlights as a `Property`
    /// only when it actually resolves (the `as` keyword stays syntactic).
    fn interface_field_link(&self, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
        let link = InterfaceFieldLink::cast(node.clone());
        // The class field's owning type is the enclosing class.
        let class_name: Option<Name> = node
            .ancestors()
            .find_map(ClassDef::cast)
            .and_then(|c| c.name())
            .map(|t| Name::new(t.text()));
        // The interface field's owning type is the implements-block target.
        let iface_segments: Vec<Name> = node
            .ancestors()
            .find_map(ImplementsBlock::cast)
            .and_then(|b| b.syntax().children().find_map(ImplementsTarget::cast))
            .and_then(|t| t.type_expr())
            .map(|te| type_path_segments(&te))
            .unwrap_or_default();
        let field_of = |segments: &[Name], tok: &SyntaxToken| {
            resolve_field(
                self.db,
                self.file,
                tok.text_range().start(),
                segments,
                &Name::new(tok.text()),
            )
            .then_some(plain(SemanticTokenType::Property))
        };
        self.tokens(node, out, |t| {
            let here = Some(t.text_range());
            if link
                .as_ref()
                .and_then(baml_compiler_syntax::InterfaceFieldLink::interface_field)
                .map(|x| x.text_range())
                == here
            {
                return field_of(&iface_segments, t);
            }
            if link
                .as_ref()
                .and_then(baml_compiler_syntax::InterfaceFieldLink::class_field)
                .map(|x| x.text_range())
                == here
            {
                return class_name
                    .as_ref()
                    .and_then(|cn| field_of(std::slice::from_ref(cn), t));
            }
            None
        });
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
                NodeOrToken::Node(n) => self.run(&n, out),
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
}

/// The leading dotted path segments of a type expression (`baml.x.Iface` ->
/// `[baml, x, Iface]`), stopping at any generic arguments.
fn type_path_segments(te: &TypeExpr) -> Vec<Name> {
    let mut segments = Vec::new();
    for element in te.syntax().children_with_tokens() {
        match element {
            NodeOrToken::Token(t) if t.kind().is_trivia() => {}
            NodeOrToken::Token(t) if t.kind() == SyntaxKind::WORD => {
                segments.push(Name::new(t.text()));
            }
            NodeOrToken::Token(t) if t.kind() == SyntaxKind::DOT => {}
            // Stop at generics (`<...>`) or anything past the path.
            _ => break,
        }
    }
    segments
}

/// Classify a single leaf token from its parent kind, reproducing what the old
/// per-node handlers emitted for direct child tokens. Stateful position checks
/// (which WORD is the name, a key before its `:`) are read off the typed AST or
/// preceding siblings. Returns `None` for tokens with no parent-driven class, so
/// the caller falls back to the syntactic classification ([`Walk::token`]).
fn classify_token(token: &SyntaxToken) -> Option<Class> {
    let parent = token.parent()?;
    let kind = token.kind();
    let word = kind == SyntaxKind::WORD;
    match parent.kind() {
        SyntaxKind::ATTRIBUTE | SyntaxKind::BLOCK_ATTRIBUTE => {
            (matches!(kind, SyntaxKind::AT_AT | SyntaxKind::AT | SyntaxKind::WORD)
                || kind.is_keyword())
            .then_some(plain(SemanticTokenType::Decorator))
        }
        SyntaxKind::CLIENT_TYPE => word.then_some(plain(SemanticTokenType::Type)),
        SyntaxKind::CONFIG_ITEM => {
            (kind.is_keyword() || word).then_some(plain(SemanticTokenType::Property))
        }
        SyntaxKind::CLIENT_FIELD => {
            (kind == SyntaxKind::KW_CLIENT).then_some(plain(SemanticTokenType::Property))
        }
        // A generic parameter declaration (`T` in `class Box<T>`, `<T: Bound>`):
        // the leading name as a `TypeParameter` declaration; a bound is a child
        // `TYPE_EXPR` and classifies on its own.
        SyntaxKind::GENERIC_PARAM => GenericParam::cast(parent)
            .and_then(|p| p.name())
            .filter(|name| name.text_range() == token.text_range())
            .map(|_| decl(SemanticTokenType::TypeParameter)),
        SyntaxKind::ENUM_DEF => word.then_some(decl(SemanticTokenType::Enum)),
        SyntaxKind::ENUM_VARIANT => word.then_some(decl(SemanticTokenType::EnumMember)),
        SyntaxKind::CLASS_DEF => word.then_some(decl(SemanticTokenType::Class)),
        SyntaxKind::INTERFACE_DEF => word.then_some(decl(SemanticTokenType::Interface)),
        SyntaxKind::FIELD | SyntaxKind::PROMPT_FIELD => {
            word.then_some(decl(SemanticTokenType::Property))
        }
        SyntaxKind::PARAMETER => word.then_some(decl(SemanticTokenType::Parameter)),
        // `let x`, match-arm bindings, etc.: the bound name is a declaration.
        SyntaxKind::BINDING_PATTERN => word.then_some(decl(SemanticTokenType::Variable)),
        // The catch binding(s) `catch (e, stack) { … }` — parameter-like locals
        // bound by the clause and scoped to its body (consistent with uses,
        // which resolve via DefinitionSite::CatchBinding -> parameter).
        SyntaxKind::CATCH_BINDING | SyntaxKind::CATCH_STACK_TRACE_BINDING => {
            word.then_some(decl(SemanticTokenType::Parameter))
        }
        SyntaxKind::CLIENT_DEF | SyntaxKind::RETRY_POLICY_DEF | SyntaxKind::TEST_DEF => {
            word.then_some(decl(SemanticTokenType::Struct))
        }
        SyntaxKind::TEMPLATE_STRING_DEF => word.then_some(decl(SemanticTokenType::Function)),
        // The name as a `Function`, or a `Method` inside a class / interface /
        // implements block.
        SyntaxKind::FUNCTION_DEF | SyntaxKind::METHOD_SIG => word.then(|| {
            decl(if in_method_context(&parent) {
                SemanticTokenType::Method
            } else {
                SemanticTokenType::Function
            })
        }),
        SyntaxKind::TYPE_ALIAS_DEF | SyntaxKind::ASSOCIATED_TYPE_DECL => {
            classify_type_decl_word(token)
        }
        // A bare-word key (before the `:`) is a `Property`; a string key and the
        // value expression are dispatched as nodes.
        SyntaxKind::OBJECT_FIELD => ObjectField::cast(parent)
            .and_then(|f| f.key())
            .filter(|key| key.text_range() == token.text_range())
            .map(|_| plain(SemanticTokenType::Property)),
        // A named argument `name = value` at a call site: the name refers to the
        // callee's parameter.
        SyntaxKind::CALL_ARG => CallArg::cast(parent)
            .and_then(|a| a.name())
            .filter(|name| name.text_range() == token.text_range())
            .map(|_| plain(SemanticTokenType::Parameter)),
        _ => None,
    }
}

/// Classify a WORD inside a `TYPE_ALIAS_DEF` (`type X = …`), an associated-type
/// *declaration* (`type Item [extends Bound] [= Default]`), or an associated-type
/// *binding* (`Item = string` inside `Iterator<…>`). The leading `type` keywords
/// (WORDs in the grammar) are `Keyword`; the type name is `Type` — a declaration
/// when introduced by `type`, otherwise a reference (a binding names an existing
/// associated type). Bounds / values are child `TYPE_EXPR`s classified on their
/// own.
fn classify_type_decl_word(token: &SyntaxToken) -> Option<Class> {
    if token.kind() != SyntaxKind::WORD {
        return None;
    }
    let parent = token.parent()?;
    // The name is the first direct WORD (bounds/values are child TYPE_EXPRs).
    let first_word = parent
        .children_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .find(|t| t.kind() == SyntaxKind::WORD)?;
    if first_word.text_range() != token.text_range() {
        return None;
    }
    // Introduced by a `type` keyword => a declaration; otherwise a binding.
    let is_decl = parent
        .children_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .any(|t| t.kind() == SyntaxKind::KW_TYPE);
    let ty = SemanticTokenType::Type;
    Some(if is_decl { decl(ty) } else { plain(ty) })
}

// ── Type-name classification (annotations) ──────────────────────────────────────

/// Builtin primitive type keywords. These are not `Definition`s (so they don't
/// resolve to an item), but they are part of the language's standard surface, so
/// they are classified as `Type` with the `defaultLibrary` modifier.
/// Classify a type-expression name token.
///
/// Resolves the name through the real resolver (the same path used by go-to-def),
/// so user types, dependency types, and `baml` stdlib types are all classified by
/// what they actually name. Builtins (resolved stdlib types and primitives) get
/// `defaultLibrary`. A name in type position that doesn't resolve is still a
/// type (e.g. a type parameter or an as-yet-undefined type) — only an explicit
/// path *prefix* is a namespace, which the caller handles.
fn classify_type_token(db: &dyn Db, file: SourceFile, name: &str, offset: TextSize) -> Class {
    if let Some(class) = classify::classify_primitive(name) {
        return class;
    }
    let resolved = resolve_name_at(db, file, offset, &Name::new(name));
    classify::classify_resolved(&resolved).unwrap_or_else(|| plain(SemanticTokenType::Type))
}
