//! Documents: immutable trees, allocated in a bump arena and shared by
//! reference.

use std::{
    fmt::{self, Write as _},
    hash::{Hash, Hasher},
    ptr,
};

use bumpalo::Bump;
use unicode_width::UnicodeWidthStr;

use super::printer::{self, Mode};
use crate::FormatOptions;

/// How a line node prints in each layout of its enclosing group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineMode {
    /// Nothing when flat, a newline when broken.
    Soft,
    /// A space when flat, a newline when broken.
    SoftOrSpace,
    /// A space when the line being written started at the node's own
    /// indent or left of it; else a newline to that indent — in either
    /// layout. The `{` after a header stays on the header's last line only
    /// when that line is the header's own (`) {`, `} {`), never on a
    /// continuation line indented deeper (`    | B {`): a brace never goes
    /// on a line that starts more indented than the item it closes.
    Hang,
    /// Always a newline.
    Hard,
    /// Always a newline followed by exactly one blank line.
    Empty,
}

/// How much of a [`Docs::best_fitting`] variant a fits-check measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Measure {
    /// Its first line, with what follows it on that line; the lines after a
    /// break inside it are its own business. A hugged last argument is
    /// chosen when `f(a, (x) -> {` fits, however long the body's lines are,
    /// as in Prettier and Biome.
    FirstLine,
    /// Every line of it, each within the line width, and then what follows
    /// its last line: a layout worth choosing only when the whole of it fits
    /// — a right-hand side moved to a line of its own, a member chain broken
    /// so that its last call's arguments stay on one line.
    AllLines,
    /// Every line of it, with every layout inside it that is not forced to
    /// break kept on one line: the variant of a call whose arguments break
    /// one per line is chosen only when each argument then fits on its line
    /// whole — rustfmt breaks the outer of two nested calls when the inner
    /// fits on a line of its own, and lets the inner overflow otherwise.
    AllLinesFlat,
    /// Its first line, with the variant laid out broken — measured, and
    /// then printed, in break mode, its groups deciding for themselves: for
    /// content that ends in brackets of its own, where `let x = if (c) {`
    /// is what must fit and the branches are the braces' business. Unlike
    /// a group forced to break, it forces nothing on what encloses it.
    Broken,
}

impl Measure {
    /// The mode a [`Docs::best_fitting`] variant with this measure is
    /// measured and printed in when it is not the most expanded one.
    pub(crate) fn mode(self) -> Mode {
        match self {
            Self::FirstLine | Self::AllLines | Self::AllLinesFlat => Mode::Flat,
            Self::Broken => Mode::Break,
        }
    }
}

/// One layout of a [`Docs::best_fitting`], with how it is measured. A `Doc`
/// converts into one measured by its first line; [`Doc::all_lines`] makes
/// the other.
#[derive(Debug, Clone, Copy)]
pub struct Variant<'a> {
    pub doc: Doc<'a>,
    pub measure: Measure,
}

impl<'a> From<Doc<'a>> for Variant<'a> {
    fn from(doc: Doc<'a>) -> Self {
        Self {
            doc,
            measure: Measure::FirstLine,
        }
    }
}

/// A document, or any part of one.
///
/// A `Doc` is a reference: copying one shares it, so the same part may appear
/// in several places (which is how [`Docs::best_fitting`] variants share
/// their common pieces), and there is nothing to keep consistent between a
/// document and a store it lives in. Leaves with no payload are constants.
#[derive(Debug, Clone, Copy)]
#[must_use = "a document prints nothing until it is part of the one that is printed"]
pub struct Doc<'a>(&'a Node<'a>);

/// A piece of verbatim text: lines that move with the node, or lines kept
/// exactly as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerbatimPiece<'a> {
    Moved(&'a str),
    Exact(&'a str),
}

impl<'a> VerbatimPiece<'a> {
    pub(crate) fn text(self) -> &'a str {
        match self {
            Self::Moved(text) | Self::Exact(text) => text,
        }
    }
}

