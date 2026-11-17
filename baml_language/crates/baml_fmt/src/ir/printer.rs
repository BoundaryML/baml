//! The printer: lays a document out within the line width.
//!
//! The printer recurses over the document. Its state is what it has produced
//! so far: the [`Output`] items and the layout each named group was given.
//! Everything a decision needs beyond that — the layout and indent a node is
//! printed in, and what follows it on the line — is passed down the recursion
//! by value or by reference, so a fits-check is a pure function of the printer
//! and never has to be undone.

use std::iter;

use rustc_hash::FxHashMap;

use super::doc::{
    Doc, GroupId, LineMode, Measure, Node, SuffixWidth, VerbatimPiece, display_width,
};
use crate::FormatOptions;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Mode {
    Flat,
    Break,
}

/// The options in the printer's own units.
#[derive(Debug, Clone, Copy)]
struct Layout {
    line_width: u32,
    indent_width: u32,
}

impl From<FormatOptions> for Layout {
    fn from(options: FormatOptions) -> Self {
        // A width beyond `u32::MAX` columns means "unlimited", and so does
        // `u32::MAX` itself: no line is that wide. Indents saturate at it.
        let clamp = |columns: usize| u32::try_from(columns).unwrap_or(u32::MAX);
        Self {
            line_width: clamp(options.line_width),
            indent_width: clamp(options.indent_width),
        }
    }
}

/// A node together with the layout and indent it is printed in.
#[derive(Debug, Clone, Copy)]
struct Piece<'a> {
    doc: Doc<'a>,
    mode: Mode,
    indent: u32,
}

/// What follows a node up to the end of the document: the unprinted siblings
/// of each enclosing concat, innermost first. Borrowed down the recursion, so
/// a fits-check can measure past the node it is deciding without the printer
/// keeping a queue of its own.
#[derive(Debug, Clone, Copy)]
struct Rest<'r, 'a> {
    siblings: &'r [Doc<'a>],
    mode: Mode,
    indent: u32,
    outer: Option<&'r Rest<'r, 'a>>,
}

impl<'a> Rest<'_, 'a> {
    /// Every piece that follows, in print order.
    fn pieces(rest: Option<&Self>) -> impl Iterator<Item = Piece<'a>> {
        iter::successors(rest, |frame| frame.outer).flat_map(|frame| {
            frame.siblings.iter().map(|&doc| Piece {
                doc,
                mode: frame.mode,
                indent: frame.indent,
            })
        })
    }
}

/// What the printer emits, in order. Text is borrowed from the document;
/// nothing is copied until the result is written, once, at the end.
/// Where a tail line is indented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TailIndent {
    /// At the indent it was emitted at: a comment printed where it was
    /// written keeps its level.
    Fixed(u32),
    /// At the level of the list item it was carried out of ([`Node::Scope`]),
    /// or at that of the line break ending the line when that is deeper: a
    /// comment on a line of its own after an item reads as the next item's,
    /// or as the item's own when none follows.
    AtLeast(u32),
}

#[derive(Debug, Clone, Copy)]
enum Item<'a> {
    /// Text on the current line, and the column it ends at.
    Text { text: &'a str, end: u32 },
    /// Text for the end of the current line, rendered after everything else
    /// on it however early it was emitted.
    Tail(&'a str),
    /// Text for a line of its own after the current line, rendered after the
    /// line's tails.
    TailLine {
        text: &'a str,
        blank_before: bool,
        indent: TailIndent,
    },
    /// The end of a line, and the indent of the next; with `blank`, one blank
    /// line comes first. A newline `in_text` is one the text itself had: the
    /// next line keeps its own columns, pending tails stay pending across
    /// it, and it is written even before the first content.
    Newline {
        indent: u32,
        blank: bool,
        in_text: bool,
    },
}

/// The output so far: a flat sequence of [`Item`]s. The line being written
/// is whatever follows the last newline, so the current column and blankness
/// are read off the end of the sequence rather than tracked; only where the
/// line starts is remembered, so that reading it is not a scan.
/// Indentation is rendered only for a line that has content, so a blank line
/// never carries whitespace. Empty text is never stored, so a line is blank
/// exactly when it has no text and no tail that renders on it.
#[derive(Debug)]
struct Output<'a> {
    items: Vec<Item<'a>>,
    /// Whether the content being printed flat was measured to fit on the
    /// line being written: true after a group's or a `best_fitting`
    /// variant's fits-check, false again after any newline. A group met in
    /// flat mode once the line has ended must be measured for itself, as a
    /// long call inside a hugged lambda's body must still break.
    measured_fits: bool,
    /// The first item of the line being written: after the last newline of
    /// either kind.
    line: usize,
    /// The first item after the last line end the printer made. Tails since
    /// then are pending: they render at that line's end, after any newlines
    /// a text had of its own.
    tails: usize,
    /// How many texts the line being written has, so that asking is not a
    /// scan of it.
    line_texts: usize,
    /// How many tails and tail lines are pending: what a boundary and a
    /// snuggle ask on every token.
    pending_tails: usize,
}

