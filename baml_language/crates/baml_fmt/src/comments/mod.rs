//! Plain comments, each attached to the node it belongs to.
//!
//! A `//` or `/* */` comment is trivia: no node owns it, so the tree says
//! nothing about where it goes once the nodes around it are laid out afresh.
//! This module decides that once, for every comment of a file, from the
//! tokens and the tree alone; the rules read the decisions.
//!
//! # The rule
//!
//! A comment lies between two tokens that nodes own. The deepest node owning
//! both, itself or through its children, is the comment's *enclosing* node.
//! Each of the two tokens is either a token of the enclosing node itself or
//! a token of one of its children: the child before the comment is its
//! *preceding* child, the one after it its *following* child, and the
//! comment is attached to one of them, or — when neither of its neighbours
//! is a child's token — to the enclosing node, *dangling* after the token
//! before it. Which one depends on where the comment sits on its line:
//!
//! * on a line of its own: it leads the following child (`// c` above a
//!   statement), else trails the preceding one (`// c` after the last
//!   statement of a block, `// c` between a `///` line and the keyword it
//!   documents), else dangles (`// c` in an empty block);
//! * at the end of a line of code: after a separator it dangles after that
//!   separator, of the node owning it (`a, // c`, `a; // c`); else it
//!   trails the preceding child (`a // c`), else dangles after the token
//!   before it (`{ // c`, `function // c`, `= // c`), which a rule prints
//!   on that token's line;
//! * between code on one line (so a `/* */`): after a separator it dangles
//!   after that separator (`a, /* c */ b`); else it trails the preceding
//!   child (`a /* c */, b`), else leads the following one (`= /* c */ 2`),
//!   else dangles after the token before it (`map /* c */ {`).
//!
//! A comment never crosses a token of the enclosing node to reach a child
//! on the far side of it, with one exception: a comment on a line that ends
//! before the enclosing node's terminator (`a // c` then `;` or `,`), a
//! line comment that ends an item's line before the item's `,` or `;`, or a
//! comment on a line of its own before such a `,` or `;`, dangles after that
//! separator, which the rules print right after the node whatever it was
//! written like, so that re-reading the output attaches it the same way —
//! and so that the comments between an item and its separator stay in the
//! order they were written, whichever lines they were on.
//!
//! Nothing stands between a prefix operator and its operand: a `-` that
//! opens a line with anything after it continues the line before instead of
//! negating. So a comment written after the operator (`- /* c */ x`,
//! `- // c` then `x`) leads the whole expression — the whole run, when
//! prefix operators are nested (`-- /* c */ x`) — and the rule that places
//! the expression places it.
//!
//! The rules print a token of the source on a new line when a line comment
//! is pending on the current one ([`Formatter::token`](crate::format::Formatter::token)), so a comment is
//! never overtaken by a token it preceded; only the punctuation the
//! formatter owns — a separator, a bracket that cannot start a line — is
//! tucked in before it.
//!
//! Where a comment sits is judged against the code around it, so a run of
//! comments on one line is one unit; what separates a comment from its
//! neighbours, other comments included, is kept separately for printing.
//! That is the attachment Prettier and Biome default to, minus the step
//! over the enclosing node's tokens, for which they carry per-language
//! handlers. What a comment's placement means for the layout of the node it
//! is attached to is the rule's business; [`docs`] prints the comments
//! themselves.

use std::num::NonZeroU32;

use baml_syntax::{
    index::TreeIndex,
    nodes::{FileTree, UnaryExpr},
    token::{KindSet, TokenIdx, TokenKind, Tokens},
    tree::AnyNodeId,
};

pub mod docs;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommentKind {
    /// `// …`, to the end of its line.
    Line,
    /// `/* … */`, possibly spanning lines.
    Block,
}

/// What separates a comment from the nearest token that is not a blank on
/// one side of it — a comment counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gap {
    /// Nothing but the edge of the file.
    Edge,
    /// Spaces at most: the token is on the comment's line.
    Inline,
    /// This many line breaks, blank lines included: a blank line separates
    /// them when there are two or more.
    Lines(NonZeroU32),
}

