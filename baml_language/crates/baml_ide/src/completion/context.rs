//! Classifying the cursor: what kind of position is this, and what does the
//! qualifier before it mean?
//!
//! The classification runs on a speculative parse — the file's text with a
//! marker identifier spliced in at the cursor — for the reason rust-analyzer
//! does the same: at a completion position the real text is usually not
//! valid, and a tree parsed from it labels the position by how the parser
//! recovered rather than by what the user is writing. `function f() -> ⎸ {`
//! parses with an EMPTY type node the cursor cannot be found in; with a
//! marker spliced in it is an ordinary type reference.
//!
//! Only offsets STRICTLY BEFORE the cursor are shared between the two texts.
//! The classification hands the RESOLUTION step such offsets, and the
//! resolution reads the real file's recorded facts — which is what makes it
//! safe to classify on one tree and resolve on the other.
//!
//! Resolution happens HERE, not in a provider: deciding what the `.` before
//! the cursor reads from is analysis (rust-analyzer's
//! `Qualified::With { resolution }`), and a provider that had to try the
//! readings itself would be re-deriving the position it was handed.

use baml_base::SourceFile;
use baml_compiler_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};
use baml_compiler2_hir::{body::BodyOwnerId, contributions::Definition};
use baml_compiler2_ppir::resolve::{NamespaceMember, namespace_members_at};
use text_size::{TextRange, TextSize};

use crate::{resolve, syntax};

/// The identifier spliced in at the cursor. A plausible identifier (so the
/// parser takes the branch the user is heading for) that no real source would
/// contain (so a classification can find it unambiguously).
const MARKER: &str = "bamlCompletionMarker";

/// Where the cursor is, in the terms providers need.
pub(crate) struct CompletionContext<'db> {
    /// The range an item replaces: the identifier fragment already typed,
    /// empty when the cursor sits right after a `.`.
    pub(crate) source_range: TextRange,
    pub(crate) analysis: CompletionAnalysis<'db>,
}

/// The kind of position, with everything about it already resolved.
pub(crate) enum CompletionAnalysis<'db> {
    /// A name is being written: `<here>`, or `<qualifier>.<here>`.
    Path {
        kind: PathKind,
        /// The resolved dot before the cursor, when there is one. A `None`
        /// qualifier is a bare name.
        qualifier: Option<DotTarget<'db>>,
    },
    /// An argument slot: `f(<here>)`, `f(1, <here>)`. Carries the callee's
    /// resolved position; a slot whose call the checker recorded nothing
    /// for classifies as a bare expression instead.
    CallArgument { call: resolve::CallPosition },
    /// A field slot in an object literal: `Foo { <here> }`, resolved to the
    /// class inference recorded for the literal.
    RecordField {
        literal: resolve::ObjectLiteralPosition<'db>,
    },
    /// A position no provider knows yet. Additive by construction: an
    /// unclassified position offers nothing rather than guessing.
    Unsupported,
}

/// What a written name at the position IS. The kind selects which view of a
/// [`DotTarget`] a provider offers — the same qualifier means different
/// members in expression and type position — so C3's type positions are a
/// new variant here, not a second qualifier story.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PathKind {
    /// An expression: locals, items, packages, and the keywords that can
    /// start one.
    Expr,
    /// A type is being written: an annotation, a parameter/return type, a
    /// type argument, a bound, a `throws` clause — or a match/catch arm,
    /// because a pattern IS a type (`TYPE_PATTERN` wraps a `TYPE_EXPR`;
    /// BEP-015's membership model makes that literal).
    Type,
    // Item positions arrive with C4.
}