impl<'a> Output<'a> {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            line: 0,
            tails: 0,
            measured_fits: false,
            line_texts: 0,
            pending_tails: 0,
        }
    }

    /// The column the next text would start at.
    fn column(&self) -> u32 {
        self.items
            .iter()
            .rev()
            .find_map(|item| match *item {
                Item::Text { end, .. } => Some(end),
                Item::Newline { indent, .. } => Some(indent),
                Item::Tail(_) | Item::TailLine { .. } => None,
            })
            .unwrap_or(0)
    }

    fn has_pending_tail(&self) -> bool {
        self.pending_tails > 0
    }

    /// Whether the line being written has text on it.
    fn line_has_text(&self) -> bool {
        self.line_texts > 0
    }

    /// Whether the line being written ends, so far, in a closing bracket:
    /// what rustfmt's orphan rule looks for before joining it.
    fn line_ends_with_closer(&self) -> bool {
        self.items[self.line..]
            .iter()
            .rev()
            .find_map(|item| match *item {
                Item::Text { text, .. } => Some(text),
                Item::Newline { .. } | Item::Tail(_) | Item::TailLine { .. } => None,
            })
            .is_some_and(|text| text.trim_end().ends_with([')', ']', '}']))
    }

    /// The counts agree with the items (checked in debug builds).
    fn counts_agree(&self) -> bool {
        let texts = self.items[self.line..]
            .iter()
            .filter(|item| matches!(item, Item::Text { .. }))
            .count();
        let tails = self.items[self.tails..]
            .iter()
            .filter(|item| matches!(item, Item::Tail(_) | Item::TailLine { .. }))
            .count();
        (texts, tails) == (self.line_texts, self.pending_tails)
    }

    /// Whether the line being written has anything on it: text, or a tail
    /// that will render on it.
    fn line_has_content(&self) -> bool {
        self.line_has_text() || self.has_pending_tail()
    }

    fn push_str(&mut self, text: &'a str, width: u32) {
        if !text.is_empty() {
            let end = self.column().saturating_add(width);
            self.items.push(Item::Text { text, end });
            self.line_texts += 1;
        }
    }

    /// A space between two things on the line being written: nothing at the
    /// start of a line, where there is nothing before it to separate — a
    /// line comment that ended the previous line leaves the space after its
    /// token at the start of the next.
    fn push_space(&mut self) {
        if self.line_has_text() {
            self.push_str(" ", 1);
        }
    }

    /// Text as written: each line after a newline in it starts at column
    /// zero and is kept exactly, trailing spaces included.
    fn push_multi_line(&mut self, text: &'a str) {
        let mut lines = text.split('\n');
        let first = lines
            .next()
            .unwrap_or_else(|| unreachable!("`split` yields at least one piece"));
        self.push_str(first, display_width(first));
        for line in lines {
            self.push_newline(Item::Newline {
                indent: 0,
                blank: false,
                in_text: true,
            });
            self.push_str(line, display_width(line));
        }
    }

    /// The indentation of the line being written: what the last line end
    /// the printer made set, or none on the first line. A newline a text had
    /// of its own does not count: a literal that starts on the last line of
    /// a block comment, or after a template printed as written, is laid out
    /// from the line the printer started, not from column zero.
    fn line_indent(&self) -> u32 {
        match self.tails.checked_sub(1).map(|at| &self.items[at]) {
            Some(Item::Newline { indent, .. }) => *indent,
            Some(Item::Text { .. } | Item::Tail(_) | Item::TailLine { .. }) | None => 0,
        }
    }

    /// Source text whose later lines are re-indented: the first
    /// `source_indent` columns of each line's leading whitespace are the
    /// source's indentation and go, `indent` spaces come in their place, and
    /// the rest of the line stays exactly as written — a tab inside a
    /// string literal is a tab still, so what the text means does not
    /// change. A blank line stays blank.
    fn push_reindented(&mut self, text: &'a str, source_indent: u32, indent: u32) {
        let mut lines = text.split('\n');
        let first = lines
            .next()
            .unwrap_or_else(|| unreachable!("`split` yields at least one piece"));
        self.push_str(first, display_width(first));
        for line in lines {
            let kept = past_source_indent(line, source_indent);
            self.push_newline(Item::Newline {
                indent,
                blank: false,
                in_text: true,
            });
            if !kept.trim_start_matches([' ', '\t']).is_empty() {
                self.push_str(kept, display_width(kept));
            }
        }
    }

    /// Text deferred to the end of the current line.
    fn push_tail(&mut self, text: &'a str) {
        if !text.is_empty() {
            self.items.push(Item::Tail(text));
            self.pending_tails += 1;
        }
    }

    /// Text deferred to a line of its own after the current line, indented
    /// as `indent` says (see [`TailIndent`]). On a line with nothing on it,
    /// that line is its own.
    fn push_tail_line(&mut self, text: &'a str, blank_before: bool, indent: TailIndent) {
        if text.is_empty() {
            return;
        }
        if self.line_has_content() {
            self.items.push(Item::TailLine {
                text,
                blank_before,
                indent,
            });
            self.pending_tails += 1;
        } else {
            let indent = match indent {
                TailIndent::Fixed(indent) => indent,
                TailIndent::AtLeast(scope) => scope.max(self.line_indent()),
            };
            if blank_before {
                self.empty_line(indent);
            } else {
                self.end_line(indent);
            }
            self.push_multi_line(text);
        }
    }

    fn push_newline(&mut self, newline: Item<'a>) {
        debug_assert!(matches!(newline, Item::Newline { .. }));
        debug_assert!(self.counts_agree());
        self.items.push(newline);
        self.line = self.items.len();
        self.line_texts = 0;
        self.measured_fits = false;
        if let Item::Newline { in_text: false, .. } = newline {
            self.tails = self.line;
            self.pending_tails = 0;
        }
    }

    /// Drop the trailing spaces of the line being written: of its texts,
    /// from the last one back through every one that is only spaces; of its
    /// pending tails the same way; and of each tail line. A tail follows the
    /// text when rendered, so the text's trailing spaces go either way; a
    /// suffix brings its own leading space.
    fn trim_current(&mut self) {
        while let Some(index) = self.items[self.line..]
            .iter()
            .rposition(|item| matches!(item, Item::Text { .. }))
            .map(|index| self.line + index)
            && let Item::Text { text, end } = self.items[index]
        {
            let trimmed = text.trim_end_matches(' ');
            if trimmed.is_empty() {
                // At or after `line`, so `line` and `tails` do not move.
                self.items.remove(index);
                self.line_texts -= 1;
                continue;
            }
            let dropped = u32::try_from(text.len() - trimmed.len())
                .unwrap_or_else(|_| unreachable!("a line is narrower than 2^32"));
            self.items[index] = Item::Text {
                text: trimmed,
                end: end - dropped,
            };
            break;
        }
        while let Some(index) = self.items[self.tails..]
            .iter()
            .rposition(|item| matches!(item, Item::Tail(_)))
            .map(|index| self.tails + index)
            && let Item::Tail(text) = self.items[index]
        {
            let trimmed = text.trim_end_matches(' ');
            if trimmed.is_empty() {
                // A tail pending from before a newline a text had of its own
                // sits before `line`: removing it moves the line's start.
                self.items.remove(index);
                if index < self.line {
                    self.line -= 1;
                }
                self.pending_tails -= 1;
                continue;
            }
            self.items[index] = Item::Tail(trimmed);
            break;
        }
        // Each tail line is a line of its own.
        for item in &mut self.items[self.tails..] {
            if let Item::TailLine { text, .. } = item {
                *text = text.trim_end_matches(' ');
            }
        }
    }

    /// End the current line; the next one is indented by `indent`. On a line
    /// with nothing on it this only sets the indent, so hard lines never
    /// stack up into blank lines.
    fn end_line(&mut self, indent: u32) {
        self.trim_current();
        let blank_line = !self.line_has_content();
        match self.items.last_mut() {
            Some(Item::Newline {
                indent: pending, ..
            }) if blank_line => *pending = indent,
            _ => self.push_newline(Item::Newline {
                indent,
                blank: false,
                in_text: false,
            }),
        }
    }

    /// End the current line and leave exactly one blank line before the
    /// next, however many empty lines are asked for in a row.
    fn empty_line(&mut self, indent: u32) {
        self.trim_current();
        let blank_line = !self.line_has_content();
        match self.items.last_mut() {
            Some(Item::Newline {
                indent: pending,
                blank,
                ..
            }) if blank_line => {
                *pending = indent;
                *blank = true;
            }
            _ => self.push_newline(Item::Newline {
                indent,
                blank: true,
                in_text: false,
            }),
        }
    }

    fn into_string(mut self) -> String {
        self.trim_current();
        let capacity = self
            .items
            .iter()
            .map(|item| match *item {
                Item::Text { text, .. } | Item::Tail(text) => text.len(),
                Item::TailLine { text, .. } => text.len() + 2,
                Item::Newline { indent, .. } => 2 + indent as usize,
            })
            .sum();
        let mut render = Render {
            out: String::with_capacity(capacity),
            indent: 0,
            on_line: false,
            started: false,
        };
        let mut tails = Vec::new();
        for item in self.items {
            match item {
                Item::Text { text, .. } => render.text(text),
                Item::Tail(_) | Item::TailLine { .. } => tails.push(item),
                Item::Newline {
                    indent,
                    blank,
                    in_text,
                } => {
                    if !in_text {
                        render.tails(&tails, indent);
                        tails.clear();
                    }
                    render.line_break(indent, blank, in_text);
                }
            }
        }
        let indent = render.indent;
        render.tails(&tails, indent);
        render.out
    }
}

