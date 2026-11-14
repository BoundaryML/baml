//! Semantic tokens for BAML files.
//!
//! `semantic_tokens(db, file) -> Vec<SemanticToken>` is a single pass over the
//! token array in document order. Classification follows rust-analyzer's model:
//!
//! - **Structural tokens** (keywords, punctuation, strings, comments, numbers)
//!   are classified syntactically by token kind — the tree supplies only the
//!   node that owns a name.
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

use std::sync::Arc;

use baml_base::{Name, SourceFile};
use baml_compiler_diagnostics::{HighlightAttributes, HighlightColor, HighlightStyle};
use baml_compiler2_hir::resolve::{
    resolve_enum_variant, resolve_field, resolve_name_at, resolve_namespace_prefix, resolve_path_at,
};
use baml_syntax::{
    index::TreeIndex,
    nodes::{
        AnyObjectCtor, Arg, AssocTypeBinding, AssocTypeDecl, AssocTypeDef, Attribute,
        BindingPattern, BlockAttr, CatchClause, ClassDef, ClientField, ClientValueDef, DottedName,
        EnumDef, EnumVariant, Field, FileTree, FunctionDef, GenericApplyExpr, GenericParam,
        ImplementsBlock, ImplementsFor, InterfaceDef, InterfaceFieldLink, LetItem, MethodSig,
        Name as NameNode, ObjectField, ObjectLiteral, Param, PathExpr, PathType, PromptField,
        QualifiedPath, RemovedItem, ToolsField, TypeAliasDef, TypeBindingStmt,
    },
    ops::{AnyOperator, OpTok, Operator as _},
    token::{TokenIdx, TokenKind, TokenSet as _, sets::CommentToken},
    tree::{AnyNodeId, AnyNodeIdOf as _, Id},
};
use text_size::{TextRange, TextSize};

mod classify;
mod index;

// ── SemanticTokenType ─────────────────────────────────────────────────────────

/// The semantic token type for a BAML file.
///
/// The standard LSP semantic token types plus BAML extensions
/// (`escapeSequence`, and `boolean` mirroring rust-analyzer's). This crate
/// owns the enum so IDE features and the CLI painter share one legend without
/// any LSP dependency.
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
/// Derived from [`SemanticTokenType::ALL`], whose order is enforced against
/// [`SemanticTokenType::legend_index`]. The order MUST match what the server
/// advertises in its capabilities.
pub const TOKEN_TYPES: &[SemanticTokenType] = &SemanticTokenType::ALL;