/// What the `.` before the cursor reads a member OF, resolved once.
///
/// BAML's `.` is overloaded three ways where Rust spells two tokens (`.`
/// and `::`), so the one thing every dotted completion needs decided is
/// which reading the prefix takes. The readings are tried in resolution's
/// own order: a typed VALUE claims first, then a TYPE declaration, then a
/// NAMESPACE — and a prefix none of them claim offers nothing.
pub(crate) enum DotTarget<'db> {
    /// A value receiver: `p.` — its inferred type, in the body owner's
    /// param env (a `T extends Compare` receiver has members only because
    /// the owner declared that bound).
    Value {
        owner: BodyOwnerId<'db>,
        receiver: baml_type::interned::Ty,
    },
    /// A type qualifier: `Point.`, `int.`, `baml.iter.Range.` — the
    /// declaration it names.
    Type(Definition<'db>),
    /// A namespace qualifier: `baml.`, `baml.http.`, `root.ns.`.
    ///
    /// Carries the members rather than the written chain because resolving
    /// a namespace and enumerating it are one query — `namespace_members_at`
    /// answering `None` is precisely what distinguishes a qualifier from a
    /// value — so the target holds what resolution already had in hand.
    Namespace(Vec<NamespaceMember<'db>>),
}

impl<'db> CompletionContext<'db> {
    pub(crate) fn new(
        db: &'db dyn baml_compiler2_ppir::Db,
        file: SourceFile,
        offset: TextSize,
    ) -> Option<Self> {
        let text = file.text(db);
        if usize::from(offset) > text.len() {
            return None;
        }
        let (tree, marker) = speculative_parse(db, file, text, offset)?;
        let token = tree
            .token_at_offset(marker.start() + TextSize::from(1))
            .right_biased()?;

        // The fragment already typed is the marker token minus the marker
        // itself — real coordinates, because it ends at the cursor.
        let source_range = if token.kind() == SyntaxKind::WORD {
            TextRange::new(token.text_range().start().min(offset), offset)
        } else {
            TextRange::empty(offset)
        };

        // Classify on the speculative tree, then resolve what the
        // classification located against the real file's recorded facts.
        let analysis = match classify(&token) {
            Position::Dotted { dot, kind } => match dot_target(db, file, dot) {
                Some(qualifier) => CompletionAnalysis::Path {
                    kind,
                    qualifier: Some(qualifier),
                },
                None => CompletionAnalysis::Unsupported,
            },
            Position::Type => CompletionAnalysis::Path {
                kind: PathKind::Type,
                qualifier: None,
            },
            // A slot whose call inference recorded nothing for is still an
            // expression position; only the labels are gone.
            Position::ArgumentSlot { open_paren } => match resolve::call_at(db, file, open_paren) {
                Some(call) => CompletionAnalysis::CallArgument { call },
                None => CompletionAnalysis::Path {
                    kind: PathKind::Expr,
                    qualifier: None,
                },
            },
            Position::FieldSlot { literal } => {
                match resolve::object_literal_at(db, file, literal) {
                    Some(literal) => CompletionAnalysis::RecordField { literal },
                    None => CompletionAnalysis::Unsupported,
                }
            }
            Position::Expression => CompletionAnalysis::Path {
                kind: PathKind::Expr,
                qualifier: None,
            },
            Position::Unsupported => CompletionAnalysis::Unsupported,
        };

        Some(Self {
            source_range,
            analysis,
        })
    }
}

/// The file's text with [`MARKER`] spliced in at `offset`, parsed. Returns the
/// tree and the marker's range within it.
fn speculative_parse(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    text: &str,
    offset: TextSize,
) -> Option<(SyntaxNode, TextRange)> {
    let at = usize::from(offset);
    if !text.is_char_boundary(at) {
        return None;
    }
    let mut faked = String::with_capacity(text.len() + MARKER.len());
    faked.push_str(&text[..at]);
    faked.push_str(MARKER);
    faked.push_str(&text[at..]);

    let tokens = baml_compiler_lexer::lex_lossless(&faked, file.file_id(db));
    let (green, _errors) = baml_compiler_parser::parse_file(&tokens);
    let marker_len = TextSize::of(MARKER);
    Some((
        SyntaxNode::new_root(green),
        TextRange::at(offset, marker_len),
    ))
}

/// The purely syntactic classification, before any resolution. Every offset
/// it carries lies strictly before the cursor, so it is valid in the real
/// file too.
enum Position {
    /// `<something>.<here>` — `dot` is where the `.` begins, which is also
    /// where the prefix's recorded span ends.
    Dotted {
        dot: TextSize,
        kind: PathKind,
    },
    /// A bare name in type position.
    Type,
    /// `f(<here>)` — `open_paren` is where the argument list begins, which
    /// is where the callee's span ends.
    ArgumentSlot {
        open_paren: TextSize,
    },
    /// `Foo { <here> }` — `literal` is an offset inside the literal's
    /// recorded span.
    FieldSlot {
        literal: TextSize,
    },
    Expression,
    Unsupported,
}

/// What the marker token's surroundings say the position is.
///
/// Innermost-first: an object literal inside an argument list is a field
/// slot, not an argument slot, because that is the node the cursor is in.
fn classify(token: &SyntaxToken) -> Position {
    if in_prose(token) {
        return Position::Unsupported;
    }
    // A marker inside a TYPE_EXPR is a type being written, wherever the
    // expression sits — annotation, signature, bound, `throws`, or a
    // match/catch arm (patterns parse as TYPE_PATTERN around a TYPE_EXPR).
    // The kind rides along on the dotted form: `baml.⎸` in a type slot
    // resolves through the same DotTarget and filters to types.
    let in_type = token
        .parent_ancestors()
        .any(|node| node.kind() == SyntaxKind::TYPE_EXPR);
    if let Some(dot) = preceding_dot(token) {
        return Position::Dotted {
            dot,
            kind: if in_type {
                PathKind::Type
            } else {
                PathKind::Expr
            },
        };
    }
    if in_type {
        return Position::Type;
    }
    for node in token.parent_ancestors() {
        match node.kind() {
            SyntaxKind::OBJECT_LITERAL => {
                return Position::FieldSlot {
                    literal: node.text_range().start(),
                };
            }
            // The VALUE of an already-labelled argument (`tag = <here>`) is
            // an expression; the label is spoken for.
            SyntaxKind::CALL_ARG if labelled_before(&node, token) => {
                return Position::Expression;
            }
            SyntaxKind::CALL_ARGS => {
                return Position::ArgumentSlot {
                    open_paren: node.text_range().start(),
                };
            }
            SyntaxKind::BLOCK_EXPR | SyntaxKind::EXPR_FUNCTION_BODY => {
                return Position::Expression;
            }
            _ => {}
        }
    }
    Position::Unsupported
}

/// The three readings of the `.` at `dot`, tried in resolution's own order.
fn dot_target(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    dot: TextSize,
) -> Option<DotTarget<'_>> {
    if let Some((owner, receiver)) = resolve::receiver_at_dot(db, file, dot) {
        return Some(DotTarget::Value { owner, receiver });
    }
    let chain = qualifier_chain(db, file, dot)?;
    if let Some(definition) = resolve::type_qualifier_at(db, file, &chain) {
        return Some(DotTarget::Type(definition));
    }
    namespace_members_at(db, file, &chain).map(DotTarget::Namespace)
}