impl Gap {
    fn of_newlines(newlines: u32) -> Self {
        NonZeroU32::new(newlines).map_or(Self::Inline, Self::Lines)
    }

    /// Whether a line ends between the two.
    pub fn is_line_break(self) -> bool {
        match self {
            Self::Edge | Self::Lines(_) => true,
            Self::Inline => false,
        }
    }

    /// Whether at least one blank line lies between the two.
    pub fn has_blank_line(self) -> bool {
        match self {
            Self::Lines(newlines) => newlines.get() >= 2,
            Self::Edge | Self::Inline => false,
        }
    }
}

/// Where a comment sits on its line, relative to the code around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Placement {
    /// No code before it on its line.
    OwnLine,
    /// Code before it on its line, none after.
    EndOfLine,
    /// Code on both sides of it on its line.
    SameLine,
}

impl Placement {
    /// The placement of a comment with a line break before it or not, and
    /// after it or not.
    fn of(break_before: bool, break_after: bool) -> Self {
        match (break_before, break_after) {
            (true, _) => Self::OwnLine,
            (false, true) => Self::EndOfLine,
            (false, false) => Self::SameLine,
        }
    }
}

/// How a comment relates to the node it is attached to. The order is the
/// order the comments of one node are printed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Position {
    /// Before the node.
    Leading,
    /// Inside the node, directly after the given token of its own, with no
    /// child beside the comment to attach to; `None` only for a comment at
    /// the start of a file that has nothing but comments.
    Dangling { after: Option<TokenIdx> },
    /// After the node.
    Trailing,
}

/// One comment of the file and the node it is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceComment {
    pub token: TokenIdx,
    pub kind: CommentKind,
    pub placement: Placement,
    /// What separates it from the token before it.
    pub before: Gap,
    /// What separates it from the token after it.
    pub after: Gap,
    pub node: AnyNodeId,
    pub position: Position,
}

impl SourceComment {
    /// The comment as written, without the blanks a line comment may run
    /// up to its line break with.
    pub fn text(self, tree: &FileTree) -> &str {
        tree.token_text(self.token).trim_end()
    }
}

/// Every plain comment of one file, attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comments {
    /// Each comment token exactly once, sorted by node, then position, then
    /// source order — so a node's comments are one run, in print order.
    comments: Box<[SourceComment]>,
}

impl Comments {
    /// Attach every comment of `tree`; `index` is `tree`'s.
    pub fn from_tree(tree: &FileTree, index: &TreeIndex) -> Self {
        debug_assert!(index.is_for(tree), "an index is read against its own tree");
        let mut comments = Sweep::new(tree, index).comments();
        comments.sort_by_key(|comment| (comment.node, comment.position, comment.token));
        Self {
            comments: comments.into_boxed_slice(),
        }
    }

    /// Every comment, grouped by node.
    pub fn all(&self) -> &[SourceComment] {
        &self.comments
    }

    /// Every comment attached to `node`, in print order: leading, dangling,
    /// trailing.
    pub fn of(&self, node: AnyNodeId) -> &[SourceComment] {
        let start = self.comments.partition_point(|c| c.node < node);
        let end = start + self.comments[start..].partition_point(|c| c.node == node);
        &self.comments[start..end]
    }

    /// The comments before `node`.
    pub fn leading(&self, node: AnyNodeId) -> &[SourceComment] {
        let of = self.of(node);
        &of[..of.partition_point(|c| c.position == Position::Leading)]
    }

    /// The comments inside `node` that no child of it carries, in source
    /// order; each knows the token of `node` it follows.
    pub fn dangling(&self, node: AnyNodeId) -> &[SourceComment] {
        let of = self.of(node);
        let start = of.partition_point(|c| c.position == Position::Leading);
        let end = of.partition_point(|c| c.position != Position::Trailing);
        &of[start..end]
    }