/// The single pass that writes the result.
struct Render {
    out: String,
    /// The indent of the line being written.
    indent: u32,
    /// Whether the line being written has content yet; its indent is
    /// written before the first.
    on_line: bool,
    /// Whether anything has been written yet: line ends the printer made
    /// before the first content print nothing, so a file never opens blank.
    started: bool,
}

impl Render {
    fn text(&mut self, text: &str) {
        if !self.on_line {
            self.out.extend(iter::repeat_n(' ', self.indent as usize));
            self.on_line = true;
            self.started = true;
        }
        self.out.push_str(text);
    }

    /// End the line; the next starts at `indent`. A newline a text had of
    /// its own is written whatever came before it.
    fn line_break(&mut self, indent: u32, blank: bool, in_text: bool) {
        if self.started || in_text {
            self.out.push('\n');
            if blank {
                self.out.push('\n');
            }
            self.started = true;
        }
        self.on_line = false;
        self.indent = indent;
    }

    /// The deferred items of the line being written, at its end: its tails
    /// in order, then each tail line on a line of its own, in order — a tail
    /// is part of the line whenever it was emitted relative to a tail line,
    /// which comes after it. `next_indent` is that of the line break ending
    /// the line (see [`TailIndent`]).
    fn tails(&mut self, tails: &[Item<'_>], next_indent: u32) {
        for tail in tails {
            // A suffix's leading space is for the text before it on the
            // line; alone at the start of a line it is nothing.
            if let Item::Tail(text) = *tail
                && (self.on_line || !text.trim_start_matches(' ').is_empty())
            {
                self.text(text);
            }
        }
        for tail in tails {
            if let Item::TailLine {
                text,
                blank_before,
                indent,
            } = *tail
            {
                let indent = match indent {
                    TailIndent::Fixed(indent) => indent,
                    TailIndent::AtLeast(scope) => scope.max(next_indent),
                };
                self.line_break(indent, blank_before, false);
                self.text(text);
            }
        }
    }
}

/// The verdict of a fits-check, once a measurement reaches one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Fits,
    Overflows,
}