impl SemanticTokenType {
    /// Every token type, in legend order.
    ///
    /// [`Self::legend_index`] is the enforcing source of the ordering; the
    /// round-trip unit test pins this array to it, so a variant can be
    /// neither missing here nor listed out of order without a test failure.
    pub const ALL: [SemanticTokenType; 25] = [
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

    /// The index of this token type in the [`TOKEN_TYPES`] legend — the
    /// `token_type` field of the LSP `SemanticToken` encoding.
    ///
    /// An exhaustive match, so adding a variant without assigning it a legend
    /// slot is a compile error rather than a runtime panic.
    pub fn legend_index(self) -> u32 {
        match self {
            Self::Namespace => 0,
            Self::Type => 1,
            Self::Class => 2,
            Self::Enum => 3,
            Self::Interface => 4,
            Self::Struct => 5,
            Self::TypeParameter => 6,
            Self::Parameter => 7,
            Self::Variable => 8,
            Self::Property => 9,
            Self::EnumMember => 10,
            Self::Event => 11,
            Self::Function => 12,
            Self::Method => 13,
            Self::Macro => 14,
            Self::Keyword => 15,
            Self::Modifier => 16,
            Self::Comment => 17,
            Self::String => 18,
            Self::Number => 19,
            Self::Regexp => 20,
            Self::Operator => 21,
            Self::Decorator => 22,
            Self::EscapeSequence => 23,
            Self::Boolean => 24,
        }
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
    /// [`TOKEN_MODIFIERS`], which is what the server capabilities advertise.
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
/// The order MUST match the bit positions in [`ModifierSet`] and what the
/// server advertises in its capabilities.
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
#[derive(Debug, Clone, PartialEq, Eq, salsa::SalsaValue)]
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

/// Push a classified token.
fn emit(range: TextRange, class: Class, out: &mut Vec<SemanticToken>) {
    out.push(SemanticToken {
        range,
        token_type: class.0,
        modifiers: class.1,
    });
}

/// The token runs of one string literal's text: `String` runs interleaved
/// with `EscapeSequence` runs, contiguous over `text`, offset by `base`.
/// Every run boundary lies on a `char` boundary of `text`, so each range is
/// encodable as an editor position.
fn emit_string_runs(text: &str, base: TextSize, out: &mut Vec<SemanticToken>) {
    let span = |from: usize, to: usize| {
        let at = |offset: usize| {
            base + TextSize::try_from(offset)
                .unwrap_or_else(|_| unreachable!("literal offsets fit the file's u32 text size"))
        };
        TextRange::new(at(from), at(to))
    };

    let bytes = text.as_bytes();
    let mut i = 0;
    let mut text_start = 0;
    // Byte-stepping is char-safe: only the ASCII `\` is ever matched at `i`,
    // and `escape_len` always ends on a char boundary.
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            if i > text_start {
                emit(span(text_start, i), plain(SemanticTokenType::String), out);
            }
            let len = escape_len(&text[i..]);
            debug_assert!(text.is_char_boundary(i + len));
            emit(
                span(i, i + len),
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
            span(text_start, bytes.len()),
            plain(SemanticTokenType::String),
            out,
        );
    }
}

/// Byte length of the escape sequence at the start of `s` (which begins `\`).
///
/// The result always lies on a `char` boundary of `s`: hex-digit scanning is
/// ASCII-only and a plain escaped character is measured by its UTF-8 length,
/// so an `EscapeSequence` range can never split a multi-byte character. An
/// unterminated `\u{` ends after its digits (at worst at the literal's end)
/// rather than extending to an unrelated `}` later in the text.
fn escape_len(s: &str) -> usize {
    debug_assert!(s.starts_with('\\'));
    let rest = &s[1..];
    let Some(kind) = rest.chars().next() else {
        // A trailing backslash (shouldn't occur before the closing quote).
        return 1;
    };
    match kind {
        // `\xNN` — backslash, `x`, then at most two hex digits (fewer when
        // the next char isn't one).
        'x' => 2 + hex_digit_run(&rest[1..], 2),
        // `\u{...}` — hex digits (and `_` separators) through the closing
        // brace, which must directly follow the digits to count as part of
        // the escape.
        'u' if rest[1..].starts_with('{') => {
            let digits = rest[2..]
                .bytes()
                .take_while(|b| b.is_ascii_hexdigit() || *b == b'_')
                .count();
            let terminated = rest[2 + digits..].starts_with('}');
            3 + digits + usize::from(terminated)
        }
        // `\uHHHH` — backslash, `u`, then at most four hex digits.
        'u' => 2 + hex_digit_run(&rest[1..], 4),
        // `\n`, `\t`, `\r`, `\0`, `\\`, `\"`, `\'`, ... —
        // the backslash plus one escaped char of whatever UTF-8 length.
        c => 1 + c.len_utf8(),
    }
}

/// Byte length of the leading ASCII-hex-digit run of `s`, capped at `cap`.
fn hex_digit_run(s: &str, cap: usize) -> usize {
    s.bytes()
        .take(cap)
        .take_while(u8::is_ascii_hexdigit)
        .count()
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Compute semantic tokens for a file.
///
/// Always returns tokens in document order (required by the LSP
/// `textDocument/semanticTokens/full` contract).
///
/// Salsa tracked query: the walk re-classifies every token against type
/// inference, which measured 40–150ms on real projects — too slow to
/// recompute per request while the file is unchanged. Memoization keys off
/// the file revision; edits to *other* files reuse this file's result if its
/// inference inputs are unaffected. `returns(ref)` hands borrowing callers the
/// memoized vec without an O(tokens) clone per request.
#[salsa::tracked(returns(ref))]
pub fn semantic_tokens(db: &dyn baml_compiler2_hir::Db, file: SourceFile) -> Vec<SemanticToken> {
    // Full document: classify every token, so build the merged whole-file index
    // (itself a merge of per-scope salsa-cached indices) and resolve from it.
    let index = index::build(db, file);
    let walk = Walk {
        db,
        file,
        tree: baml_parser::typed_tree(db, file),
        index: baml_parser::tree_index(db, file),
        resolve: Box::new(move |range| index.get(&range).copied()),
    };
    let mut out = Vec::new();
    walk.run(None, &mut out);
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
    db: &dyn baml_compiler2_hir::Db,
    file: SourceFile,
    start: u32,
    end: u32,
) -> Vec<SemanticToken> {
    let range = TextRange::new(start.into(), end.into());
    let walk = Walk {
        db,
        file,
        tree: baml_parser::typed_tree(db, file),
        index: baml_parser::tree_index(db, file),
        resolve: Box::new(move |r| index::resolve_token_class(db, file, r)),
    };
    let mut out = Vec::new();
    walk.run(Some(range), &mut out);
    // A token's runs are emitted whole; trim to exactly `range`.
    out.retain(|t| range.intersect(t.range).is_some());
    out.sort_by_key(|token| (token.range.start(), token.range.end()));
    out
}

// ── Walker ─────────────────────────────────────────────────────────────────────

/// The untyped id of a typed node id.
fn any<T: baml_syntax::tree::NodeKindOf>(id: Id<T>) -> AnyNodeId {
    id.any_node_id()
        .unwrap_or_else(|| unreachable!("a typed id is a node"))
}

/// A document-order walk over the token array that classifies each token
/// exactly once.
///
/// `resolve` resolves an identifier inside an expression body to its
/// classification; a declaration name is classified by the node that owns
/// it; every other token is syntactic. For a full-document walk `resolve` is
/// the merged whole-file index; a range walk resolves on demand per scope
/// (rust-analyzer's `Semantics::resolve` model — only visited scopes pay).
struct Walk<'db> {
    db: &'db dyn baml_compiler2_hir::Db,
    file: SourceFile,
    tree: Arc<FileTree>,
    index: Arc<TreeIndex>,
    resolve: Box<dyn Fn(TextRange) -> Option<Class> + 'db>,
}

impl Walk<'_> {
    /// Every token, or those intersecting `range`, in order.
    fn run(&self, range: Option<TextRange>, out: &mut Vec<SemanticToken>) {
        let tokens = self.tree.tokens();
        // Tokens a multi-token operator already covered.
        let mut skip = 0;
        for (n, idx) in tokens.iter().enumerate() {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            // Both the file-wide and viewport walks re-classify every token
            // they visit, so give salsa a cancellation probe now and then: an
            // edit landing mid-walk unwinds (`salsa::Cancelled`) instead of
            // finishing a stale highlight.
            if n % 256 == 0 {
                self.db.unwind_if_revision_cancelled();
            }
            let token_range = tokens.range(idx);
            if range.is_some_and(|range| range.intersect(token_range).is_none()) {
                continue;
            }
            skip = self.token(idx, out);
        }
    }

    fn text(&self, idx: TokenIdx) -> &str {
        self.tree.token_text(idx)
    }

    fn range(&self, idx: TokenIdx) -> TextRange {
        self.tree.tokens().range(idx)
    }

    fn parent(&self, id: AnyNodeId) -> Option<AnyNodeId> {
        self.index.parent(id)
    }

    /// Whether any ancestor of `id` (itself excluded) satisfies `test`.
    fn enclosed_by(&self, id: AnyNodeId, test: impl Fn(AnyNodeId) -> bool) -> bool {
        self.index.ancestors(id).skip(1).any(test)
    }

    /// Classify one token: by its kind where that settles it, by its owner
    /// for a name, with the resolution index as the fallback for a name in
    /// value position. Returns how many further tokens the classification
    /// covered (a multi-token operator such as `>>=`).
    fn token(&self, idx: TokenIdx, out: &mut Vec<SemanticToken>) -> usize {
        let tokens = self.tree.tokens();
        let kind = tokens.kind(idx);
        let range = self.range(idx);
        if CommentToken::KINDS.contains(kind) {
            emit(range, plain(SemanticTokenType::Comment), out);
            return 0;
        }
        if kind.is_trivia() {
            return 0;
        }
        match kind {
            // Escape-processing literals: split out `\n`, `\xNN`, `\u{..}`, ...
            TokenKind::String
            | TokenKind::ByteString
            | TokenKind::UnclosedString
            | TokenKind::UnclosedByteString => {
                emit_string_runs(self.text(idx), range.start(), out);
                return 0;
            }
            // Raw strings process no escapes; a template's literal text,
            // delimiters included, is string too.
            TokenKind::RawString
            | TokenKind::LegacyHashString
            | TokenKind::UnclosedRawString
            | TokenKind::UnclosedHashString
            | TokenKind::TemplateChunk
            | TokenKind::BacktickStart
            | TokenKind::BacktickEnd => {
                emit(range, plain(SemanticTokenType::String), out);
                return 0;
            }
            // `true`/`false` -> `boolean`. `null` is the null type's literal,
            // so it goes through the shared builtin classification
            // (defaultLibrary `type`) — matching its type position and every
            // other builtin, instead of reading as a keyword.
            TokenKind::True | TokenKind::False => {
                emit(range, plain(SemanticTokenType::Boolean), out);
                return 0;
            }
            TokenKind::Null => {
                let class = classify::classify_primitive(self.text(idx))
                    .unwrap_or_else(|| plain(SemanticTokenType::Keyword));
                emit(range, class, out);
                return 0;
            }
            TokenKind::Integer | TokenKind::Float | TokenKind::Bigint => {
                if self.index.token_owner(idx).is_some_and(|owner| {
                    owner.cast::<baml_syntax::nodes::TupleFieldExpr>().is_some()
                }) {
                    let mut start = range.start();
                    for part in self.text(idx).split('.') {
                        let end = start + TextSize::of(part);
                        emit(
                            TextRange::new(start, end),
                            plain(SemanticTokenType::Property),
                            out,
                        );
                        start = end + TextSize::from(1);
                    }
                    return 0;
                }
                emit(range, plain(SemanticTokenType::Number), out);
                return 0;
            }
            TokenKind::At | TokenKind::AtAt => {
                emit(range, plain(SemanticTokenType::Decorator), out);
                return 0;
            }
            TokenKind::Dot => {
                if let Some(class) = self.path_namespace_separator(idx) {
                    emit(range, class, out);
                }
                return 0;
            }
            // `?.` is member access, not an operator the grammar tables, but
            // it reads as one.
            TokenKind::QuestionDot => {
                emit(range, plain(SemanticTokenType::Operator), out);
                return 0;
            }
            _ => {}
        }
        if kind == TokenKind::Word || kind.is_contextual_keyword() {
            if let Some(class) = self.word(idx) {
                emit(range, class, out);
            }
            return 0;
        }
        if kind.is_keyword() {
            // A keyword the grammar accepted as a name (`client = …` as a
            // named argument, `x.client`, a field called `client`) is that
            // name; the LLM body's `client:` key declares a property.
            let as_name = self.index.token_owner(idx).is_some_and(|owner| {
                owner.cast::<NameNode>().is_some() || owner.cast::<ClientField>().is_some()
            });
            if as_name {
                if let Some(class) = self.word(idx) {
                    emit(range, class, out);
                }
                return 0;
            }
            emit(range, plain(SemanticTokenType::Keyword), out);
            return 0;
        }
        // An operator of any family, spelled by one or more adjacent tokens
        // (`??`, `>>`, `>=`, `>>=`), painted as one run.
        if let Some(op) = OpTok::<AnyOperator>::at(tokens, idx) {
            let last = op
                .tokens()
                .last()
                .unwrap_or_else(|| unreachable!("an operator has at least one token"));
            emit(
                TextRange::new(range.start(), tokens.range(last).end()),
                plain(SemanticTokenType::Operator),
                out,
            );
            return op.op().token_len() - 1;
        }
        0
    }

    /// The class of a word, from the node that owns it.
    fn word(&self, idx: TokenIdx) -> Option<Class> {
        let range = self.range(idx);
        let Some(owner) = self.index.token_owner(idx) else {
            return (self.resolve)(range);
        };
        let Some(name) = owner.cast::<NameNode>() else {
            // A contextual keyword used as one (`type`, `with`, …), or a
            // word the parser could not place.
            return self.keyword_word(idx, owner);
        };
        let Some(parent) = self.parent(owner) else {
            return (self.resolve)(range);
        };
        let class = if let Some(class_def) = parent.cast::<ClassDef>() {
            (self.tree.get(class_def).name == Ok(name)).then_some(decl(SemanticTokenType::Class))
        } else if let Some(enum_def) = parent.cast::<EnumDef>() {
            (self.tree.get(enum_def).name == Ok(name)).then_some(decl(SemanticTokenType::Enum))
        } else if parent.cast::<EnumVariant>().is_some() {
            Some(decl(SemanticTokenType::EnumMember))
        } else if let Some(iface) = parent.cast::<InterfaceDef>() {
            (self.tree.get(iface).name == Ok(name)).then_some(decl(SemanticTokenType::Interface))
        } else if parent.cast::<Field>().is_some() {
            Some(decl(SemanticTokenType::Property))
        } else if parent.cast::<Param>().is_some() {
            Some(decl(SemanticTokenType::Parameter))
        } else if parent.cast::<BindingPattern>().is_some() {
            // `let x`, match-arm bindings, etc.: the bound name is a
            // declaration — a `readonly` one for a global variable, which
            // cannot be reassigned.
            let (ty, modifiers) = decl(SemanticTokenType::Variable);
            let global = self.enclosed_by(parent, |a| a.cast::<LetItem>().is_some());
            Some(if global {
                (ty, modifiers | ModifierSet::READONLY)
            } else {
                (ty, modifiers)
            })
        } else if parent.cast::<CatchClause>().is_some() {
            // The catch binding(s) `catch (e, stack) { … }` — parameter-like
            // locals bound by the clause and scoped to its body.
            Some(decl(SemanticTokenType::Parameter))
        } else if parent.cast::<FunctionDef>().is_some() || parent.cast::<MethodSig>().is_some() {
            // The name as a `Function`, or a `Method` inside a class /
            // interface / implements block.
            let method = self.enclosed_by(parent, |a| {
                a.cast::<ClassDef>().is_some()
                    || a.cast::<InterfaceDef>().is_some()
                    || a.cast::<ImplementsBlock>().is_some()
                    || a.cast::<ImplementsFor>().is_some()
            });
            Some(decl(if method {
                SemanticTokenType::Method
            } else {
                SemanticTokenType::Function
            }))
        } else if parent.cast::<TypeAliasDef>().is_some()
            || parent.cast::<AssocTypeDecl>().is_some()
            || parent.cast::<AssocTypeDef>().is_some()
            || parent.cast::<AssocTypeBinding>().is_some()
        {
            // An alias, an interface's associated type, and its binding in
            // an implementation (`type Item = …`) or among generic arguments
            // (`Item = …`) each declare the name they bind.
            Some(decl(SemanticTokenType::Type))
        } else if parent.cast::<GenericParam>().is_some()
            || parent.cast::<TypeBindingStmt>().is_some()
        {
            Some(decl(SemanticTokenType::TypeParameter))
        } else if parent.cast::<Arg>().is_some() {
            // A named argument `name = value` at a call site: the name
            // refers to the callee's parameter.
            Some(plain(SemanticTokenType::Parameter))
        } else if parent.cast::<ClientValueDef>().is_some()
            || parent.cast::<RemovedItem>().is_some()
        {
            Some(decl(SemanticTokenType::Struct))
        } else if let Some(link) = parent.cast::<InterfaceFieldLink>() {
            self.interface_field_link(link, name, idx)
        } else if let Some(dotted) = parent.cast::<DottedName>() {
            let grandparent = self.parent(any(dotted))?;
            if grandparent.cast::<Attribute>().is_some()
                || grandparent.cast::<BlockAttr>().is_some()
            {
                Some(plain(SemanticTokenType::Decorator))
            } else if grandparent.cast::<ObjectField>().is_some() {
                // A bare-word key (before the `:`) is a `Property`.
                Some(plain(SemanticTokenType::Property))
            } else {
                None
            }
        } else if let Some(path) = parent.cast::<PathType>() {
            Some(self.type_path_name(path, name, idx))
        } else if let Some(path) = parent.cast::<PathExpr>() {
            // The constructed type name of `Foo { … }` / `Foo<int> { … }` as
            // a `Class` reference; any other value path resolves.
            if self.constructs(any(path)) && self.tree.get(path).head == name {
                Some(plain(SemanticTokenType::Class))
            } else {
                (self.resolve)(range)
            }
        } else if parent.cast::<QualifiedPath>().is_some() {
            // `(B as Shows).show`: the base and interface are type nodes of
            // their own; the only name owned directly is the projected
            // member, which resolves like any value-position name.
            (self.resolve)(range)
        } else {
            None
        };
        class.or_else(|| (self.resolve)(range))
    }

    /// Whether `path` is the constructor of an object literal, directly or
    /// as the base of its generic application.
    fn constructs(&self, path: AnyNodeId) -> bool {
        let Some(parent) = self.parent(path) else {
            return false;
        };
        if let Some(literal) = parent.cast::<ObjectLiteral>() {
            return matches!(self.tree.get(literal).ctor, AnyObjectCtor::Path(_));
        }
        if let Some(apply) = parent.cast::<GenericApplyExpr>() {
            return self
                .parent(any(apply))
                .and_then(AnyNodeId::cast::<ObjectLiteral>)
                .is_some_and(|literal| {
                    matches!(self.tree.get(literal).ctor, AnyObjectCtor::Generic(_))
                });
        }
        false
    }

    /// A contextual keyword's token owned by a node other than a `Name`:
    /// the LLM body's field keys are properties, every other spelling is the
    /// keyword it reads as.
    fn keyword_word(&self, idx: TokenIdx, owner: AnyNodeId) -> Option<Class> {
        if owner.cast::<PromptField>().is_some()
            || owner.cast::<ClientField>().is_some()
            || owner.cast::<ToolsField>().is_some()
        {
            // The key of an LLM body field declares the body's property.
            return Some(decl(SemanticTokenType::Property));
        }
        if self.tree.tokens().kind(idx).is_contextual_keyword() {
            return Some(plain(SemanticTokenType::Keyword));
        }
        (self.resolve)(self.range(idx))
    }

    /// Classify a `.` inside a value-position path as part of the namespace
    /// prefix only when everything to its left resolves as a real namespace.
    /// Member/enum/associated-type separators remain ordinary operators.
    fn path_namespace_separator(&self, idx: TokenIdx) -> Option<Class> {
        let owner = self.index.token_owner(idx)?;
        // A namespace prefix paints its dots the same way in a value path
        // (`baml.random.Rng.new`) and in a type path (`baml.random.Rng`).
        let (head, segments) = if let Some(path) = owner.cast::<PathExpr>() {
            let path = self.tree.get(path);
            (path.head, path.segments)
        } else {
            let path = self.tree.get(owner.cast::<PathType>()?);
            (path.head, path.segments)
        };
        let mut names = vec![Name::new(self.text(self.tree.get(head).token.idx))];
        for segment in self.tree.list(segments) {
            if segment.dot.idx == idx {
                break;
            }
            let name = segment.name.ok()?;
            names.push(Name::new(self.text(self.tree.get(name).token.idx)));
        }
        resolve_namespace_prefix(self.db, self.file, &names).map(classify::namespace_class)
    }

    /// The names of a type path, head first.
    fn type_path_names(&self, path: Id<PathType>) -> Vec<(Id<NameNode>, Name)> {
        let path = self.tree.get(path);
        let mut names = vec![(
            path.head,
            Name::new(self.text(self.tree.get(path.head).token.idx)),
        )];
        for segment in self.tree.list(path.segments) {
            if let Ok(name) = segment.name {
                names.push((name, Name::new(self.text(self.tree.get(name).token.idx))));
            }
        }
        names
    }

    /// One name of a (possibly dotted) type path: a prefix segment is a
    /// namespace (builtin-flagged) or, if not a namespace (e.g. the base type
    /// of an associated-type path), a type; the leaf resolves as a whole.
    fn type_path_name(&self, path: Id<PathType>, name: Id<NameNode>, idx: TokenIdx) -> Class {
        let range = self.range(idx);
        let text = self.text(idx);
        let names = self.type_path_names(path);
        let position = names.iter().position(|(id, _)| *id == name);
        let names: Vec<Name> = names.into_iter().map(|(_, name)| name).collect();
        match position {
            Some(i) if i + 1 < names.len() => {
                match resolve_namespace_prefix(self.db, self.file, &names[0..=i]) {
                    Some(builtin) => classify::namespace_class(builtin),
                    None => classify_type_token(self.db, self.file, text, range.start()),
                }
            }
            Some(_) if names.len() > 1 => {
                let resolved = resolve_path_at(self.db, self.file, range.start(), &names, None);
                classify::classify_resolved(&resolved).unwrap_or_else(|| {
                    // An unresolved qualified leaf: a verified enum variant (the
                    // prefix resolves to an enum and the leaf is one of its
                    // variants) is an `enumMember` — e.g. `Direction.North` in a
                    // match pattern. Otherwise it is still a type.
                    if resolve_enum_variant(self.db, self.file, range.start(), &names) {
                        plain(SemanticTokenType::EnumMember)
                    } else {
                        plain(SemanticTokenType::Type)
                    }
                })
            }
            _ => classify_type_token(self.db, self.file, text, range.start()),
        }
    }

    /// An `interface_field as class_field` link inside an `implements` block.
    /// The interface field resolves against the implemented interface and the
    /// class field against the enclosing class; each highlights as a `Property`
    /// only when it actually resolves.
    fn interface_field_link(
        &self,
        link: Id<InterfaceFieldLink>,
        name: Id<NameNode>,
        idx: TokenIdx,
    ) -> Option<Class> {
        let link_node = self.tree.get(link);
        let owner_segments: Vec<Name> = if link_node.iface_field == name {
            // The interface field's owning type is the implements-block target.
            let block = self
                .index
                .ancestors(any(link))
                .find_map(AnyNodeId::cast::<ImplementsBlock>)?;
            match self.tree.get(block).target.ok()? {
                baml_syntax::nodes::AnyType::Path(path) => self
                    .type_path_names(path)
                    .into_iter()
                    .map(|(_, name)| name)
                    .collect(),
                _ => return None,
            }
        } else if link_node.class_field == Ok(name) {
            // The class field's owning type is the enclosing class.
            let class = self
                .index
                .ancestors(any(link))
                .find_map(AnyNodeId::cast::<ClassDef>)?;
            let class_name = self.tree.get(class).name.ok()?;
            vec![Name::new(self.text(self.tree.get(class_name).token.idx))]
        } else {
            return None;
        };
        resolve_field(
            self.db,
            self.file,
            self.range(idx).start(),
            &owner_segments,
            &Name::new(self.text(idx)),
        )
        .then_some(plain(SemanticTokenType::Property))
    }
}

// ── Type-name classification (annotations) ──────────────────────────────────────

/// Classify a type-expression name token.
///
/// Resolves the name through the real resolver (the same path used by go-to-def),
/// so user types, dependency types, and `baml` stdlib types are all classified by
/// what they actually name. Builtins (resolved stdlib types and primitives) get
/// `defaultLibrary`. A name in type position that doesn't resolve is still a
/// type (e.g. a type parameter or an as-yet-undefined type) — only an explicit
/// path *prefix* is a namespace, which the caller handles.
fn classify_type_token(
    db: &dyn baml_compiler2_hir::Db,
    file: SourceFile,
    name: &str,
    offset: TextSize,
) -> Class {
    if let Some(class) = classify::classify_primitive(name) {
        return class;
    }
    let resolved = resolve_name_at(db, file, offset, &Name::new(name));
    classify::classify_resolved(&resolved).unwrap_or_else(|| plain(SemanticTokenType::Type))
}

#[cfg(test)]
mod tests {
    use std::{fmt::Write as _, path::Path};

