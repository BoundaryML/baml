//! Documents: immutable trees, allocated in a bump arena and shared by
//! reference.

use std::{
    fmt::{self, Write as _},
    hash::{Hash, Hasher},
    iter, ptr,
};

use bumpalo::Bump;
use unicode_width::UnicodeWidthStr;

use super::printer;
use crate::FormatOptions;

/// How a line node prints in each layout of its enclosing group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineMode {
    /// Nothing when flat, a newline when broken.
    Soft,
    /// A space when flat, a newline when broken.
    SoftOrSpace,
    /// Always a newline.
    Hard,
    /// Always a newline followed by exactly one blank line.
    Empty,
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

/// A node. The variants whose subtree may or may not contain a forced break
/// record that in `forced`, fixed when the node is created since its children
/// exist before it does; for every other variant the kind decides. A forced
/// break is a hard or empty line, an `EXPAND_PARENT`, a text with a newline
/// in it, a trailing line, or an expanded group; a group with one inside
/// always prints broken.
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
    /// node: each is indented by the difference between the indent the node
    /// is printed at and `source_indent`, the indentation of the line the
    /// text started on in the source. Always a forced break.
    Verbatim {
        text: &'a str,
        width: u32,
        source_indent: u32,
    },
    Space,
    Line(LineMode),
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
    /// line break it carries makes it a forced break.
    TrailingLine {
        text: &'a str,
        blank_before: bool,
    },
    LineSuffixBoundary,
    ExpandParent,
    /// Two or more layouts, most flat first and most expanded last: the
    /// shape itself holds the "two or more". A forced break inside a variant
    /// is resolved when that variant is chosen, not by breaking what encloses
    /// it, so this never counts as forced — and the most flat variant never
    /// contains one: [`Docs::best_fitting`], the only way to make one, drops
    /// such a variant and puts an `EXPAND_PARENT` before the choice instead,
    /// since every layout of content that is sure to break is multi-line.
    BestFitting {
        most_flat: Doc<'a>,
        /// The more expanded layouts, in order; the last is printed when no
        /// other fits.
        expansions: &'a nunny::Slice<Doc<'a>>,
    },
}