    /// The comments dangling in `node` directly after its token `token`.
    pub fn dangling_after(&self, node: AnyNodeId, token: TokenIdx) -> &[SourceComment] {
        let position = Position::Dangling { after: Some(token) };
        let of = self.of(node);
        let start = of.partition_point(|c| c.position < position);
        let end = start + of[start..].partition_point(|c| c.position == position);
        &of[start..end]
    }

    /// The comments after `node`.
    pub fn trailing(&self, node: AnyNodeId) -> &[SourceComment] {
        let of = self.of(node);
        &of[of.partition_point(|c| c.position != Position::Trailing)..]
    }

    /// What separates `node`, its leading comments included, from the token
    /// before it: what a list layout reads to keep a blank line between two
    /// items.
    pub fn gap_before_node(&self, tree: &FileTree, node: AnyNodeId) -> Gap {
        self.leading(node).first().map_or_else(
            || gap_before(tree.tokens(), tree.arenas().span(node).start()),
            |first| first.before,
        )
    }
}

/// Whether a token is a comment the formatter places: a trivia comment (see `comment_kind`).
pub fn is_comment(kind: TokenKind) -> bool {
    comment_kind(kind).is_some()
}

/// The kind of comment a token is, if it is one the formatter places: the
/// trivia comments. `///` and `//baml:` lines are syntax a declaration owns,
/// and a `//#` line is a statement or a function's title where it is allowed.
fn comment_kind(kind: TokenKind) -> Option<CommentKind> {
    match kind {
        TokenKind::LineComment => Some(CommentKind::Line),
        TokenKind::BlockComment | TokenKind::UnclosedBlockComment => Some(CommentKind::Block),
        TokenKind::Whitespace | TokenKind::Newline => None,
        kind => {
            debug_assert!(
                !kind.is_trivia(),
                "{kind:?} is trivia the formatter does not place"
            );
            None
        }
    }
}

/// What separates `token` from the previous token that is not a blank.
pub fn gap_before(tokens: &Tokens, token: TokenIdx) -> Gap {
    let mut newlines = 0;
    for earlier in (0..token.0).rev().map(TokenIdx) {
        match tokens.kind(earlier) {
            TokenKind::Whitespace => {}
            TokenKind::Newline => newlines += 1,
            _ => return Gap::of_newlines(newlines),
        }
    }
    Gap::Edge
}

/// What separates `token` from the next token that is not a blank.
pub fn gap_after(tokens: &Tokens, token: TokenIdx) -> Gap {
    let mut newlines = 0;
    for later in (token.0 + 1..len(tokens)).map(TokenIdx) {
        match tokens.kind(later) {
            TokenKind::Whitespace => {}
            TokenKind::Newline => newlines += 1,
            _ => return Gap::of_newlines(newlines),
        }
    }
    Gap::Edge
}

fn len(tokens: &Tokens) -> u32 {
    u32::try_from(tokens.len()).unwrap_or_else(|_| unreachable!("a file never has 2^32 tokens"))
}

/// The separators a list node owns between its items: a comment directly
/// after one belongs to the item before it.
const SEPARATORS: KindSet =
    KindSet::new(&[TokenKind::Comma, TokenKind::Semicolon, TokenKind::Pipe]);

/// The separators the rules print as text that tucks in before a pending
/// line comment (`a, // c`); a `|` is a token, which starts a new line then.
const TUCKED: KindSet = KindSet::new(&[TokenKind::Comma, TokenKind::Semicolon]);

/// A token a node owns, and that node.
#[derive(Debug, Clone, Copy)]
struct Owned {
    token: TokenIdx,
    owner: AnyNodeId,
}

/// A comment and the nearest owned token on each side of it, gathered in
/// two passes over the tokens so that a run of comments costs no more than
/// the tokens it spans.
#[derive(Debug, Clone, Copy)]
struct Found {
    token: TokenIdx,
    kind: CommentKind,
    prev: Option<Owned>,
    next: Option<Owned>,
}