    use baml_db::ProjectDatabase;

    use super::*;
    use crate::test_support::TestDbExt;

    // ── escape_len ────────────────────────────────────────────────────────────

    #[test]
    fn unterminated_unicode_escape_stops_after_its_digits() {
        // No digits before the non-hex char: the escape is just `\u{` — it
        // must not swallow up to the unrelated `}` later in the literal.
        assert_eq!(escape_len(r"\u{ 12} more"), r"\u{".len());
        // Digits, then no closing brace: the escape ends after the digits.
        assert_eq!(escape_len(r"\u{12 tail"), r"\u{12".len());
        // Unterminated at the literal's very end.
        assert_eq!(escape_len(r"\u{1F600"), r"\u{1F600".len());
        // A terminated escape still includes its closing brace, and no more.
        assert_eq!(escape_len(r"\u{1F600} x"), r"\u{1F600}".len());
    }

    #[test]
    fn unicode_escapes_span_their_digits() {
        assert_eq!(escape_len(r"\u00E9 x"), r"\u00E9".len());
        assert_eq!(escape_len(r"\u00E9F"), r"\u00E9".len());
        assert_eq!(escape_len(r"\u12 x"), r"\u12".len());
        assert_eq!(escape_len(r"\u{1F_600} x"), r"\u{1F_600}".len());
    }