/// A node. The variants whose subtree may or may not contain a forced break
/// record that in `forced`, fixed when the node is created since its children
/// exist before it does; for every other variant the kind decides. A forced
/// break is a hard or empty line, an `EXPAND_PARENT`, a text with a newline
/// in it, a trailing line, or an expanded group; a group with one inside
/// always prints broken. An `if_break` carries one only through the arm that
/// prints when its group is flat: a break in the other arm prints once that
/// group is broken anyway, so it forces nothing.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Node<'a> {
    /// A run of text. With a newline in it, the later lines are printed as
    /// written, at their own columns; `width` is the first line's.
    Text {
        text: &'a str,
        width: u32,
        forced: bool,
    },
    /// Source text with a newline in it whose later lines move with the
    /// node: each is indented by the difference between the indentation of
    /// the line the node is printed on and `source_indent`, the indentation
    /// of the line the text started on in the source. Code (`forced`)
    /// breaks the enclosing groups; a literal does not, and is measured by
    /// its first line — but it [spans lines](Doc::spans_lines), which a list
    /// it is not the last item of must know.
    Verbatim {
        /// The text in pieces, each moved with the node or kept exactly: a
        /// string literal or template inside moved code keeps its lines.
        pieces: &'a [VerbatimPiece<'a>],
        width: u32,
        source_indent: u32,
        forced: bool,
    },
    Space,
    Line(LineMode),
    /// `content` after a line that is a space when flat and, when broken, a
    /// newline — unless the line so far ends in a closing bracket and
    /// reaches no further than the column the newline would indent to (a
    /// closing delimiter's line, `})`) and the content's first line fits on
    /// it: then a space, and the content continues that line at its own
    /// level, rustfmt's orphan rule, which keeps `}) + x` together rather
    /// than leaving the closing line on its own, and lays `x` out as part of
    /// that line.
    Snuggle {
        content: Doc<'a>,
        forced: bool,
    },
    Concat {
        children: &'a [Doc<'a>],
        forced: bool,
    },
    Group {
        content: Doc<'a>,
        forced: bool,
        /// Whether an `if_group_breaks` may refer to it, so the printer has
        /// to remember its layout.
        named: bool,
    },
    Indent {
        content: Doc<'a>,
        forced: bool,
    },
    /// The content is one item of a list — a statement, a member, an
    /// argument, a top-level item (see [`Docs::scope`]): a comment carried
    /// out of it to a line of its own ([`Docs::closing_line`]) takes the
    /// item's level, not the level the comment was emitted at.
    Scope {
        content: Doc<'a>,
        forced: bool,
    },
    /// The content, flat only when it is no wider than `max_width` columns,
    /// however much room the line has: the group deciding the layout breaks
    /// otherwise. rustfmt's small heuristics: a call's arguments wider than
    /// 60% of the line go one per line even when they would fit beside the
    /// call. Transparent in a broken layout.
    Limited {
        content: Doc<'a>,
        max_width: u32,
        forced: bool,
    },
    IfBreak {
        group: Option<GroupId<'a>>,
        breaks: Doc<'a>,
        flat: Doc<'a>,
        forced: bool,
    },
    LineSuffix {
        text: &'a str,
        width: u32,
        fit: SuffixWidth,
        forced: bool,
    },
    /// Text for a line of its own after the current line ends: a comment
    /// that follows the last item of a block on its own line is printed
    /// after whatever the enclosing nodes still put on the item's line. The
    /// line break it carries makes it a forced break when `forced`: a
    /// comment printed where it was written breaks the group around it so
    /// that nothing slips in before it; one a type suffix carried past itself
    /// (see [`Docs::closing_line`]) has given up its place and breaks nothing.
    TrailingLine {
        text: &'a str,
        blank_before: bool,
        forced: bool,
    },
    /// Text for the end of the current line when no suffix is pending there
    /// yet, else for a line of its own after it (see
    /// [`Docs::closing_suffix`]). Never a forced break: a comment carried
    /// past the construct it was written in has given up its place.
    ClosingSuffix {
        text: &'a str,
    },
    LineSuffixBoundary,
    ExpandParent,
    /// Two or more layouts, most flat first and most expanded last; the
    /// last is printed when no other fits. A forced break inside a variant
    /// is resolved when that variant is chosen, not by breaking what encloses
    /// it, so this never counts as forced. [`Docs::best_fitting`] drops a
    /// most flat variant that contains one and puts an `EXPAND_PARENT`
    /// before the choice instead, since every layout of content that is
    /// sure to break is multi-line; [`Docs::layouts`] keeps such a variant
    /// for a forced tail every layout shares, and its caller does the same.
    BestFitting {
        variants: &'a nunny::Slice<Variant<'a>>,
    },
}

// Four words on a 64-bit target, 32 bytes anywhere: the payloads are at
// most two words plus two `u32`s or a word, a `u32` and two flags, with the
// tag in the padding.
const _: () = assert!(size_of::<Node<'_>>() <= 32);

/// Whether a line suffix's columns count in the fits-check of the groups
/// before it on the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuffixWidth {
    /// They count: a long trailing comment breaks the construct before it.
    Reserved,
    /// They do not: the comment may run past the line width.
    Ignored,
}

