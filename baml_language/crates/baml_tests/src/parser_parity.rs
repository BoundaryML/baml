//! The two parsers compared, file by file, over every `.baml` source the repo
//! owns (the roots of [`crate::ast_dump`]).
//!
//! Per file, all in-process (no stored baseline):
//!
//! - **errors**: each parser's diagnostics as `(range, text)`, as a multiset;
//! - **shape**: each tree's nodes as `(category, range)`, where a category is
//!   a construct both trees name ([`Category`]) and ranges exclude trivia;
//! - **placeholders**: every range the typed parser has not been taught to
//!   read yet must coincide with one rowan node or token, so a placeholder
//!   that swallowed its neighbours is a difference even when nothing else is
//!   left to compare;
//! - **error presence**: the typed parser may lack errors the rowan parser
//!   reports *inside* a placeholder or a removed item's body (those are not
//!   read), and may not report errors where the rowan parser has none.
//!
//! Nothing inside a placeholder, a `RemovedItem`'s head/body or a `JinjaArg`
//! body is compared on either side; in a parse with errors, runs of tokens the
//! typed tree does not own are not compared either (recovery skipped them). A
//! reported `Bogus` is *not* opaque: it is a recovery the rowan side must
//! mirror. Known differences are listed in [`PENDING`], which may only shrink:
//! an entry that stops differing, or names no file, fails the test.

use std::{collections::BTreeMap, fmt::Write as _};

use baml_base::FileId;
use baml_db::{
    baml_compiler_lexer, baml_compiler_parser,
    baml_compiler_syntax::{SyntaxKind, SyntaxNode, traversal::trimmed_range},
};
use baml_syntax::{
    nodes::{AnyNodeRef, Arenas, NodeKind},
    token::{TokenIdx, TokenKind, Tokens},
    tree::{AnyNodeId, Missing, TokenRange, Visitor},
};
use text_size::TextRange;

/// Files whose two parses are known to differ, with the reason. Each entry is
/// removed when its layer lands or the difference is ruled acceptable.
pub const PENDING: &[(&str, &str)] = &[
    (
        "projects/broken_syntax/header_in_llm_function/test.baml",
        "the `//#` diagnostic names the rule (a statement) instead of the old exemption",
    ),
    (
        "projects/broken_syntax/type_attributes/signatures.baml",
        "a string in a recovery position is one token, not re-split around its quotes",
    ),
    (
        "projects/broken_syntax/mismatched_brackets/main.baml",
        "an unclosed `[`/`<` in a body no longer swallows the next item: bodies are brace-balanced",
    ),
    (
        "projects/broken_syntax/parser_strings/nested_quotes.baml",
        "single-quoted text is `Error` tokens, not re-split around `\\` and `\"`",
    ),
    (
        "projects/broken_syntax/parser_strings/unicode_strings.baml",
        "a non-ASCII name is `Error` tokens on both sides, but the signature now recovers in place",
    ),
    (
        "projects/broken_syntax/pending_greaters_fix/extra_greater.baml",
        "`>>` is two `>` tokens: the extra `>` is a stray token, not an unmatched-`>>` diagnostic",
    ),
    (
        "projects/broken_syntax/type_attributes/nested.baml",
        "`>>` is two `>` tokens: both generic lists close after the attribute error",
    ),
    (
        "projects/broken_syntax/is_in_type_position/main.baml",
        "stray text at the top level is one `Bogus` with one diagnostic up to the next item start; rowan reported every token",
    ),
    (
        "projects/broken_syntax/parser_error_recovery/invalid_syntax.baml",
        "stray text at the top level is one `Bogus` with one diagnostic up to the next item start; rowan reported every token",
    ),
    (
        "projects/broken_syntax/parser_error_recovery/partial_input.baml",
        "stray text at the top level is one `Bogus` with one diagnostic up to the next item start; rowan reported every token",
    ),
];