/// The dotted prefix the reader has already written, left of `dot` — read
/// from the REAL file (the chain ends before the cursor), never from the
/// fragment being typed.
fn qualifier_chain(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    dot: TextSize,
) -> Option<Vec<baml_base::Name>> {
    let token = syntax::find_token_at_offset(db, file, dot)?;
    let last = token
        .prev_token()
        .filter(|prev| prev.kind() == SyntaxKind::WORD)?;
    Some(syntax::dotted_chain_to(&last))
}

/// Whether this argument already carries its label, i.e. there is an `=`
/// between the argument's start and the cursor.
fn labelled_before(arg: &SyntaxNode, token: &SyntaxToken) -> bool {
    arg.children_with_tokens()
        .filter_map(baml_compiler_syntax::NodeOrToken::into_token)
        .any(|child| {
            child.kind() == SyntaxKind::EQUALS
                && child.text_range().end() <= token.text_range().start()
        })
}

/// Whether the cursor sits in text that is not code: a comment, or the
/// literal part of a string.
///
/// A backtick string is both — its `${…}` interpolations ARE code — so the
/// walk stops at whichever comes first: an interpolation (code, keep going)
/// or the string itself (prose, stop).
fn in_prose(token: &SyntaxToken) -> bool {
    if token.kind().is_comment() {
        return true;
    }
    for node in token.parent_ancestors() {
        match node.kind() {
            SyntaxKind::BACKTICK_INTERPOLATION
            | SyntaxKind::BACKTICK_FOR_OPEN
            | SyntaxKind::BACKTICK_IF_OPEN
            | SyntaxKind::BACKTICK_ELSE_IF => return false,
            SyntaxKind::STRING_LITERAL
            | SyntaxKind::RAW_STRING_LITERAL
            | SyntaxKind::BYTE_STRING_LITERAL
            | SyntaxKind::BACKTICK_STRING_LITERAL
            | SyntaxKind::BACKTICK_TEXT => return true,
            _ => {}
        }
    }
    false
}

/// The `.` immediately before `token` — plain or optional-chaining, since
/// `a?.b` reads a member of `a` exactly as `a.b` does. Trivia between the
/// two is skipped: a dot on its own line is a member access being written
/// across lines, and the receiver is still the value before it.
fn preceding_dot(token: &SyntaxToken) -> Option<TextSize> {
    let mut previous = token.prev_token()?;
    while matches!(
        previous.kind(),
        SyntaxKind::WHITESPACE
            | SyntaxKind::NEWLINE
            | SyntaxKind::LINE_COMMENT
            | SyntaxKind::BLOCK_COMMENT
            | SyntaxKind::HEADER_COMMENT
    ) {
        previous = previous.prev_token()?;
    }
    matches!(previous.kind(), SyntaxKind::DOT | SyntaxKind::QUESTION_DOT)
        .then(|| previous.text_range().start())
}