// Four words on a 64-bit target (and no more than that anywhere): the
// payloads are at most two words plus two `u32`s or a word, a `u32` and two
// flags, with the tag in the padding.
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

    /// Whether this subtree contains a forced break.
    pub(crate) fn forced(self) -> bool {
        match self.node() {
            Node::Space
            | Node::Line(LineMode::Soft | LineMode::SoftOrSpace)
            | Node::LineSuffixBoundary
            | Node::BestFitting { .. } => false,
            Node::Line(LineMode::Hard | LineMode::Empty)
            | Node::ExpandParent
            | Node::TrailingLine { .. }
            | Node::Verbatim { .. } => true,
            Node::Text { forced, .. }
            | Node::Concat { forced, .. }
            | Node::Group { forced, .. }
            | Node::Indent { forced, .. }
            | Node::IfBreak { forced, .. }
            | Node::LineSuffix { forced, .. } => forced,
        }
    }

    /// Lay the document out within `options.line_width` columns.
    pub fn print(self, options: FormatOptions) -> String {
        printer::print(self, options)
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

    /// Source text printed as written, except that its later lines move
    /// with the node: each keeps its indentation relative to
    /// `source_indent`, the indentation of the line the text starts on in
    /// the source, measured from the indent the node is printed at. For a
    /// node printed where it was, nothing changes; for one printed deeper or
    /// shallower, its lines follow. Text with no newline is plain text.
    pub fn verbatim(self, text: &'a str, source_indent: u32) -> Doc<'a> {
        match text.split_once('\n') {
            None => self.text(text),
            Some((first, _)) => self.alloc(Node::Verbatim {
                text,
                width: display_width(first),
                source_indent,
            }),
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

    /// `breaks` when the enclosing group is broken, `flat` when it is flat.
    pub fn if_break(self, breaks: Doc<'a>, flat: Doc<'a>) -> Doc<'a> {
        self.alloc(Node::IfBreak {
            group: None,
            breaks,
            flat,
            forced: breaks.forced() || flat.forced(),
        })
    }

    /// `breaks` when the named group is broken, `flat` when it is flat. A
    /// group that has not been printed yet counts as flat, so this is sound
    /// only after the group it names: every use refers back to a group
    /// printed earlier in the document.
    pub fn if_group_breaks(self, group: GroupId<'a>, breaks: Doc<'a>, flat: Doc<'a>) -> Doc<'a> {
        self.alloc(Node::IfBreak {
            group: Some(group),
            breaks,
            flat,
            forced: breaks.forced() || flat.forced(),
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
        self.alloc(Node::TrailingLine { text, blank_before })
    }

    /// Ordered layouts, most flat first and most expanded last: the first
    /// whose first line fits, with what follows it on that line, is printed,
    /// else the last. A layout choice needs at least two variants, which the
    /// array length enforces at compile time.
    ///
    /// A most flat variant with a forced break in it could never print flat,
    /// so it is left out — as a group with one is never flat — and the
    /// choice may collapse to one layout. The content is then sure to take
    /// more than one line whichever layout is chosen, and the groups around
    /// it must know: an [`EXPAND_PARENT`](Doc::EXPAND_PARENT) goes before
    /// the choice, as Prettier puts a `breakParent` before a conditional
    /// group whose one-line form will break.
    pub fn best_fitting<const N: usize>(self, variants: [Doc<'a>; N]) -> Doc<'a> {
        const {
            assert!(N >= 2, "best_fitting needs two or more variants");
        }
        let (kept, sure_to_break) = match &variants[..] {
            [flat, ..] if !flat.forced() => (&variants[..], false),
            [_, rest @ ..] => (rest, true),
            [] => unreachable!("two or more variants"),
        };
        let choice = match kept {
            [only] => *only,
            [most_flat, expansions @ ..] => {
                let expansions = nunny::Slice::new(self.slice(expansions.iter().copied()))
                    .unwrap_or_else(|| unreachable!("two or more variants, one dropped at most"));
                self.alloc(Node::BestFitting {
                    most_flat: *most_flat,
                    expansions,
                })
            }
            [] => unreachable!("two or more variants, one dropped at most"),
        };
        if sure_to_break {
            self.concat([Doc::EXPAND_PARENT, choice])
        } else {
            choice
        }
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
        | Node::LineSuffixBoundary
        | Node::ExpandParent => {}
        Node::Concat { children, .. } => {
            for &child in children {
                collect_referenced_groups(child, referenced);
            }
        }
        Node::BestFitting {
            most_flat,
            expansions,
        } => {
            for &variant in iter::once(&most_flat).chain(expansions.iter()) {
                collect_referenced_groups(variant, referenced);
            }
        }
        Node::Group { content, .. } | Node::Indent { content, .. } => {
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
                text,
                source_indent,
                ..
            } => write!(self.out, "(verbatim {source_indent} {text:?})").unwrap_or_else(never),
            Node::Space => self.out.push_str("space"),
            Node::Line(LineMode::Soft) => self.out.push_str("softline"),
            Node::Line(LineMode::SoftOrSpace) => self.out.push_str("line"),
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
            Node::TrailingLine { text, blank_before } => {
                let blank = if blank_before { " blank" } else { "" };
                write!(self.out, "(trailing_line{blank} {text:?})").unwrap_or_else(never);
            }
            Node::LineSuffixBoundary => self.out.push_str("line_suffix_boundary"),
            Node::ExpandParent => self.out.push_str("expand_parent"),
            Node::BestFitting {
                most_flat,
                expansions,
            } => {
                self.out.push_str("(best_fitting");
                for &variant in iter::once(&most_flat).chain(expansions.iter()) {
                    self.out.push(' ');
                    self.doc(variant);
                }
                self.out.push(')');
            }
        }
    }
}

/// Display width in terminal columns of text with no newline in it. A
/// control character (a tab inside a string, say) counts as one column: the
/// printer never emits tabs itself, and treating them as wide would only
/// make unrelated lines break early.
pub(crate) fn display_width(text: &str) -> u32 {
    u32::try_from(text.width()).unwrap_or_else(|_| unreachable!("a line is narrower than 2^32"))
}