    #[test]
    fn escape_len_always_ends_on_a_char_boundary() {
        // `\x` adjacent to a multi-byte char: the old byte count (4) split 🐑.
        assert_eq!(escape_len("\\x🐑"), r"\x".len());
        assert_eq!(escape_len("\\x4🐑"), r"\x4".len());
        // A backslash escaping a multi-byte char spans the whole char.
        assert_eq!(escape_len("\\🐑"), 1 + '🐑'.len_utf8());
        for s in ["\\x🐑", "\\x4🐑", "\\🐑", "\\u{1F600}🐑", "\\u{🐑}"] {
            assert!(s.is_char_boundary(escape_len(s)), "{s:?}");
        }
    }

    #[test]
    fn string_runs_tile_the_literal_on_char_boundaries() {
        let text = "\"a\\u{ 12}\u{1F411}\\x41 b\\\u{1F411}c\"";
        let mut out = Vec::new();
        emit_string_runs(text, TextSize::new(0), &mut out);

        // The runs are contiguous, cover the whole literal, and every
        // boundary is a char boundary (i.e. each range is sliceable).
        let mut cursor = 0usize;
        for token in &out {
            assert_eq!(usize::from(token.range.start()), cursor);
            cursor = usize::from(token.range.end());
            assert!(text.is_char_boundary(cursor), "{token:?}");
        }
        assert_eq!(cursor, text.len());

        let escapes: Vec<&str> = out
            .iter()
            .filter(|t| t.token_type == SemanticTokenType::EscapeSequence)
            .map(|t| &text[usize::from(t.range.start())..usize::from(t.range.end())])
            .collect();
        assert_eq!(escapes, [r"\u{", r"\x41", "\\\u{1F411}"]);
    }