/// A construct both trees name. Kinds with no counterpart on the other side
/// (token wrappers, layers not yet ported) have none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Category {
    Function,
    Class,
    Enum,
    Interface,
    TypeAlias,
    ClientValue,
    /// A removed config block the rowan parser still has a node for; a
    /// `RemovedItem` with the same keyword here. Rowan's `ERROR` wrappers are
    /// recovery artifacts, not constructs, and are not compared.
    Removed,
    ImplementsFor,
    ImplementsBlock,
    FieldLink,
    MethodSig,
    AssocType,
    Requires,
    Field,
    Variant,
    Params,
    Param,
    GenericParams,
    GenericParam,
    GenericBounds,
    TypeArgs,
    Attribute,
    BlockAttribute,
    AttributeArgs,
    Jinja,
    Throws,
    /// Every type node; a modifier chain (`T[]?`) counts once, as the rowan
    /// `TYPE_EXPR` does.
    Type,
    FunctionType,
    FunctionTypeParam,
    LlmBody,
    ClientField,
    PromptField,
    ToolsField,
}

/// `parent` disambiguates rowan's `FUNCTION_TYPE_PARAM`: it also wraps the
/// operand of a parenthesized type (the parser decides after parsing), where
/// the typed tree has a `ParenType` and no parameter.
fn rowan_category(kind: SyntaxKind, parent: Option<SyntaxKind>) -> Option<Category> {
    use SyntaxKind as K;
    Some(match kind {
        K::FUNCTION_DEF => Category::Function,
        K::CLASS_DEF => Category::Class,
        K::ENUM_DEF => Category::Enum,
        K::INTERFACE_DEF => Category::Interface,
        K::TYPE_ALIAS_DEF => Category::TypeAlias,
        K::CLIENT_VALUE_DEF => Category::ClientValue,
        K::CLIENT_DEF | K::GENERATOR_DEF | K::RETRY_POLICY_DEF | K::TEMPLATE_STRING_DEF => {
            Category::Removed
        }
        K::IMPLEMENTS_FOR => Category::ImplementsFor,
        K::IMPLEMENTS_BLOCK => Category::ImplementsBlock,
        K::INTERFACE_FIELD_LINK => Category::FieldLink,
        K::METHOD_SIG => Category::MethodSig,
        K::ASSOCIATED_TYPE_DECL => Category::AssocType,
        K::REQUIRES_CLAUSE => Category::Requires,
        K::FIELD => Category::Field,
        K::ENUM_VARIANT => Category::Variant,
        K::PARAMETER_LIST => Category::Params,
        K::PARAMETER => Category::Param,
        K::GENERIC_PARAM_LIST => Category::GenericParams,
        K::GENERIC_PARAM => Category::GenericParam,
        K::GENERIC_PARAM_BOUNDS => Category::GenericBounds,
        K::TYPE_ARGS => Category::TypeArgs,
        K::ATTRIBUTE => Category::Attribute,
        K::BLOCK_ATTRIBUTE => Category::BlockAttribute,
        K::ATTRIBUTE_ARGS => Category::AttributeArgs,
        K::EXPR => Category::Jinja,
        K::THROWS_CLAUSE => Category::Throws,
        K::TYPE_EXPR => Category::Type,
        K::FUNCTION_TYPE => Category::FunctionType,
        K::FUNCTION_TYPE_PARAM if parent == Some(K::FUNCTION_TYPE) => Category::FunctionTypeParam,
        K::FUNCTION_TYPE_PARAM => return None,
        K::LLM_FUNCTION_BODY => Category::LlmBody,
        K::CLIENT_FIELD => Category::ClientField,
        K::PROMPT_FIELD => Category::PromptField,
        K::TOOLS_FIELD => Category::ToolsField,
        _ => return None,
    })
}