/// A measurement carries the room forward (`Ok`) until it reaches a verdict
/// (`Err`), which ends it early: at the first line break, what was measured
/// fits.
type Measured = Result<Room, Verdict>;

/// The room left on the line being measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Room {
    remaining: i64,
    /// Whether a line suffix is pending, so a boundary would end the line.
    has_suffix: bool,
    /// Whether the line has text on it yet: a space at its start prints
    /// nothing ([`Output::push_space`]), so it measures nothing.
    has_text: bool,
    /// The indentation of the line being measured, which the moved lines of
    /// a verbatim text on it are laid out from ([`Output::push_reindented`])
    /// — not the indent of the document around the text, which has not
    /// moved anything until it reaches a line break.
    line_indent: u32,
    /// How many line breaks the measurement has gone past: a limit
    /// ([`Node::Limited`]) withholds columns from its content and gives them
    /// back after it only while it is still on the line it started on.
    line: u32,
}

impl Room {
    fn consume(self, columns: u32) -> Measured {
        Self {
            remaining: self.remaining - i64::from(columns),
            has_text: self.has_text || columns > 0,
            ..self
        }
        .check()
    }

    fn space(self) -> Measured {
        if self.has_text {
            self.consume(1)
        } else {
            Ok(self)
        }
    }

    fn with_suffix(self) -> Self {
        Self {
            has_suffix: true,
            ..self
        }
    }

    fn check(self) -> Measured {
        if self.remaining < 0 {
            Err(Verdict::Overflows)
        } else {
            Ok(self)
        }
    }
}

/// `line` past the first `source_indent` columns of its leading whitespace
/// (a tab is one column, as the rules count a source indent); the whole
/// line when it has less.
fn past_source_indent(line: &str, source_indent: u32) -> &str {
    let columns = usize::try_from(source_indent).unwrap_or(usize::MAX);
    let cut = line
        .bytes()
        .take(columns)
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count();
    &line[cut..]
}

/// The column the last line of re-indented `text` ends at, when its later
/// lines are indented by `indent` (see [`Output::push_reindented`]): a blank
/// last line ends where its indentation would.
fn reindented_end(text: &str, source_indent: u32, indent: u32, start: u32) -> u32 {
    match text.rsplit_once('\n') {
        None => start.saturating_add(display_width(text)),
        Some((_, last)) => {
            let kept = past_source_indent(last, source_indent);
            if kept.trim_start_matches([' ', '\t']).is_empty() {
                indent
            } else {
                indent.saturating_add(display_width(kept))
            }
        }
    }
}

/// What a [`Printer::measure`] of a layout choice in break mode (the only
/// mode memoised) depends on, for its memo.
type MemoKey = (usize, u32, Measure, i64, bool, bool, u32, u32);

pub(crate) fn print(root: Doc<'_>, options: FormatOptions) -> String {
    let mut printer = Printer {
        layout: Layout::from(options),
        out: Output::new(),
        groups: FxHashMap::default(),
        measured: FxHashMap::default(),
        memo: FxHashMap::default(),
        scopes: Vec::new(),
    };
    printer.print(
        Piece {
            doc: root,
            mode: Mode::Break,
            indent: 0,
        },
        None,
    );
    printer.out.into_string()
}

struct Printer<'a> {
    layout: Layout,
    out: Output<'a>,
    /// The layout each named group was printed in, for `if_group_breaks`.
    groups: FxHashMap<GroupId<'a>, Mode>,
    /// The layout each named group was last measured in during the
    /// fits-check under way, for an `if_group_breaks` measured after it in
    /// the same check: a group inside the content being measured is not
    /// printed yet, and reading it as flat would measure a `{` on the
    /// header's line that the broken header then puts on a line of its own.
    measured: FxHashMap<GroupId<'a>, Mode>,
    /// Measurements of layout choices during one fits-check, by everything
    /// they depend on. A choice nested in a choice is reached once per
    /// variant of every choice around it; measured once per distinct room,
    /// the check is polynomial in the depth where it was exponential.
    memo: FxHashMap<MemoKey, Measured>,
    /// The level of each enclosing list item ([`Node::Scope`]), innermost
    /// last: the indent of the line it started on.
    scopes: Vec<u32>,
}