    // ── Legend ────────────────────────────────────────────────────────────────

    #[test]
    fn legend_round_trips_every_token_type() {
        for (index, token_type) in TOKEN_TYPES.iter().enumerate() {
            assert_eq!(
                token_type.legend_index(),
                u32::try_from(index).unwrap(),
                "{token_type:?} is out of legend order"
            );
        }
    }

    // ── End-to-end walks ──────────────────────────────────────────────────────

    /// An interface projection reads a member like any other receiver, so its
    /// member name carries the member's kind — and ONLY the member name. The
    /// span comes from the recorded member-name span; the fallback (the whole
    /// `(B as Shows).show`) would have painted the receiver too.
    #[test]
    fn an_interface_projection_colours_only_its_member_name() {
        let (db, file) = test_db(
            r#"interface Shows {
    function show(self) -> string throws never
}

class B { v: int }

implement Shows for B {
    function show(self) -> string throws never { "b" }
}

function projection(b: B) -> string throws never { (B as Shows).show(b) }
"#,
        );
        let text = file.text(&db);
        let member = text
            .rfind(".show")
            .unwrap_or_else(|| unreachable!("the fixture writes the projection last"))
            + 1;
        let tokens = semantic_tokens(&db, file);
        let painted = tokens
            .iter()
            .find(|token| usize::from(token.range.start()) == member)
            .unwrap_or_else(|| unreachable!("the projection's member name is classified"));
        assert_eq!(
            &text[usize::from(painted.range.start())..usize::from(painted.range.end())],
            "show",
            "only the name, not the receiver"
        );
        assert_eq!(painted.token_type, SemanticTokenType::Method);
    }