/// The categories a typed node contributes: a function type is both a
/// `TYPE_EXPR` and a `FUNCTION_TYPE` on the rowan side.
fn typed_categories(kind: NodeKind) -> &'static [Category] {
    use NodeKind as K;
    match kind {
        K::FunctionDef => &[Category::Function],
        K::ClassDef => &[Category::Class],
        K::EnumDef => &[Category::Enum],
        K::InterfaceDef => &[Category::Interface],
        K::TypeAliasDef => &[Category::TypeAlias],
        K::ClientValueDef => &[Category::ClientValue],
        K::RemovedItem => &[Category::Removed],
        K::ImplementsFor => &[Category::ImplementsFor],
        K::ImplementsBlock => &[Category::ImplementsBlock],
        K::InterfaceFieldLink => &[Category::FieldLink],
        K::MethodSig => &[Category::MethodSig],
        K::AssocTypeDecl | K::AssocTypeDef | K::AssocTypeBinding => &[Category::AssocType],
        K::RequiresClause => &[Category::Requires],
        K::Field => &[Category::Field],
        K::EnumVariant => &[Category::Variant],
        K::ParamList => &[Category::Params],
        K::Param => &[Category::Param],
        K::GenericParamList => &[Category::GenericParams],
        K::GenericParam => &[Category::GenericParam],
        K::GenericParamBounds => &[Category::GenericBounds],
        K::TypeArgs => &[Category::TypeArgs],
        K::Attribute => &[Category::Attribute],
        K::BlockAttr => &[Category::BlockAttribute],
        K::AttrArgs => &[Category::AttributeArgs],
        K::JinjaArg => &[Category::Jinja],
        K::ThrowsClause => &[Category::Throws],
        K::PathType
        | K::LiteralType
        | K::ParenType
        | K::OptionalType
        | K::ArrayType
        | K::UnionType
        | K::QualifiedPath => &[Category::Type],
        K::FunctionType => &[Category::Type, Category::FunctionType],
        K::FunctionTypeParam => &[Category::FunctionTypeParam],
        K::LlmBody => &[Category::LlmBody],
        K::ClientField => &[Category::ClientField],
        K::PromptField => &[Category::PromptField],
        K::ToolsField => &[Category::ToolsField],
        _ => &[],
    }
}

/// Kinds whose member separator (`,`/`;`) the typed node owns and the rowan
/// node leaves outside.
fn owns_separator(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Field
            | NodeKind::EnumVariant
            | NodeKind::AssocTypeDecl
            | NodeKind::AssocTypeDef
            | NodeKind::InterfaceFieldLink
            | NodeKind::MethodSig
    )
}

/// One comparable fact: a byte range and what is there.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Entry {
    range: (u32, u32),
    text: String,
}

impl Entry {
    fn new(range: TextRange, text: impl Into<String>) -> Self {
        Self {
            range: (range.start().into(), range.end().into()),
            text: text.into(),
        }
    }

    /// Within one of `opaque`. An empty entry at a range's end is not inside
    /// it: an EOF caret after a placeholder is about what follows.
    fn inside(&self, opaque: &[TextRange]) -> bool {
        let (start, end) = self.range;
        opaque.iter().any(|r| {
            let (rs, re) = (u32::from(r.start()), u32::from(r.end()));
            !r.is_empty() && rs <= start && end <= re && !(start == end && start == re)
        })
    }
}

type Multiset = BTreeMap<Entry, usize>;

fn multiset(entries: impl IntoIterator<Item = Entry>) -> Multiset {
    let mut set = Multiset::new();
    for entry in entries {
        *set.entry(entry).or_default() += 1;
    }
    set
}

/// Collects the typed tree's nodes with their spans, and the ranges nothing
/// is compared inside.
struct TypedWalk<'a> {
    arenas: &'a Arenas,
    tokens: &'a Tokens,
    nodes: Vec<(AnyNodeId, NodeKind, TokenRange)>,
    /// `inner` operands of a modifier (part of the outer chain's `TYPE_EXPR`)
    /// and removed items the rowan tree has no node for.
    suppressed: Vec<AnyNodeId>,
    in_modifier: bool,
    /// Ranges of the typed parser's `RemovedFeature` diagnostics (the removed
    /// form's keyword): a `Bogus` starting at one is removed syntax, read on
    /// neither side.
    removed: Vec<TextRange>,
    /// Removed-item heads and bodies, removed-syntax `Bogus`, jinja bodies.
    opaque: Vec<TextRange>,
    pending: Vec<AnyNodeId>,
    owned: Vec<TokenIdx>,
}