/// The passes that attach a file's comments.
struct Sweep<'t> {
    tree: &'t FileTree,
    index: &'t TreeIndex,
    /// `newlines[i]`: the newline tokens among the first `i` tokens, so the
    /// line breaks between any two tokens are a subtraction.
    newlines: Vec<u32>,
}

impl<'t> Sweep<'t> {
    fn new(tree: &'t FileTree, index: &'t TreeIndex) -> Self {
        let tokens = tree.tokens();
        let mut newlines = Vec::with_capacity(tokens.len() + 1);
        newlines.push(0);
        for token in tokens.iter() {
            let count =
                newlines[token.index()] + u32::from(tokens.kind(token) == TokenKind::Newline);
            newlines.push(count);
        }
        Self {
            tree,
            index,
            newlines,
        }
    }

    fn owned(&self, token: TokenIdx) -> Option<Owned> {
        self.index
            .token_owner(token)
            .map(|owner| Owned { token, owner })
    }

    /// The newline tokens strictly between `a` and `b`.
    fn newlines_between(&self, a: TokenIdx, b: TokenIdx) -> u32 {
        self.newlines[b.index()] - self.newlines[a.index() + 1]
    }

    fn comments(&self) -> Vec<SourceComment> {
        let tokens = self.tree.tokens();
        let mut found = Vec::new();
        let mut prev = None;
        for token in tokens.iter() {
            // A comment inside a run of tokens a node covers without reading
            // them (a removed item's body, a jinja argument) is printed with
            // that run: it is owned, so it is no comment here.
            match (comment_kind(tokens.kind(token)), self.owned(token)) {
                (Some(kind), None) => found.push(Found {
                    token,
                    kind,
                    prev,
                    next: None,
                }),
                (_, Some(owned)) => prev = Some(owned),
                (None, None) => {}
            }
        }
        // Backwards, each token read once: the scan for a comment's next
        // owned token stops where the scan for the comment after it began.
        let mut next: Option<Owned> = None;
        let mut scanned_to = len(tokens);
        for comment in found.iter_mut().rev() {
            for token in (comment.token.0 + 1..scanned_to).rev().map(TokenIdx) {
                if let Some(owned) = self.owned(token) {
                    next = Some(owned);
                }
            }
            scanned_to = comment.token.0 + 1;
            comment.next = next;
        }
        found
            .into_iter()
            .map(|comment| self.attach(comment))
            .collect()
    }