    /// `let type T extends Named = t else { … };`: the keywords are keywords,
    /// `T` is a type-parameter declaration, and the bound is a type reference.
    #[test]
    fn let_type_declares_a_type_parameter() {
        let (db, file) = test_db(
            r#"interface Named { name string }

function f(t: reflect.Type) -> reflect.Type {
    let type T extends Named = t else { return t };
    reflect.Type.of<T>()
}
"#,
        );
        let tokens = semantic_tokens(&db, file);
        let rendered = render(&db, file, tokens);
        for expected in [
            "keyword        \"let\"",
            "keyword        \"type\"",
            "typeParameter  [declaration] \"T\"",
            "interface      \"Named\"",
        ] {
            assert!(
                rendered.contains(expected),
                "missing `{expected}` in:\n{rendered}"
            );
        }
    }

    /// The names the grammar admits in a keyword's spelling are names: a
    /// field, named argument or member called `client`; an LLM body's keys
    /// declare properties; a type path's dots are its namespace's; an
    /// associated type's binding declares the name it binds.
    #[test]
    fn keywords_in_name_positions_and_type_paths_are_classified() {
        let (db, file) = test_db(
            r#"interface Source { type Item }

class Bag { client string }

implements Source for Bag { type Item = int }

function ask(x: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `${x}`
}

function take(client: string, s: Source<Item = int>) -> reflect.Type {
    reflect.Type.of<int>()
}

function use(b: Bag) -> string { take(client = b.client, s = b); b.client }
"#,
        );
        let tokens = semantic_tokens(&db, file);
        let rendered = render(&db, file, tokens);
        for expected in [
            "property       [declaration] \"client\"",
            "property       [declaration] \"prompt\"",
            "type           [declaration] \"Item\"",
            "parameter      [declaration] \"client\"",
            "parameter      \"client\"",
            "property       \"client\"",
            "namespace      [defaultLibrary] \".\"",
        ] {
            assert!(
                rendered.contains(expected),
                "missing `{expected}` in:\n{rendered}"
            );
        }
        assert!(
            !rendered.contains("keyword        \"client\""),
            "a `client` in a name position is not a keyword:\n{rendered}"
        );
        let item_declarations = rendered
            .matches("type           [declaration] \"Item\"")
            .count();
        assert_eq!(
            item_declarations, 3,
            "the interface, the impl and the binding:\n{rendered}"
        );
    }