/// A group that [`Docs::if_group_breaks`] elsewhere can refer to. Its
/// identity is the group node itself, so only [`Docs::group_with_id`] can
/// make one and it can never name anything but a group. A shared group
/// printed in several places is one node: an `if_group_breaks` sees the
/// layout of whichever print came last.
#[derive(Debug, Clone, Copy)]
pub struct GroupId<'a>(&'a Node<'a>);

impl<'a> GroupId<'a> {
    /// The id of a named group node, for the printer to key its decisions
    /// by.
    pub(crate) fn of(group: Doc<'a>) -> Self {
        debug_assert!(matches!(group.node(), Node::Group { named: true, .. }));
        Self(group.0)
    }

    /// Whether the group always prints broken.
    pub(crate) fn is_forced(self) -> bool {
        matches!(self.0, Node::Group { forced: true, .. })
    }
}

impl PartialEq for GroupId<'_> {
    fn eq(&self, other: &Self) -> bool {
        ptr::eq(self.0, other.0)
    }
}

impl Eq for GroupId<'_> {}

impl Hash for GroupId<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        ptr::hash(self.0, state);
    }
}

impl<'a> Doc<'a> {
    pub(crate) fn node(self) -> Node<'a> {
        *self.0
    }

    /// The node's address: its identity, for a measurement memo. Two
    /// handles to one allocation measure the same.
    pub(crate) fn addr(self) -> usize {
        std::ptr::from_ref(self.0) as usize
    }

    /// Whether the node is sure to print a line break: [`Self::forced`],
    /// looking into a `best_fitting` as well — at the variant it prints when
    /// flat, which is a broken layout once every flatter one is ruled out
    /// (a call hugging a multi-line lambda). What a layout decision asks
    /// before putting such a node on one line with something after it. An
    /// `if_break` is sure only when both its arms are, or the named group it
    /// follows is sure to break and so is the arm that prints then.
    pub(crate) fn will_break(self) -> bool {
        match self.node() {
            Node::BestFitting { variants } => variants.first().doc.will_break(),
            Node::Concat { children, .. } => children.iter().any(|child| child.will_break()),
            Node::Group { content, .. }
            | Node::Indent { content, .. }
            | Node::Scope { content, .. }
            | Node::Snuggle { content, .. }
            | Node::Limited { content, .. } => self.forced() || content.will_break(),
            Node::IfBreak {
                group,
                breaks,
                flat,
                ..
            } => {
                breaks.will_break() && (flat.will_break() || group.is_some_and(GroupId::is_forced))
            }
            _ => self.forced(),
        }
    }

    /// Whether this subtree contains a forced break.
    pub(crate) fn forced(self) -> bool {
        match self.node() {
            Node::Space
            | Node::Line(LineMode::Soft | LineMode::SoftOrSpace | LineMode::Hang)
            | Node::LineSuffixBoundary
            | Node::ClosingSuffix { .. }
            | Node::BestFitting { .. } => false,
            Node::Line(LineMode::Hard | LineMode::Empty) | Node::ExpandParent => true,
            Node::TrailingLine { forced, .. } => forced,
            Node::Text { forced, .. }
            | Node::Verbatim { forced, .. }
            | Node::Concat { forced, .. }
            | Node::Group { forced, .. }
            | Node::Indent { forced, .. }
            | Node::Scope { forced, .. }
            | Node::Snuggle { forced, .. }
            | Node::Limited { forced, .. }
            | Node::IfBreak { forced, .. }
            | Node::LineSuffix { forced, .. } => forced,
        }
    }

    /// Whether the node is sure to take more than one line: sure to print a
    /// line break (`will_break`), or a multi-line literal anywhere in it. A literal is an atom to the
    /// group around it — `f(`…`)` stays on the line it starts on — but the
    /// item after it in a list would land on its closing line, so a list
    /// breaks when any item but the last spans lines, as an operator chain
    /// and a run of attributes do.
    pub fn spans_lines(self) -> bool {
        match self.node() {
            Node::Verbatim { .. } => true,
            Node::BestFitting { variants } => variants.first().doc.spans_lines(),
            Node::Concat { children, .. } => children.iter().any(|child| child.spans_lines()),
            Node::Group { content, .. }
            | Node::Indent { content, .. }
            | Node::Scope { content, .. }
            | Node::Snuggle { content, .. }
            | Node::Limited { content, .. } => self.forced() || content.spans_lines(),
            Node::IfBreak { breaks, flat, .. } => {
                self.will_break() || (breaks.spans_lines() && flat.spans_lines())
            }
            _ => self.forced(),
        }
    }

    /// Lay the document out within `options.line_width` columns.
    pub fn print(self, options: FormatOptions) -> String {
        printer::print(self, options)
    }

    /// This document as a [`Docs::best_fitting`] variant chosen only when
    /// every line of it fits ([`Measure::AllLines`]).
    pub fn all_lines(self) -> Variant<'a> {
        Variant {
            doc: self,
            measure: Measure::AllLines,
        }
    }

    /// This document as a [`Docs::best_fitting`] variant measured and
    /// printed broken, chosen when its first line fits ([`Measure::Broken`]).
    pub fn broken(self) -> Variant<'a> {
        Variant {
            doc: self,
            measure: Measure::Broken,
        }
    }

    /// This document as a [`Docs::best_fitting`] variant chosen only when
    /// every line of it fits with nothing inside it broken that is not
    /// forced to be ([`Measure::AllLinesFlat`]).
    pub fn all_lines_flat(self) -> Variant<'a> {
        Variant {
            doc: self,
            measure: Measure::AllLinesFlat,
        }
    }
}

