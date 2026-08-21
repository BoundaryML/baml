//! Classifying the cursor: what kind of position is this?
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
//! Everything this module hands downstream is such an offset, which is what
//! makes it safe to classify on one tree and read facts from the other.

use baml_base::SourceFile;
use baml_compiler_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};
use text_size::{TextRange, TextSize};

/// The identifier spliced in at the cursor. A plausible identifier (so the
/// parser takes the branch the user is heading for) that no real source would
/// contain (so a classification can find it unambiguously).
const MARKER: &str = "bamlCompletionMarker";

/// Where the cursor is, in the terms providers need.
pub(crate) struct CompletionContext {
    /// The range an item replaces: the identifier fragment already typed,
    /// empty when the cursor sits right after a `.`.
    pub(crate) source_range: TextRange,
    pub(crate) analysis: CompletionAnalysis,
}

/// The kind of position, and the real-file facts that locate it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionAnalysis {
    /// `<receiver>.<here>` — a member of the value to the left.
    ///
    /// `dot` is where the `.` begins, which is also where the receiver's
    /// span ends: the compiler's recorded facts are addressed by that
    /// offset, so no name is re-resolved to find the receiver.
    Member { dot: TextSize },
    /// A position no provider knows yet. Additive by construction: an
    /// unclassified position offers nothing rather than guessing.
    Unsupported,
}

impl CompletionContext {
    pub(crate) fn new(
        db: &dyn baml_compiler2_ppir::Db,
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

        Some(Self {
            source_range,
            analysis: analyze(&token),
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

/// What the marker token's surroundings say the position is.
fn analyze(token: &SyntaxToken) -> CompletionAnalysis {
    if let Some(dot) = preceding_dot(token) {
        return CompletionAnalysis::Member { dot };
    }
    CompletionAnalysis::Unsupported
}

/// The `.` immediately before `token`, skipping nothing: a dot with
/// whitespace after it is a member access being written on the next line, and
/// the receiver is still the value before the dot.
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
    (previous.kind() == SyntaxKind::DOT).then(|| previous.text_range().start())
}