impl<'a> Printer<'a> {
    /// The level of the innermost enclosing list item, for a tail line
    /// carried out of it; the current level outside any.
    fn scope(&self, indent: u32) -> u32 {
        self.scopes.last().copied().unwrap_or(indent)
    }

    fn print(&mut self, piece: Piece<'a>, rest: Option<&Rest<'_, 'a>>) {
        let Piece { doc, mode, indent } = piece;
        match doc.node() {
            Node::Text {
                text,
                width,
                forced: false,
            } => self.out.push_str(text, width),
            // A text with lines of its own, or verbatim text, after a pending
            // tail: the tail would render after the text's last line, past
            // content it was written before, so the line ends first.
            Node::Text {
                text, forced: true, ..
            } => {
                if self.out.has_pending_tail() {
                    self.out.end_line(indent);
                }
                self.out.push_multi_line(text);
            }
            // Verbatim lines sit relative to the line the text starts on, not
            // to the indents around it: an indent that has not reached a line
            // break yet has not moved anything, so a lambda body inside
            // flat-printed arguments stays where its first line is, and a
            // literal's closing line lands under the line that opened it.
            Node::Verbatim {
                pieces,
                source_indent,
                ..
            } => {
                if self.out.has_pending_tail() {
                    self.out.end_line(indent);
                }
                let line_indent = self.out.line_indent();
                for piece in pieces {
                    match *piece {
                        VerbatimPiece::Moved(text) => {
                            self.out.push_reindented(text, source_indent, line_indent);
                        }
                        VerbatimPiece::Exact(text) => self.out.push_multi_line(text),
                    }
                }
            }
            Node::Space => self.out.push_space(),
            Node::Line(line) => match (mode, line) {
                (Mode::Flat, LineMode::Soft) => {}
                (Mode::Flat, LineMode::SoftOrSpace) => self.out.push_space(),
                (Mode::Flat | Mode::Break, LineMode::Empty) => self.out.empty_line(indent),
                (Mode::Flat, LineMode::Hard)
                | (Mode::Break, LineMode::Soft | LineMode::SoftOrSpace | LineMode::Hard) => {
                    self.out.end_line(indent);
                }
                // A line that started deeper than this indent is a
                // continuation line: what follows goes on a line of its own.
                (Mode::Flat | Mode::Break, LineMode::Hang) => {
                    if self.out.line_indent() <= indent {
                        self.out.push_space();
                    } else {
                        self.out.end_line(indent);
                    }
                }
            },
            // rustfmt's orphan rule: a line of closing brackets that reaches
            // no further than the column the newline would indent to has the
            // content join it when its first line fits there — laid out at
            // that line's level, as part of it. (A short first operand's
            // line, `a`, ends in no bracket and is not joined.)
            Node::Snuggle { content, .. } => {
                let content = Piece {
                    doc: content,
                    mode,
                    indent,
                };
                if mode == Mode::Flat {
                    self.out.push_space();
                    return self.print(content, rest);
                }
                let line_indent = self.out.line_indent();
                let siblings = [content.doc];
                let here = Rest {
                    siblings: &siblings,
                    mode: Mode::Break,
                    indent: line_indent,
                    outer: rest,
                };
                let joins = self.out.column() <= indent
                    && self.out.line_ends_with_closer()
                    && !self.out.has_pending_tail()
                    && self.fits(
                        Piece {
                            doc: Doc::SPACE,
                            mode: Mode::Flat,
                            indent,
                        },
                        Measure::FirstLine,
                        Some(&here),
                    );
                if joins {
                    self.out.push_space();
                    self.print(
                        Piece {
                            indent: line_indent,
                            ..content
                        },
                        rest,
                    );
                } else {
                    self.out.end_line(indent);
                    self.print(content, rest);
                }
            }
            Node::Concat { children, .. } => {
                for (index, &child) in children.iter().enumerate() {
                    let following = &children[index + 1..];
                    let here = Rest {
                        siblings: following,
                        mode,
                        indent,
                        outer: rest,
                    };
                    let rest = if following.is_empty() {
                        rest
                    } else {
                        Some(&here)
                    };
                    self.print(
                        Piece {
                            doc: child,
                            ..piece
                        },
                        rest,
                    );
                }
            }
            Node::Group { content, named, .. } => {
                let content = Piece {
                    doc: content,
                    ..piece
                };
                let flat = Piece {
                    mode: Mode::Flat,
                    ..content
                };
                let mode = if doc.forced() {
                    Mode::Break
                } else if (mode == Mode::Flat && self.out.measured_fits)
                    || self.fits(flat, Measure::FirstLine, rest)
                {
                    self.out.measured_fits = true;
                    Mode::Flat
                } else {
                    Mode::Break
                };
                if named {
                    self.groups.insert(GroupId::of(doc), mode);
                }
                self.print(Piece { mode, ..content }, rest);
            }
            Node::Indent { content, .. } => self.print(
                Piece {
                    doc: content,
                    indent: indent.saturating_add(self.layout.indent_width),
                    ..piece
                },
                rest,
            ),
            Node::Scope { content, .. } => {
                self.scopes.push(self.out.line_indent());
                self.print(
                    Piece {
                        doc: content,
                        ..piece
                    },
                    rest,
                );
                self.scopes.pop();
            }
            Node::Limited { content, .. } => self.print(
                Piece {
                    doc: content,
                    ..piece
                },
                rest,
            ),
            Node::IfBreak {
                group,
                breaks,
                flat,
                ..
            } => {
                let chosen = match self.group_mode(group, mode) {
                    Mode::Break => breaks,
                    Mode::Flat => flat,
                };
                self.print(
                    Piece {
                        doc: chosen,
                        ..piece
                    },
                    rest,
                );
            }
            Node::LineSuffix { text, .. } => self.out.push_tail(text),
            // After a pending tail — a suffix, or a tail line it would
            // otherwise render before — a line of its own, in order.
            Node::ClosingSuffix { text } => {
                if self.out.has_pending_tail() {
                    let indent = TailIndent::AtLeast(self.scope(indent));
                    self.out.push_tail_line(text, false, indent);
                } else {
                    self.out.push_tail(" ");
                    self.out.push_tail(text);
                }
            }
            Node::TrailingLine {
                text,
                blank_before,
                forced,
            } => {
                let indent = if forced {
                    TailIndent::Fixed(indent)
                } else {
                    TailIndent::AtLeast(self.scope(indent))
                };
                self.out.push_tail_line(text, blank_before, indent);
            }
            Node::LineSuffixBoundary => {
                if self.out.has_pending_tail() {
                    self.out.end_line(indent);
                }
            }
            Node::ExpandParent => {}
            // Flat, and measured to fit on this line, the most flat variant it
            // is; once the line has ended (a statement in a hugged lambda's
            // body) the choice is made afresh, as a group is.
            Node::BestFitting { variants } => {
                if mode == Mode::Flat && self.out.measured_fits {
                    let first = variants.first();
                    return self.print(
                        Piece {
                            doc: first.doc,
                            mode: first.measure.mode(),
                            indent,
                        },
                        rest,
                    );
                }
                let (most_expanded, candidates) = variants.split_last();
                for variant in candidates {
                    let candidate = Piece {
                        doc: variant.doc,
                        mode: variant.measure.mode(),
                        indent,
                    };
                    if self.fits(candidate, variant.measure, rest) {
                        self.out.measured_fits = true;
                        return self.print(candidate, rest);
                    }
                }
                self.print(
                    Piece {
                        doc: most_expanded.doc,
                        mode: Mode::Break,
                        indent,
                    },
                    rest,
                );
            }
        }
    }