/// An S-expression rendering, for tests and debugging. Groups that an
/// `if_group_breaks` refers to are numbered in order of appearance.
impl fmt::Display for Doc<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut referenced = Vec::new();
        collect_referenced_groups(*self, &mut referenced);
        let mut display = Display {
            referenced,
            numbered: Vec::new(),
            out: String::new(),
        };
        display.doc(*self);
        f.write_str(&display.out)
    }
}

impl Doc<'static> {
    // Each `&Node::..` in a constant is promoted to a place in the binary: a
    // leaf with no payload is never allocated.

    /// The empty document.
    pub const NIL: Self = Doc(&Node::Concat {
        children: &[],
        forced: false,
    });
    pub const SPACE: Self = Doc(&Node::Space);
    /// Nothing when the enclosing group is flat, a newline when it breaks.
    pub const SOFT_LINE: Self = Doc(&Node::Line(LineMode::Soft));
    /// A space when the enclosing group is flat, a newline when it breaks.
    pub const SOFT_LINE_OR_SPACE: Self = Doc(&Node::Line(LineMode::SoftOrSpace));
    /// A space, or a newline when the line being written started deeper
    /// than this node's indent — a continuation line: the `{` after a
    /// header stays on `) {` and `} {`, never on `    | B {`.
    pub const HANG_LINE: Self = Doc(&Node::Line(LineMode::Hang));
    /// Always a newline; the enclosing group breaks. On a line with nothing
    /// on it yet it only sets the next line's indent, so two in a row leave
    /// no blank line.
    pub const HARD_LINE: Self = Doc(&Node::Line(LineMode::Hard));
    /// Always a newline and exactly one blank line, however many in a row;
    /// the enclosing group breaks.
    pub const EMPTY_LINE: Self = Doc(&Node::Line(LineMode::Empty));
    /// Flush any pending line suffix before continuing on this line.
    pub const LINE_SUFFIX_BOUNDARY: Self = Doc(&Node::LineSuffixBoundary);
    /// Force the enclosing group to break.
    pub const EXPAND_PARENT: Self = Doc(&Node::ExpandParent);
}

/// Where one document's nodes are allocated.
///
/// A `Docs` is a copyable handle; every builder takes it by value and returns
/// a [`Doc`], so a rule that builds a document is a plain function of its
/// inputs. Text is borrowed: a token's text is a slice of the file, never a
/// copy; a string a rule computes is copied into the arena once by
/// [`Self::copy_text`].
#[derive(Debug, Clone, Copy)]
pub struct Docs<'a> {
    bump: &'a Bump,
}

impl<'a> Docs<'a> {
    pub fn new(bump: &'a Bump) -> Self {
        Self { bump }
    }

    fn alloc(self, node: Node<'a>) -> Doc<'a> {
        Doc(self.bump.alloc(node))
    }

    fn slice_of<T>(self, items: impl IntoIterator<Item = T>) -> &'a [T] {
        bumpalo::collections::Vec::from_iter_in(items, self.bump).into_bump_slice()
    }