impl Visitor for TypedWalk<'_> {
    fn node(&mut self, field: &'static str, id: AnyNodeId) {
        if self.in_modifier && field == "inner" {
            self.suppressed.push(id);
        }
        self.pending.push(id);
    }
    fn token(&mut self, _field: &'static str, token: TokenIdx) {
        self.owned.push(token);
    }
    fn range(&mut self, _field: &'static str, range: TokenRange) {
        self.owned.extend(range.iter());
    }
    fn missing(&mut self, _field: &'static str, _missing: Missing) {}
}

impl TypedWalk<'_> {
    fn text_range(&self, range: TokenRange) -> TextRange {
        if range.is_empty() {
            return TextRange::empty(0.into());
        }
        TextRange::new(
            self.tokens.range(range.start()).start(),
            self.tokens.range(TokenIdx(range.end().0 - 1)).end(),
        )
    }

    fn run(&mut self, root: AnyNodeId) {
        self.pending.push(root);
        while let Some(id) = self.pending.pop() {
            let kind = Arenas::kind(id);
            let span = self.arenas.span(id);
            match self.arenas.node(id) {
                AnyNodeRef::RemovedItem(item) => {
                    self.opaque.push(self.text_range(item.head));
                    self.opaque.push(self.text_range(item.body));
                    if !matches!(
                        self.tokens.kind(item.kw.idx),
                        TokenKind::Client
                            | TokenKind::Generator
                            | TokenKind::RetryPolicy
                            | TokenKind::TemplateString
                    ) {
                        self.suppressed.push(id);
                    }
                }
                AnyNodeRef::JinjaArg(arg) => self.opaque.push(self.text_range(arg.tokens)),
                AnyNodeRef::Bogus(_) => {
                    let range = self.text_range(span);
                    if self.removed.iter().any(|r| r.start() == range.start()) {
                        self.opaque.push(range);
                    }
                }
                _ => {}
            }
            self.nodes.push((id, kind, span));
            self.in_modifier = matches!(kind, NodeKind::OptionalType | NodeKind::ArrayType);
            self.arenas.walk_node(id, self);
        }
        self.owned.sort_unstable();
    }

    /// Runs of significant tokens no node owns: recovery skipped them.
    fn gaps(&self) -> Vec<TextRange> {
        let mut gaps = Vec::new();
        let mut run: Option<TextRange> = None;
        for idx in self.tokens.iter() {
            if self.tokens.kind(idx).is_trivia() {
                continue;
            }
            let range = self.tokens.range(idx);
            if self.owned.binary_search(&idx).is_ok() {
                gaps.extend(run.take());
            } else {
                run = Some(run.map_or(range, |r| r.cover(range)));
            }
        }
        gaps.extend(run);
        gaps
    }

    /// A node's span without an owned member separator and the trivia before
    /// it, so it matches the rowan node's trimmed range.
    fn comparable_span(&self, kind: NodeKind, span: TokenRange) -> TokenRange {
        if !owns_separator(kind) || span.is_empty() {
            return span;
        }
        let mut end = span.end().0;
        if matches!(
            self.tokens.kind(TokenIdx(end - 1)),
            TokenKind::Comma | TokenKind::Semicolon
        ) {
            end -= 1;
            while end > span.start().0 && self.tokens.kind(TokenIdx(end - 1)).is_trivia() {
                end -= 1;
            }
        }
        TokenRange::new(span.start(), TokenIdx(end))
    }

    fn entries(&self) -> Vec<Entry> {
        self.nodes
            .iter()
            .filter(|(id, _, _)| !self.suppressed.contains(id))
            .flat_map(|&(_, kind, span)| {
                let range = self.text_range(self.comparable_span(kind, span));
                typed_categories(kind)
                    .iter()
                    .map(move |category| Entry::new(range, format!("{category:?}")))
            })
            .collect()
    }
}