    /// The layout `if_break` should follow: the named group's, if it has been
    /// printed, else (as in Prettier) flat for a group not yet seen — unless
    /// that group is sure to break, which a measurement of it has already
    /// counted on — and the enclosing mode when no group is named.
    fn group_mode(&self, group: Option<GroupId<'a>>, enclosing: Mode) -> Mode {
        group.map_or(enclosing, |group| {
            self.measured
                .get(&group)
                .or_else(|| self.groups.get(&group))
                .copied()
                .unwrap_or(if group.is_forced() {
                    Mode::Break
                } else {
                    Mode::Flat
                })
        })
    }

    /// Whether `candidate`, laid out in its given mode, fits in the room left
    /// on the current line — up to the first line break in it, or on every
    /// line of it, per `measure` — followed by whatever comes after it up to
    /// the next line break.
    fn fits(
        &mut self,
        candidate: Piece<'a>,
        measure: Measure,
        rest: Option<&Rest<'_, 'a>>,
    ) -> bool {
        // The memo and the measured layouts hold for one check: the named
        // groups' layouts a measurement reads are those printed so far and
        // those measured earlier in this check.
        self.memo.clear();
        self.measured.clear();
        let room = Room {
            remaining: i64::from(self.layout.line_width) - i64::from(self.out.column()),
            has_suffix: self.out.has_pending_tail(),
            has_text: self.out.line_has_text(),
            line_indent: self.out.line_indent(),
            line: 0,
        };
        let measured = self.measure(candidate, room, measure).and_then(|room| {
            Rest::pieces(rest).try_fold(room, |room, piece| {
                self.measure(piece, room, Measure::FirstLine)
            })
        });
        self.measured.clear();
        match measured {
            // Nothing left to measure before the document ends: it all fit.
            Ok(_) | Err(Verdict::Fits) => true,
            Err(Verdict::Overflows) => false,
        }
    }

    /// The room on a fresh line indented by `indent`, the `line`th the
    /// measurement has reached, for one that goes on past a line break.
    fn line_room(&self, indent: u32, line: u32) -> Room {
        Room {
            remaining: i64::from(self.layout.line_width) - i64::from(indent),
            has_suffix: false,
            has_text: false,
            line_indent: indent,
            line,
        }
    }