    fn test_db(source: &str) -> (ProjectDatabase, SourceFile) {
        let mut db = ProjectDatabase::new();
        db.workspace(Path::new("/test"));
        let file = db.file(Path::new("/test/main.baml"), source);
        (db, file)
    }

    fn render(db: &ProjectDatabase, file: SourceFile, tokens: &[SemanticToken]) -> String {
        let text = file.text(db);
        let mut rendered = String::new();
        for token in tokens {
            let slice = &text[usize::from(token.range.start())..usize::from(token.range.end())];
            let modifiers: Vec<&str> = token.modifiers.names().collect();
            let modifiers = if modifiers.is_empty() {
                String::new()
            } else {
                format!(" [{}]", modifiers.join(","))
            };
            writeln!(
                rendered,
                "{:<14}{} {:?}",
                token.token_type.as_str(),
                modifiers,
                slice
            )
            .unwrap_or_else(|_| unreachable!("writing to a String cannot fail"));
        }
        rendered
    }

    const FIXTURE: &str = r#"// Greeting demo
enum Status {
  Active
  Inactive
}

class Person {
  name string
  status Status
}

function greet(person: Person) -> string {
  let greeting = "hi \u{1F600} \x41";
  if person.status == Status.Active {
    greeting
  } else {
    "bye"
  }
}
"#;

    #[test]
    fn full_file_walk_classifies_declarations_and_uses() {
        let (db, file) = test_db(FIXTURE);
        let tokens = semantic_tokens(&db, file);
        insta::assert_snapshot!(render(&db, file, tokens));
    }

    #[test]
    fn range_walk_agrees_with_the_full_walk() {
        let (db, file) = test_db(FIXTURE);
        let full = semantic_tokens(&db, file);

        // Viewport over the function only.
        let start = u32::try_from(FIXTURE.find("function").expect("fixture has a function"))
            .expect("fixture is small");
        let end = u32::try_from(FIXTURE.len()).expect("fixture is small");
        let ranged = semantic_tokens_in_range(&db, file, start, end);

        let range = TextRange::new(start.into(), end.into());
        let expected: Vec<SemanticToken> = full
            .iter()
            .filter(|t| range.intersect(t.range).is_some())
            .cloned()
            .collect();
        assert_eq!(ranged, expected);
    }
}