    /// The node `comment` belongs to and how (see the module doc).
    fn attach(&self, comment: Found) -> SourceComment {
        let tokens = self.tree.tokens();
        let Found {
            token,
            kind,
            prev,
            next,
        } = comment;
        let enclosing = match (prev, next) {
            (Some(prev), Some(next)) => self.index.common_ancestor(prev.owner, next.owner),
            // Before the first token or after the last: outside every node
            // but the root.
            (None, _) | (_, None) => self.tree.root_id(),
        };
        let child = |owned: Owned| {
            (owned.owner != enclosing).then(|| self.child_of(enclosing, owned.owner))
        };
        let preceding = prev.and_then(child);
        let following = next.and_then(child);
        // A separator directly before the comment: the comment dangles
        // after it, of whichever node owns it — the item, when the item
        // owns its separator, else the list — so that it is printed right
        // after the separator, behind any comment the terminator rule below
        // moved past it.
        let after_separator = prev
            .filter(|prev| SEPARATORS.contains(tokens.kind(prev.token)))
            .map(|separator| {
                (
                    separator.owner,
                    Position::Dangling {
                        after: Some(separator.token),
                    },
                )
            });
        // The item before a separator the enclosing node owns.
        let before_separator = || {
            prev.filter(|prev| {
                prev.owner == enclosing && SEPARATORS.contains(tokens.kind(prev.token))
            })
            .and_then(|separator| self.owned_before(separator.token))
            .and_then(child)
        };
        let placement = Placement::of(
            prev.is_none_or(|prev| self.newlines_between(prev.token, token) > 0),
            next.is_none_or(|next| self.newlines_between(token, next.token) > 0),
        );
        // A comment on a line that ends before the enclosing node's
        // terminator — the `;` or `,` that is its last token — dangles
        // after that terminator: a rule prints the terminator and then the
        // comment, and re-reading that output attaches the comment the same
        // way, so the layout does not change between passes. So does a line
        // comment ending an item's line when the item's `,` or `;` follows
        // on a later one: the rules print those separators as text that
        // tucks in before a pending line comment, so the output has the
        // comment after the separator either way. A comment on the same
        // line as the terminator stays where it was written.
        let terminator = (placement != Placement::SameLine)
            .then(|| {
                next.filter(|next| {
                    let separator = tokens.kind(next.token);
                    let tucks_in = placement == Placement::EndOfLine
                        && kind == CommentKind::Line
                        && TUCKED.contains(separator);
                    next.owner == enclosing
                        && SEPARATORS.contains(separator)
                        && (TokenIdx(next.token.0 + 1) == self.tree.arenas().span(enclosing).end()
                            || tucks_in)
                })
            })
            .flatten();
        // A comment on a line of its own directly before an item's `,` or
        // `;` that the enclosing list owns: printed right after the item,
        // the separator comes before the comment on re-reading, and a line
        // comment that ended the item's line (which dangles after the
        // separator, above) keeps its place before it.
        let before_tucked = (placement == Placement::OwnLine)
            .then(|| {
                next.filter(|next| {
                    next.owner == enclosing && TUCKED.contains(tokens.kind(next.token))
                })
            })
            .flatten();
        let dangling = (
            enclosing,
            Position::Dangling {
                after: prev.map(|prev| prev.token),
            },
        );
        let after_prefix_operator = prev.is_some_and(|prev| prev.owner == enclosing)
            && following.is_some()
            && enclosing.cast::<UnaryExpr>().is_some();
        let (node, position) = if after_prefix_operator {
            (enclosing, Position::Leading)
        } else if let Some(separator) = terminator.or(before_tucked) {
            (
                enclosing,
                Position::Dangling {
                    after: Some(separator.token),
                },
            )
        } else {
            match placement {
                Placement::OwnLine => match (following, after_separator, preceding) {
                    (Some(following), _, _) => (following, Position::Leading),
                    (None, Some(after_separator), _) => after_separator,
                    (None, None, Some(preceding)) => (preceding, Position::Trailing),
                    (None, None, None) => dangling,
                },
                Placement::EndOfLine => match (after_separator, preceding) {
                    (Some(after_separator), _) => after_separator,
                    (None, Some(preceding)) => (preceding, Position::Trailing),
                    (None, None) => dangling,
                },
                // Directly after a separator it dangles after it, as a
                // comment that ends the separator's line does: it then stays
                // on the separator's line when the list breaks, and reads the
                // same way on the next pass.
                Placement::SameLine => match (after_separator, preceding, following) {
                    (Some(after_separator), _, _) => after_separator,
                    (None, Some(preceding), _) => (preceding, Position::Trailing),
                    (None, None, Some(following)) => (following, Position::Leading),
                    (None, None, None) => match before_separator() {
                        Some(preceding) => (preceding, Position::Trailing),
                        None => dangling,
                    },
                },
            }
        };
        SourceComment {
            token,
            kind,
            placement,
            before: gap_before(tokens, token),
            after: gap_after(tokens, token),
            node,
            position,
        }
    }

    /// The nearest token before `token` that a node owns.
    fn owned_before(&self, token: TokenIdx) -> Option<Owned> {
        (0..token.0)
            .rev()
            .map(TokenIdx)
            .find_map(|earlier| self.owned(earlier))
    }

    /// The child of `enclosing` that `descendant` is or lies under.
    fn child_of(&self, enclosing: AnyNodeId, descendant: AnyNodeId) -> AnyNodeId {
        self.index
            .ancestors(descendant)
            .find(|&node| self.index.parent(node) == Some(enclosing))
            .unwrap_or_else(|| unreachable!("`descendant` lies under `enclosing`"))
    }
}
