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

use super::doc::{Doc, GroupId, LineMode, Node, SuffixWidth, display_width};
use crate::FormatOptions;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
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
    siblings: &'a [Doc<'a>],
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
#[derive(Debug, Clone, Copy)]
enum Item<'a> {
    /// Text on the current line, and the column it ends at.
    Text { text: &'a str, end: u32 },
    /// Text for the end of the current line, rendered after everything else
    /// on it however early it was emitted.
    Tail(&'a str),
    /// Text for a line of its own after the current line, rendered after the
    /// line's tails at the indent it was emitted at.
    TailLine {
        text: &'a str,
        blank_before: bool,
        indent: u32,
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
/// exactly when it has no items.
#[derive(Debug)]
struct Output<'a> {
    items: Vec<Item<'a>>,
    /// The first item of the line being written: after the last newline of
    /// either kind.
    line: usize,
    /// The first item after the last line end the printer made. Tails since
    /// then are pending: they render at that line's end, after any newlines
    /// a text had of its own.
    tails: usize,
}

impl<'a> Output<'a> {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            line: 0,
            tails: 0,
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
        self.items[self.tails..]
            .iter()
            .any(|item| matches!(item, Item::Tail(_) | Item::TailLine { .. }))
    }

    /// Whether the line being written has anything on it: text, or a tail
    /// that will render on it.
    /// Whether the line being written has text on it.
    fn line_has_text(&self) -> bool {
        self.items[self.line..]
            .iter()
            .any(|item| matches!(item, Item::Text { .. }))
    }

    /// Whether the line being written has anything on it: text, or a tail
    /// that will render on it.
    fn line_has_content(&self) -> bool {
        self.line_has_text() || self.has_pending_tail()
    }

    fn push_str(&mut self, text: &'a str, width: u32) {
        if !text.is_empty() {
            let end = self.column() + width;
            self.items.push(Item::Text { text, end });
        }
    }

    /// A space between two things on the line being written: nothing at the
    /// start of a line, where there is nothing before it to separate — a
    /// block comment that follows a line break leaves the space before it at
    /// the start of the next line.
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

    /// The indentation of the line being written: what the newline that
    /// started it set, or none on the first line.
    fn line_indent(&self) -> u32 {
        match self.line.checked_sub(1).map(|at| &self.items[at]) {
            Some(Item::Newline { indent, .. }) => *indent,
            Some(Item::Text { .. } | Item::Tail(_) | Item::TailLine { .. }) | None => 0,
        }
    }

    /// Source text whose later lines are re-indented: each keeps its
    /// indentation relative to `source_indent`, measured from `indent`. A
    /// blank line stays blank.
    fn push_reindented(&mut self, text: &'a str, source_indent: u32, indent: u32) {
        let mut lines = text.split('\n');
        let first = lines
            .next()
            .unwrap_or_else(|| unreachable!("`split` yields at least one piece"));
        self.push_str(first, display_width(first));
        for line in lines {
            let content = line.trim_start_matches([' ', '\t']);
            let was = display_width(&line[..line.len() - content.len()]);
            self.push_newline(Item::Newline {
                indent: (indent + was).saturating_sub(source_indent),
                blank: false,
                in_text: true,
            });
            self.push_str(content, display_width(content));
        }
    }

    /// Text deferred to the end of the current line.
    fn push_tail(&mut self, text: &'a str) {
        if !text.is_empty() {
            self.items.push(Item::Tail(text));
        }
    }

    /// Text deferred to a line of its own after the current line, at
    /// `indent`. On a line with nothing on it, that line is its own.
    fn push_tail_line(&mut self, text: &'a str, blank_before: bool, indent: u32) {
        if text.is_empty() {
            return;
        }
        if self.line_has_content() {
            self.items.push(Item::TailLine {
                text,
                blank_before,
                indent,
            });
        } else {
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
        self.items.push(newline);
        self.line = self.items.len();
        if let Item::Newline { in_text: false, .. } = newline {
            self.tails = self.line;
        }
    }

    /// Drop the trailing spaces of the line being written: of its last text,
    /// of the last tail that will render on it, and of each tail line. A tail
    /// follows the text when rendered, so the text's trailing spaces go
    /// either way; a suffix brings its own leading space.
    fn trim_current(&mut self) {
        if let Some(index) = self.items[self.line..]
            .iter()
            .rposition(|item| matches!(item, Item::Text { .. }))
            .map(|index| self.line + index)
            && let Item::Text { text, end } = self.items[index]
        {
            let trimmed = text.trim_end_matches(' ');
            if trimmed.is_empty() {
                self.items.remove(index);
            } else {
                let dropped = u32::try_from(text.len() - trimmed.len())
                    .unwrap_or_else(|_| unreachable!("a line is narrower than 2^32"));
                self.items[index] = Item::Text {
                    text: trimmed,
                    end: end - dropped,
                };
            }
        }
        if let Some(index) = self.items[self.tails..]
            .iter()
            .rposition(|item| matches!(item, Item::Tail(_)))
            .map(|index| self.tails + index)
            && let Item::Tail(text) = self.items[index]
        {
            let trimmed = text.trim_end_matches(' ');
            if trimmed.is_empty() {
                self.items.remove(index);
            } else {
                self.items[index] = Item::Tail(trimmed);
            }
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
                        render.tails(&tails);
                        tails.clear();
                    }
                    render.line_break(indent, blank, in_text);
                }
            }
        }
        render.tails(&tails);
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
    /// which comes after it.
    fn tails(&mut self, tails: &[Item<'_>]) {
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
#[derive(Debug, Clone, Copy)]
struct Room {
    remaining: i64,
    /// Whether a line suffix is pending, so a boundary would end the line.
    has_suffix: bool,
    /// Whether the line has text on it yet: a space at its start prints
    /// nothing ([`Output::push_space`]), so it measures nothing.
    has_text: bool,
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

pub(crate) fn print(root: Doc<'_>, options: FormatOptions) -> String {
    let mut printer = Printer {
        layout: Layout::from(options),
        out: Output::new(),
        groups: FxHashMap::default(),
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
}

impl<'a> Printer<'a> {
    fn print(&mut self, piece: Piece<'a>, rest: Option<&Rest<'_, 'a>>) {
        let Piece { doc, mode, indent } = piece;
        match doc.node() {
            Node::Text {
                text,
                width,
                forced: false,
            } => self.out.push_str(text, width),
            Node::Text {
                text, forced: true, ..
            } => self.out.push_multi_line(text),
            Node::Verbatim {
                text,
                source_indent,
                forced: true,
                ..
            } => self.out.push_reindented(text, source_indent, indent),
            // A literal's lines sit relative to the line it starts on, not
            // to the indents around it: `f(`…`)` hugs the literal, and its
            // closing line lands under the line that opened it.
            Node::Verbatim {
                text,
                forced: false,
                ..
            } => {
                let line_indent = self.out.line_indent();
                self.out.push_reindented(text, 0, line_indent);
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
            },
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
                } else if mode == Mode::Flat || self.fits(flat, rest) {
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
            Node::TrailingLine { text, blank_before } => {
                self.out.push_tail_line(text, blank_before, indent);
            }
            Node::LineSuffixBoundary => {
                if self.out.has_pending_tail() {
                    self.out.end_line(indent);
                }
            }
            Node::ExpandParent => {}
            Node::BestFitting {
                most_flat,
                expansions,
            } => {
                if mode == Mode::Flat {
                    return self.print(
                        Piece {
                            doc: most_flat,
                            ..piece
                        },
                        rest,
                    );
                }
                let (most_expanded, middle) = expansions.split_last();
                for &variant in iter::once(&most_flat).chain(middle) {
                    let candidate = Piece {
                        doc: variant,
                        mode: Mode::Flat,
                        indent,
                    };
                    if self.fits(candidate, rest) {
                        return self.print(candidate, rest);
                    }
                }
                self.print(
                    Piece {
                        doc: *most_expanded,
                        mode: Mode::Break,
                        indent,
                    },
                    rest,
                );
            }
        }
    }

    /// The layout `if_break` should follow: the named group's, if it has been
    /// printed, else (as in Prettier) flat for a group not yet seen, and the
    /// enclosing mode when no group is named.
    fn group_mode(&self, group: Option<GroupId<'a>>, enclosing: Mode) -> Mode {
        group.map_or(enclosing, |group| {
            self.groups.get(&group).copied().unwrap_or(Mode::Flat)
        })
    }

    /// Whether `candidate`, laid out in its given mode, fits in the room left
    /// on the current line, followed by whatever comes after it up to the
    /// next line break.
    fn fits(&self, candidate: Piece<'a>, rest: Option<&Rest<'_, 'a>>) -> bool {
        let room = Room {
            remaining: i64::from(self.layout.line_width) - i64::from(self.out.column()),
            has_suffix: self.out.has_pending_tail(),
            has_text: self.out.line_has_text(),
        };
        let measured = self.measure(candidate, room).and_then(|room| {
            Rest::pieces(rest).try_fold(room, |room, piece| self.measure(piece, room))
        });
        match measured {
            // Nothing left to measure before the document ends: it all fit.
            Ok(_) | Err(Verdict::Fits) => true,
            Err(Verdict::Overflows) => false,
        }
    }

    fn measure(&self, piece: Piece<'a>, room: Room) -> Measured {
        let Piece { doc, mode, indent } = piece;
        match doc.node() {
            Node::Text { width, forced, .. } => {
                let room = room.consume(width)?;
                if forced { Err(Verdict::Fits) } else { Ok(room) }
            }
            Node::Verbatim { width, .. } => {
                room.consume(width)?;
                Err(Verdict::Fits)
            }
            Node::Space => room.space(),
            Node::Line(line) => match (mode, line) {
                (Mode::Flat, LineMode::Soft) => Ok(room),
                (Mode::Flat, LineMode::SoftOrSpace) => room.space(),
                (Mode::Flat, LineMode::Hard | LineMode::Empty) | (Mode::Break, _) => {
                    Err(Verdict::Fits)
                }
            },
            Node::Concat { children, .. } => children.iter().try_fold(room, |room, &child| {
                self.measure(
                    Piece {
                        doc: child,
                        ..piece
                    },
                    room,
                )
            }),
            Node::Group { content, .. } => {
                let mode = if doc.forced() { Mode::Break } else { mode };
                self.measure(
                    Piece {
                        doc: content,
                        mode,
                        indent,
                    },
                    room,
                )
            }
            Node::Indent { content, .. } => self.measure(
                Piece {
                    doc: content,
                    indent: indent.saturating_add(self.layout.indent_width),
                    ..piece
                },
                room,
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
                self.measure(
                    Piece {
                        doc: chosen,
                        ..piece
                    },
                    room,
                )
            }
            Node::LineSuffix { width, fit, .. } => match fit {
                SuffixWidth::Reserved => room.with_suffix().consume(width),
                SuffixWidth::Ignored => Ok(room.with_suffix()),
            },
            // Its own line is never measured: a comment may run past the
            // line width, and the enclosing group breaks anyway.
            Node::TrailingLine { .. } => Ok(room.with_suffix()),
            Node::LineSuffixBoundary => {
                if room.has_suffix {
                    Err(Verdict::Fits)
                } else {
                    Ok(room)
                }
            }
            Node::ExpandParent => Ok(room),
            Node::BestFitting {
                most_flat,
                expansions,
            } => {
                let chosen = match mode {
                    Mode::Flat => most_flat,
                    Mode::Break => *expansions.last(),
                };
                self.measure(
                    Piece {
                        doc: chosen,
                        ..piece
                    },
                    room,
                )
            }
        }
    }
}