fn rowan_error_entry(error: &baml_compiler_diagnostics::ParseError) -> Entry {
    use baml_compiler_diagnostics::ParseError as E;
    match error {
        E::UnexpectedToken {
            expected,
            found,
            span,
        } => Entry::new(span.range, format!("expected {expected}, found {found}")),
        E::UnexpectedEof { expected, span } => {
            Entry::new(span.range, format!("expected {expected}, found EOF"))
        }
        E::InvalidSyntax { message, span } => Entry::new(span.range, message.clone()),
        E::AmbiguousUnion { span, groupings } => {
            Entry::new(span.range, format!("ambiguous union {groupings:?}"))
        }
        E::RemovedFeature { message, span } => {
            Entry::new(span.range, format!("removed: {message}"))
        }
    }
}

/// `None` for an error the rowan pipeline reports from its lowering, not its
/// parser: those are compared by `ast_dump::moved_diagnostics_match_rowan`.
fn typed_error_entry(error: &baml_parser::ParseError) -> Option<Entry> {
    use baml_parser::ParseError as E;
    Some(match error {
        E::UnexpectedToken {
            expected,
            found,
            range,
        } => Entry::new(*range, format!("expected {expected}, found {found}")),
        E::UnexpectedEof { expected, range } => {
            Entry::new(*range, format!("expected {expected}, found EOF"))
        }
        E::InvalidSyntax { message, range } => Entry::new(*range, message.clone()),
        E::AmbiguousUnion { range, groupings } => {
            Entry::new(*range, format!("ambiguous union {groupings:?}"))
        }
        E::RemovedFeature { message, range } => Entry::new(*range, format!("removed: {message}")),
        E::InvalidStringEscape { .. } | E::InvalidDirective { .. } => return None,
    })
}

/// Both parses of `text`, reduced to comparable facts. `Ok` when they agree;
/// `Err` reports every difference.
#[expect(
    deprecated,
    reason = "placeholders bound what is compared until the last grammar layer lands"
)]
pub fn compare(text: &str) -> Result<(), String> {
    let typed = baml_parser::parse(text);
    let mut walk = TypedWalk {
        arenas: typed.tree.arenas(),
        tokens: typed.tree.tokens(),
        nodes: Vec::new(),
        suppressed: Vec::new(),
        in_modifier: false,
        removed: typed
            .errors
            .iter()
            .filter_map(|error| match error {
                baml_parser::ParseError::RemovedFeature { range, .. } => Some(*range),
                _ => None,
            })
            .collect(),
        opaque: Vec::new(),
        pending: Vec::new(),
        owned: Vec::new(),
    };
    walk.run(typed.tree.root_id());
    let placeholders: Vec<TextRange> = typed
        .placeholders
        .iter()
        .map(|&range| walk.text_range(range))
        .collect();
    // What the typed parser does not read: the only places it may lack a
    // diagnostic the rowan parser has.
    let mut unread = walk.opaque.clone();
    unread.extend(placeholders.iter().copied());
    let mut opaque = unread.clone();
    if !typed.errors.is_empty() {
        opaque.extend(walk.gaps());
    }
    let keep = |entry: &Entry| !entry.inside(&opaque);

    let typed_shape = multiset(walk.entries().into_iter().filter(keep));
    let typed_errors = multiset(
        typed
            .errors
            .iter()
            .filter_map(typed_error_entry)
            .filter(keep),
    );

    let tokens = baml_compiler_lexer::lex_lossless(text, FileId::new(0));
    let (green, errors) = baml_compiler_parser::parse_file(&tokens);
    let root = SyntaxNode::new_root(green);
    let rowan_nodes: Vec<(SyntaxKind, Option<SyntaxKind>, TextRange)> = root
        .descendants()
        .map(|node| {
            let parent = node.parent().map(|parent| parent.kind());
            (node.kind(), parent, trimmed_range(&node))
        })
        .collect();
    let rowan_shape = multiset(
        rowan_nodes
            .iter()
            .filter_map(|&(kind, parent, range)| {
                let category = rowan_category(kind, parent)?;
                (!range.is_empty()).then(|| Entry::new(range, format!("{category:?}")))
            })
            .filter(keep),
    );
    let rowan_error_entries: Vec<Entry> = errors.iter().map(rowan_error_entry).collect();
    let rowan_errors = multiset(
        rowan_error_entries
            .iter()
            .filter(|&entry| keep(entry))
            .cloned(),
    );

    let mut report = String::new();
    diff(&mut report, "shape", &rowan_shape, &typed_shape);
    diff(&mut report, "errors", &rowan_errors, &typed_errors);

    let mut rowan_extents: Vec<(u32, u32)> = rowan_nodes
        .iter()
        .map(|&(_, _, range)| (range.start().into(), range.end().into()))
        .chain(root.descendants_with_tokens().filter_map(|element| {
            let token = element.into_token()?;
            (!token.kind().is_trivia()).then(|| {
                let range = token.text_range();
                (range.start().into(), range.end().into())
            })
        }))
        .collect();
    rowan_extents.sort_unstable();
    for range in &placeholders {
        let extent = (u32::from(range.start()), u32::from(range.end()));
        if !range.is_empty() && rowan_extents.binary_search(&extent).is_err() {
            let _ = writeln!(
                report,
                "  placeholder {extent:?} is not one rowan node or token"
            );
        }
    }

    if typed.errors.is_empty() {
        for entry in rowan_error_entries
            .iter()
            .filter(|entry| !entry.inside(&unread))
        {
            let _ = writeln!(
                report,
                "  errors: rowan reports where the typed tree is read and error-free  {:?} {}",
                entry.range, entry.text
            );
        }
    }
    if errors.is_empty() {
        for entry in typed.errors.iter().filter_map(typed_error_entry) {
            let _ = writeln!(
                report,
                "  errors: typed reports, rowan has none  {:?} {}",
                entry.range, entry.text
            );
        }
    }

    if report.is_empty() {
        Ok(())
    } else {
        Err(report)
    }
}