    /// Measure `piece` against `room`. A line break ends the measurement
    /// with a verdict of fits, or — measuring all lines — starts a fresh
    /// line's room and goes on. The lines of a multi-line text are as
    /// written and end the measurement either way: they are an atom to the
    /// layout, and what a more expanded layout would do to them is nothing.
    fn measure(&mut self, piece: Piece<'a>, room: Room, measure: Measure) -> Measured {
        let Piece { doc, mode, indent } = piece;
        let line_room = self.line_room(indent, room.line + 1);
        let line_break = || match measure {
            Measure::FirstLine | Measure::Broken => Err(Verdict::Fits),
            Measure::AllLines | Measure::AllLinesFlat => Ok(line_room),
        };
        // Measuring all lines, a text with lines of its own is measured to
        // its last, and what follows it is measured from where that ends, on
        // a line indented by `line_indent` (a text's own lines keep their
        // columns: none). A suffix pending before the text stays pending:
        // the text's own line breaks do not flush it.
        // A text with lines of its own after a pending suffix ends the line
        // first, as a boundary does (see [`Printer::print`]).
        let before_lines = |room: Room| match (room.has_suffix, mode) {
            (false, _) => Ok(room),
            (true, Mode::Flat) => Err(Verdict::Overflows),
            (true, Mode::Break) => line_break(),
        };
        let past_lines = |room: Room, end: u32, line_indent: u32| match measure {
            Measure::FirstLine | Measure::Broken => Err(Verdict::Fits),
            Measure::AllLines | Measure::AllLinesFlat => Room {
                remaining: i64::from(self.layout.line_width) - i64::from(end),
                has_suffix: room.has_suffix,
                has_text: end > line_indent,
                line_indent,
                line: room.line + 1,
            }
            .check(),
        };
        match doc.node() {
            Node::Text {
                text,
                width,
                forced,
            } => {
                let room = if forced { before_lines(room)? } else { room };
                let room = room.consume(width)?;
                if forced {
                    let last = text.rsplit('\n').next().unwrap_or(text);
                    past_lines(room, display_width(last), 0)
                } else {
                    Ok(room)
                }
            }
            Node::Verbatim {
                pieces,
                width,
                source_indent,
                ..
            } => {
                let room = before_lines(room)?.consume(width)?;
                // The last line's end, piece by piece, as the printer lays
                // them out: moved lines from the indent of the line the text
                // starts on, exact lines as written.
                let mut end = 0u32;
                let mut line_indent = room.line_indent;
                let mut on_first_line = true;
                for piece in pieces {
                    let text = piece.text();
                    end = match (*piece, on_first_line) {
                        (_, true) if !text.contains('\n') => {
                            end.saturating_add(display_width(text))
                        }
                        (VerbatimPiece::Moved(text), _) => {
                            if text.contains('\n') {
                                line_indent = room.line_indent;
                            }
                            reindented_end(text, source_indent, room.line_indent, end)
                        }
                        (VerbatimPiece::Exact(text), _) => match text.rsplit_once('\n') {
                            None => end.saturating_add(display_width(text)),
                            Some((_, last)) => {
                                line_indent = 0;
                                display_width(last)
                            }
                        },
                    };
                    on_first_line = on_first_line && !text.contains('\n');
                }
                past_lines(room, end, line_indent)
            }
            Node::Space => room.space(),
            Node::Line(line) => match (mode, line) {
                (Mode::Flat, LineMode::Soft) => Ok(room),
                (Mode::Flat, LineMode::SoftOrSpace) => room.space(),
                (Mode::Flat | Mode::Break, LineMode::Hang) => {
                    if room.line_indent <= indent {
                        room.space()
                    } else {
                        line_break()
                    }
                }
                (Mode::Flat, LineMode::Hard | LineMode::Empty)
                | (
                    Mode::Break,
                    LineMode::Soft | LineMode::SoftOrSpace | LineMode::Hard | LineMode::Empty,
                ) => line_break(),
            },
            // Broken, the content is on a line of its own as far as a
            // measurement knows: whether it joins the line before it is the
            // printer's decision, made with the line in hand.
            Node::Snuggle { content, .. } => {
                let content = Piece {
                    doc: content,
                    ..piece
                };
                match mode {
                    Mode::Flat => self.measure(content, room.space()?, measure),
                    Mode::Break => self.measure(content, line_break()?, measure),
                }
            }
            Node::Concat { children, .. } => children.iter().try_fold(room, |room, &child| {
                self.measure(
                    Piece {
                        doc: child,
                        ..piece
                    },
                    room,
                    measure,
                )
            }),
            // A group not forced to break is measured as the layout around
            // it is, except under `AllLinesFlat`, where it is measured flat:
            // only the forced breaks of that variant count.
            Node::Group { content, named, .. } => {
                let mode = match (doc.forced(), measure) {
                    (true, _) => Mode::Break,
                    (false, Measure::AllLinesFlat) => Mode::Flat,
                    (false, Measure::FirstLine | Measure::AllLines | Measure::Broken) => mode,
                };
                // A measurement memoised under one reading of this group
                // does not hold under another.
                if named && self.measured.insert(GroupId::of(doc), mode) != Some(mode) {
                    self.memo.clear();
                }
                self.measure(
                    Piece {
                        doc: content,
                        mode,
                        indent,
                    },
                    room,
                    measure,
                )
            }
            Node::Indent { content, .. } => self.measure(
                Piece {
                    doc: content,
                    indent: indent.saturating_add(self.layout.indent_width),
                    ..piece
                },
                room,
                measure,
            ),
            Node::Scope { content, .. } => self.measure(
                Piece {
                    doc: content,
                    ..piece
                },
                room,
                measure,
            ),
            // Flat, the content is measured in a room of `max_width`
            // columns: the columns beyond that are withheld from it and given
            // back after it — unless a line break inside it moved the
            // measurement to a fresh line, which has them all already.
            Node::Limited {
                content, max_width, ..
            } => {
                let content = Piece {
                    doc: content,
                    ..piece
                };
                match mode {
                    Mode::Break => self.measure(content, room, measure),
                    Mode::Flat => {
                        let withheld = (room.remaining - i64::from(max_width)).max(0);
                        let capped = Room {
                            remaining: room.remaining - withheld,
                            ..room
                        };
                        let after = self.measure(content, capped, measure)?;
                        Ok(if after.line == room.line {
                            Room {
                                remaining: after.remaining + withheld,
                                ..after
                            }
                        } else {
                            after
                        })
                    }
                }
            }
            Node::IfBreak {
                group,
                breaks,
                flat,
                ..
            } => {
                let chosen = match self.group_mode(group, mode) {
                    Mode::Break => breaks,
                    Mode::Flat => flat,
                };
                self.measure(
                    Piece {
                        doc: chosen,
                        ..piece
                    },
                    room,
                    measure,
                )
            }
            Node::LineSuffix { width, fit, .. } => match fit {
                SuffixWidth::Reserved => room.with_suffix().consume(width),
                SuffixWidth::Ignored => Ok(room.with_suffix()),
            },
            // Its own line is never measured: a comment may run past the
            // line width, and the enclosing group breaks anyway.
            Node::TrailingLine { .. } | Node::ClosingSuffix { .. } => Ok(room.with_suffix()),
            // With a suffix pending, the boundary ends the line. Measured as
            // part of content that would print flat, that line end falls in
            // the middle of it — `check( // c` and the parameters on the
            // next line laid out as if on this one — so the content does not
            // fit flat; measured as what follows, in break mode, it is where
            // the line ends and all before it fit.
            Node::LineSuffixBoundary => match (room.has_suffix, mode) {
                (false, _) => Ok(room),
                (true, Mode::Flat) => Err(Verdict::Overflows),
                (true, Mode::Break) => line_break(),
            },
            Node::ExpandParent => Ok(room),
            // Printed flat, the most flat variant it is. Printed broken, the
            // printer will pick the first variant that fits once it gets
            // there, so what is being measured now — content before it on
            // the line, or a forced group around it — fits if any variant
            // lets it: the one that gets furthest is the measure, as a
            // `line` in Prettier's break mode is a newline. A right-hand
            // side that may move to a line of its own leaves the pattern
            // before it on one line, as rustfmt does. Each variant is
            // measured as the printer would print it: the most expanded one
            // broken, every other one flat.
            Node::BestFitting { variants } => match (mode, measure) {
                (Mode::Flat, _) | (Mode::Break, Measure::AllLinesFlat) => {
                    let first = variants.first();
                    self.measure(
                        Piece {
                            doc: first.doc,
                            mode: first.measure.mode(),
                            indent,
                        },
                        room,
                        measure,
                    )
                }
                (Mode::Break, Measure::FirstLine | Measure::AllLines | Measure::Broken) => {
                    let key: MemoKey = (
                        doc.addr(),
                        indent,
                        measure,
                        room.remaining,
                        room.has_suffix,
                        room.has_text,
                        room.line_indent,
                        room.line,
                    );
                    if let Some(&measured) = self.memo.get(&key) {
                        return measured;
                    }
                    let (most_expanded, candidates) = variants.split_last();
                    let mut best = Err(Verdict::Overflows);
                    for variant in candidates {
                        let flat = Piece {
                            doc: variant.doc,
                            mode: variant.measure.mode(),
                            indent,
                        };
                        best = furthest(best, self.measure(flat, room, measure));
                        if best == Err(Verdict::Fits) {
                            break;
                        }
                    }
                    if best != Err(Verdict::Fits) {
                        let broken = Piece {
                            doc: most_expanded.doc,
                            mode: Mode::Break,
                            indent,
                        };
                        best = furthest(best, self.measure(broken, room, measure));
                    }
                    self.memo.insert(key, best);
                    best
                }
            },
        }
    }
}

/// Of two measurements, the one that gets furthest: a verdict of fits (the
/// line ended with room to spare) over room still being used, and more
/// room over less; an overflow only when both overflow.
fn furthest(a: Measured, b: Measured) -> Measured {
    match (a, b) {
        (Err(Verdict::Fits), _) | (_, Err(Verdict::Fits)) => Err(Verdict::Fits),
        (Ok(a), Ok(b)) => Ok(if a.remaining >= b.remaining { a } else { b }),
        (Ok(room), Err(Verdict::Overflows)) | (Err(Verdict::Overflows), Ok(room)) => Ok(room),
        (Err(Verdict::Overflows), Err(Verdict::Overflows)) => Err(Verdict::Overflows),
    }
}