    fn slice(self, items: impl IntoIterator<Item = Doc<'a>>) -> &'a [Doc<'a>] {
        bumpalo::collections::Vec::from_iter_in(items, self.bump).into_bump_slice()
    }

    /// A run of text: a token of the file, or a literal. One with a newline
    /// in it (a multi-line string, a block comment) is printed as written,
    /// its later lines at their own columns, and counts as a forced break.
    pub fn text(self, text: &'a str) -> Doc<'a> {
        let (first, rest) = text.split_once('\n').unzip();
        self.alloc(Node::Text {
            text,
            width: display_width(first.unwrap_or(text)),
            forced: rest.is_some(),
        })
    }

    /// [`Self::text`] for a string the rule computed: copied into the arena.
    pub fn copy_text(self, text: &str) -> Doc<'a> {
        self.text(self.bump.alloc_str(text))
    }

    /// A multi-line literal laid out by the rule — a backtick string —
    /// copied in: [`Self::verbatim`] text relative to the line it starts on,
    /// that is also an atom to the layout around it: it does not force the
    /// enclosing groups to break and is measured by its first line, so
    /// `f(`…`)` hugs it, as a long string is hugged. It does
    /// [span lines](Doc::spans_lines), which a list it is not the last item
    /// of breaks for. Text with no newline is plain text.
    pub fn literal(self, text: &str) -> Doc<'a> {
        let text = self.bump.alloc_str(text);
        match text.split_once('\n') {
            None => self.text(text),
            Some((first, _)) => self.alloc(Node::Verbatim {
                pieces: self.slice_of([VerbatimPiece::Moved(text)]),
                width: display_width(first),
                source_indent: 0,
                forced: false,
            }),
        }
    }

    /// Source text printed as written, except that its later lines move
    /// with the node: each keeps its indentation relative to
    /// `source_indent`, the indentation of the line the text starts on in
    /// the source, measured from the indentation of the line the node is
    /// printed on. For a node printed where it was, nothing changes; for one
    /// printed deeper or shallower, its lines follow. Text with no newline is
    /// plain text.
    pub fn verbatim(self, text: &'a str, source_indent: u32) -> Doc<'a> {
        self.verbatim_pieces([VerbatimPiece::Moved(text)], source_indent)
    }

    /// [`Self::verbatim`] of text in pieces: the moved pieces' lines are
    /// re-indented, the exact pieces' (a string literal, a template) kept as
    /// written — all as one node, relative to the one line it starts on.
    pub fn verbatim_pieces(
        self,
        pieces: impl IntoIterator<Item = VerbatimPiece<'a>>,
        source_indent: u32,
    ) -> Doc<'a> {
        let pieces = self.slice_of(pieces);
        let before_newline: u32 = pieces
            .iter()
            .map(|piece| piece.text())
            .take_while(|text| !text.contains('\n'))
            .map(display_width)
            .sum();
        let first_with_newline = pieces
            .iter()
            .map(|piece| piece.text())
            .find(|text| text.contains('\n'));
        match first_with_newline {
            None => match pieces {
                [only] => self.text(only.text()),
                pieces => self.concat(pieces.iter().map(|piece| self.text(piece.text()))),
            },
            Some(text) => {
                let first = text.split_once('\n').map_or(text, |(first, _)| first);
                self.alloc(Node::Verbatim {
                    pieces,
                    width: before_newline.saturating_add(display_width(first)),
                    source_indent,
                    forced: true,
                })
            }
        }
    }