fn diff(report: &mut String, what: &str, rowan: &Multiset, typed: &Multiset) {
    for (entry, &n) in rowan {
        let m = typed.get(entry).copied().unwrap_or(0);
        if n > m {
            let _ = writeln!(
                report,
                "  {what}: only rowan ×{}  {:?} {}",
                n - m,
                entry.range,
                entry.text
            );
        }
    }
    for (entry, &m) in typed {
        let n = rowan.get(entry).copied().unwrap_or(0);
        if m > n {
            let _ = writeln!(
                report,
                "  {what}: only typed ×{}  {:?} {}",
                m - n,
                entry.range,
                entry.text
            );
        }
    }
}

#[test]
fn parser_parity() {
    let mut failures = String::new();
    let mut unexpected_passes = Vec::new();
    let mut seen_pending = Vec::new();
    let mut compared = 0usize;
    for (label, path) in crate::ast_dump::source_files() {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        compared += 1;
        let pending = PENDING.iter().any(|(file, _)| *file == label);
        if pending {
            seen_pending.push(label.clone());
        }
        match compare(&text) {
            Ok(()) if pending => unexpected_passes.push(label),
            Ok(()) => {}
            Err(_) if pending => {}
            Err(report) => {
                let _ = writeln!(failures, "{label}:\n{report}");
            }
        }
    }
    assert!(
        compared > 500,
        "expected the whole corpus, compared {compared} files"
    );
    let unmatched: Vec<&str> = PENDING
        .iter()
        .map(|(file, _)| *file)
        .filter(|file| !seen_pending.iter().any(|seen| seen == file))
        .collect();
    assert!(
        unmatched.is_empty(),
        "PENDING names files that do not exist: {unmatched:#?}"
    );
    assert!(
        unexpected_passes.is_empty(),
        "these files now parse identically; remove them from PENDING: {unexpected_passes:#?}"
    );
    assert!(
        failures.is_empty(),
        "parser parity differences:\n{failures}"
    );
}