    /// The items in sequence.
    pub fn concat(self, items: impl IntoIterator<Item = Doc<'a>>) -> Doc<'a> {
        let children = self.slice(items);
        self.alloc(Node::Concat {
            children,
            forced: children.iter().any(|child| child.forced()),
        })
    }

    /// A layout unit: flat if it fits on the rest of the line, else broken.
    pub fn group(self, content: Doc<'a>) -> Doc<'a> {
        self.alloc(Node::Group {
            content,
            forced: content.forced(),
            named: false,
        })
    }

    /// A group that [`Self::if_group_breaks`] elsewhere can refer to.
    pub fn group_with_id(self, content: Doc<'a>) -> (Doc<'a>, GroupId<'a>) {
        let group = self.alloc(Node::Group {
            content,
            forced: content.forced(),
            named: true,
        });
        (group, GroupId::of(group))
    }

    /// A group that always breaks.
    pub fn expanded_group(self, content: Doc<'a>) -> Doc<'a> {
        self.alloc(Node::Group {
            content,
            forced: true,
            named: false,
        })
    }

    pub fn indent(self, content: Doc<'a>) -> Doc<'a> {
        self.alloc(Node::Indent {
            content,
            forced: content.forced(),
        })
    }

    /// `content` on the line before it when that line is only closing
    /// delimiters and the content's first line fits there, else on a line
    /// of its own; a space before it when flat (rustfmt's orphan rule).
    pub fn snuggle(self, content: Doc<'a>) -> Doc<'a> {
        self.alloc(Node::Snuggle {
            content,
            forced: content.forced(),
        })
    }

    /// `content`, one item of a list: a statement, a member, an argument, a
    /// top-level item. Nothing in the layout changes; a comment carried out
    /// of the item to a line of its own ([`Self::closing_line`]) is printed
    /// at the level of the line the item started on (or deeper, when the
    /// line break that ends the line is), where a comment on a line of its
    /// own between items reads as one of theirs.
    pub fn scope(self, content: Doc<'a>) -> Doc<'a> {
        self.alloc(Node::Scope {
            content,
            forced: content.forced(),
        })
    }

    /// `content`, flat only when it is no wider than `max_width` columns;
    /// the group deciding the layout breaks otherwise, however much room the
    /// line has — rustfmt's small heuristics. Nothing changes once that
    /// group is broken.
    pub fn limited(self, content: Doc<'a>, max_width: u32) -> Doc<'a> {
        self.alloc(Node::Limited {
            content,
            max_width,
            forced: content.forced(),
        })
    }

    /// `breaks` when the enclosing group is broken, `flat` when it is flat.
    /// A forced break in `flat` forces the group, which then prints
    /// `breaks`; one in `breaks` alone forces nothing.
    pub fn if_break(self, breaks: Doc<'a>, flat: Doc<'a>) -> Doc<'a> {
        self.alloc(Node::IfBreak {
            group: None,
            breaks,
            flat,
            forced: flat.forced(),
        })
    }

    /// `breaks` when the named group is broken, `flat` when it is flat. A
    /// group that has not been printed yet counts as flat, so this is sound
    /// only after the group it names: every use refers back to a group
    /// printed earlier in the document. This forces the enclosing group when
    /// `flat` does, or when the named group is sure to break and `breaks`
    /// does: a `{` on a line of its own after a broken signature forces
    /// nothing while the signature may still fit.
    pub fn if_group_breaks(self, group: GroupId<'a>, breaks: Doc<'a>, flat: Doc<'a>) -> Doc<'a> {
        self.alloc(Node::IfBreak {
            group: Some(group),
            breaks,
            flat,
            forced: flat.forced() || (group.is_forced() && breaks.forced()),
        })
    }

    /// Text deferred to the end of the current line, past whatever the
    /// enclosing nodes print after this point on it: a trailing `// comment`,
    /// which must follow the `,` or `)` its node is printed with. A suffix
    /// carries its own leading space; a space before it on the line is
    /// trimmed like any other trailing space. Its later lines, if any, keep
    /// their own columns, as with a multi-line [`Self::text`].
    pub fn line_suffix(self, text: &'a str, fit: SuffixWidth) -> Doc<'a> {
        self.alloc(Node::LineSuffix {
            text,
            width: display_width(text),
            fit,
            forced: text.contains('\n'),
        })
    }

    /// Text printed on a line of its own after the current line ends, at the
    /// indent this node is printed at, past whatever the enclosing nodes
    /// print after this point on that line: a comment on its own line after
    /// the last statement of a block, which must follow the `;` or `,` its
    /// node is printed with. With `blank_before`, one blank line separates it
    /// from the line it follows. Its later lines, if any, keep their own
    /// columns, as with a multi-line [`Self::text`].
    pub fn trailing_line(self, text: &'a str, blank_before: bool) -> Doc<'a> {
        self.alloc(Node::TrailingLine {
            text,
            blank_before,
            forced: true,
        })
    }

    /// [`Self::trailing_line`] that breaks no group and takes the indent of
    /// the line break that ends the line it follows rather than this node's:
    /// for a comment printed past the construct it was written in, whose
    /// place in the layout is given up already, and which belongs to what
    /// that construct is part of — the statement's block, the list.
    pub fn closing_line(self, text: &'a str) -> Doc<'a> {
        self.alloc(Node::TrailingLine {
            text,
            blank_before: false,
            forced: false,
        })
    }

    /// A line comment at the end of the current line, as a
    /// [`Self::line_suffix`] with its leading space — unless a tail is
    /// pending there already (a suffix, two line comments on one line
    /// reading as one, or a tail line, which it must not overtake), in which
    /// case on a line of its own after it, as a [`Self::closing_line`].
    /// Breaks no group either way: the comment layer adds an
    /// [`EXPAND_PARENT`](Doc::EXPAND_PARENT) for a comment printed where it
    /// was written, and none for one carried past its construct.
    pub fn closing_suffix(self, text: &'a str) -> Doc<'a> {
        self.alloc(Node::ClosingSuffix { text })
    }

    /// Ordered layouts, most flat first and most expanded last: the first
    /// that fits is printed, else the last. A variant fits when its first
    /// line does, with what follows it on that line, or — for one marked
    /// [`Doc::all_lines`] — when every line of it does. A layout choice
    /// needs at least two variants, which the array length enforces at
    /// compile time.
    ///
    /// A most flat variant with a forced break in it could never print flat,
    /// so it is left out — as a group with one is never flat — and the
    /// choice may collapse to one layout. The content is then sure to take
    /// more than one line whichever layout is chosen, and the groups around
    /// it must know: an [`EXPAND_PARENT`](Doc::EXPAND_PARENT) goes before
    /// the choice, as Prettier puts a `breakParent` before a conditional
    /// group whose one-line form will break. An object literal holding a
    /// call with a multi-line lambda in it breaks one field per line, and an
    /// `if` condition holding one puts the condition on lines of its own.
    pub fn best_fitting<V: Into<Variant<'a>>, const N: usize>(self, variants: [V; N]) -> Doc<'a> {
        const {
            assert!(N >= 2, "best_fitting needs two or more variants");
        }
        let variants = variants.map(Into::into);
        let (kept, sure_to_break) = match &variants[..] {
            [flat, ..] if !flat.doc.forced() => (&variants[..], false),
            [_, rest @ ..] => (rest, true),
            [] => unreachable!("two or more variants"),
        };
        // One variant left is printed as the most expanded one would be,
        // broken, whatever layout surrounds it: it was the fallback.
        let choice = match kept {
            [only] => self.expanded_group(only.doc),
            kept => self.choice(kept),
        };
        if sure_to_break {
            self.concat([Doc::EXPAND_PARENT, choice])
        } else {
            choice
        }
    }

    /// [`Self::best_fitting`] for variants whose most flat one may hold a
    /// forced break that every variant shares at its end — a `catch` with
    /// its arms after a member chain: the chain prints on one line and the
    /// arms break on lines of their own, as hard lines do in any layout.
    /// Nothing is dropped and nothing forces the parent; the caller has made
    /// sure that what comes before the shared tail prints flat and puts an
    /// [`EXPAND_PARENT`](Doc::EXPAND_PARENT) before the choice itself when
    /// the content is sure to break.
    pub fn layouts<V: Into<Variant<'a>>, const N: usize>(self, variants: [V; N]) -> Doc<'a> {
        const {
            assert!(N >= 2, "layouts needs two or more variants");
        }
        self.choice(&variants.map(Into::into))
    }

    fn choice(self, variants: &[Variant<'a>]) -> Doc<'a> {
        self.alloc(Node::BestFitting {
            variants: nunny::Slice::new(self.slice_of(variants.iter().copied()))
                .unwrap_or_else(|| unreachable!("two or more variants")),
        })
    }
}

fn collect_referenced_groups<'a>(doc: Doc<'a>, referenced: &mut Vec<GroupId<'a>>) {
    match doc.node() {
        Node::Text { .. }
        | Node::Verbatim { .. }
        | Node::Space
        | Node::Line(_)
        | Node::LineSuffix { .. }
        | Node::TrailingLine { .. }
        | Node::ClosingSuffix { .. }
        | Node::LineSuffixBoundary
        | Node::ExpandParent => {}
        Node::Concat { children, .. } => {
            for &child in children {
                collect_referenced_groups(child, referenced);
            }
        }
        Node::BestFitting { variants } => {
            for variant in variants {
                collect_referenced_groups(variant.doc, referenced);
            }
        }
        Node::Group { content, .. }
        | Node::Indent { content, .. }
        | Node::Scope { content, .. }
        | Node::Snuggle { content, .. }
        | Node::Limited { content, .. } => {
            collect_referenced_groups(content, referenced);
        }
        Node::IfBreak {
            group,
            breaks,
            flat,
            ..
        } => {
            if let Some(group) = group
                && !referenced.contains(&group)
            {
                referenced.push(group);
            }
            collect_referenced_groups(breaks, referenced);
            collect_referenced_groups(flat, referenced);
        }
    }
}

struct Display<'a> {
    referenced: Vec<GroupId<'a>>,
    /// Referenced groups in order of first appearance; the index is the name.
    numbered: Vec<GroupId<'a>>,
    out: String,
}

impl<'a> Display<'a> {
    fn number(&mut self, group: GroupId<'a>) {
        let number = self
            .numbered
            .iter()
            .position(|&seen| seen == group)
            .unwrap_or_else(|| {
                self.numbered.push(group);
                self.numbered.len() - 1
            });
        write!(self.out, " #{}", number + 1).unwrap_or_else(|_| unreachable!("String never fails"));
    }

    fn doc(&mut self, doc: Doc<'a>) {
        let never = |_| unreachable!("String never fails");
        match doc.node() {
            Node::Text { text, .. } => write!(self.out, "{text:?}").unwrap_or_else(never),
            Node::Verbatim {
                pieces,
                source_indent,
                forced,
                ..
            } => {
                let kind = if forced { "verbatim" } else { "literal" };
                write!(self.out, "({kind} {source_indent}").unwrap_or_else(never);
                for piece in pieces {
                    match piece {
                        VerbatimPiece::Moved(text) => write!(self.out, " {text:?}"),
                        VerbatimPiece::Exact(text) => write!(self.out, " (exact {text:?})"),
                    }
                    .unwrap_or_else(never);
                }
                self.out.push(')');
            }
            Node::Space => self.out.push_str("space"),
            Node::Line(LineMode::Soft) => self.out.push_str("softline"),
            Node::Line(LineMode::SoftOrSpace) => self.out.push_str("line"),
            Node::Line(LineMode::Hang) => self.out.push_str("hang"),
            Node::Snuggle { content, .. } => {
                self.out.push_str("(snuggle ");
                self.doc(content);
                self.out.push(')');
            }
            Node::Line(LineMode::Hard) => self.out.push_str("hardline"),
            Node::Line(LineMode::Empty) => self.out.push_str("emptyline"),
            Node::Concat { children, .. } => {
                self.out.push_str("(concat");
                for &child in children {
                    self.out.push(' ');
                    self.doc(child);
                }
                self.out.push(')');
            }
            Node::Group {
                content,
                forced,
                named,
            } => {
                self.out.push_str("(group");
                if named {
                    let id = GroupId::of(doc);
                    if self.referenced.contains(&id) {
                        self.number(id);
                    }
                }
                // Expanded by the rule, not by a forced break inside.
                if forced && !content.forced() {
                    self.out.push_str(" expanded");
                }
                self.out.push(' ');
                self.doc(content);
                self.out.push(')');
            }
            Node::Indent { content, .. } => {
                self.out.push_str("(indent ");
                self.doc(content);
                self.out.push(')');
            }
            Node::Scope { content, .. } => {
                self.out.push_str("(scope ");
                self.doc(content);
                self.out.push(')');
            }
            Node::Limited {
                content, max_width, ..
            } => {
                write!(self.out, "(limited {max_width} ").unwrap_or_else(never);
                self.doc(content);
                self.out.push(')');
            }
            Node::IfBreak {
                group,
                breaks,
                flat,
                ..
            } => {
                self.out.push_str("(if_break");
                if let Some(group) = group {
                    self.number(group);
                }
                self.out.push(' ');
                self.doc(breaks);
                self.out.push(' ');
                self.doc(flat);
                self.out.push(')');
            }
            Node::LineSuffix { text, fit, .. } => {
                let fit = match fit {
                    SuffixWidth::Reserved => " reserved",
                    SuffixWidth::Ignored => "",
                };
                write!(self.out, "(line_suffix{fit} {text:?})").unwrap_or_else(never);
            }
            Node::TrailingLine {
                text,
                blank_before,
                forced,
            } => {
                let blank = if blank_before { " blank" } else { "" };
                let kind = if forced {
                    "trailing_line"
                } else {
                    "closing_line"
                };
                write!(self.out, "({kind}{blank} {text:?})").unwrap_or_else(never);
            }
            Node::ClosingSuffix { text } => {
                write!(self.out, "(closing_suffix {text:?})").unwrap_or_else(never);
            }
            Node::LineSuffixBoundary => self.out.push_str("line_suffix_boundary"),
            Node::ExpandParent => self.out.push_str("expand_parent"),
            Node::BestFitting { variants } => {
                self.out.push_str("(best_fitting");
                for variant in variants {
                    self.out.push(' ');
                    let label = match variant.measure {
                        Measure::FirstLine => None,
                        Measure::AllLines => Some("(all_lines "),
                        Measure::AllLinesFlat => Some("(all_lines_flat "),
                        Measure::Broken => Some("(broken "),
                    };
                    match label {
                        None => self.doc(variant.doc),
                        Some(label) => {
                            self.out.push_str(label);
                            self.doc(variant.doc);
                            self.out.push(')');
                        }
                    }
                }
                self.out.push(')');
            }
        }
    }
}

/// Display width in terminal columns of `text` — of its first line, when it
/// has lines of its own; the printer measures a multi-line text by that. A
/// control character (a tab inside a string, say) counts as one column: the
/// printer never emits tabs itself, and treating them as wide would only
/// make unrelated lines break early.
pub(crate) fn display_width(text: &str) -> u32 {
    u32::try_from(text.width()).unwrap_or_else(|_| unreachable!("a line is narrower than 2^32"))
}
