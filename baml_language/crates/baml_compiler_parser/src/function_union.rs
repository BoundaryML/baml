//! Explicit groupings for an ambiguous function/union boundary (E0175).
//!
//! When a pipe inside an undelimited function's return or `throws` operand is
//! followed by a function type, the pipe can either stay in that operand or end
//! the function. The parser reports such pipes; this module searches the
//! complete parses of the surrounding expression and renders two of them that
//! disagree about the reported pipe, parenthesized so each parses back to
//! exactly that grouping.
//!
//! The search runs over a flat item sequence: function heads (a parameter list
//! followed by `->`), `throws`, pipes, and atoms. Every bracketed group that is
//! not a function head is an opaque atom, since its contents cannot interact
//! with the pipes outside it.

use baml_compiler_lexer::TokenKind;

/// A non-trivia token of the ambiguous expression.
pub(crate) struct SigToken<'a> {
    pub(crate) kind: TokenKind,
    pub(crate) text: &'a str,
    /// Whether whitespace or a comment separated this token from the previous
    /// one; rendered as a single space.
    pub(crate) space_before: bool,
}

/// Upper bound on complete parses explored. Real expressions have a handful.
const MAX_PARSES: usize = 256;

#[derive(Debug)]
enum Item {
    /// Rendered parameter list of a function type; the `->` is implied.
    Head(String),
    Throws,
    /// A pipe, identified by its index in the token slice.
    Pipe(usize),
    /// Rendered text, and whether it is a parenthesized function type.
    Atom(String, bool),
}

#[derive(Clone, Debug)]
enum Node {
    Atom(usize),
    Fn {
        head: usize,
        ret: Union,
        throws: Option<Union>,
    },
}

/// A union and the item indices of the pipes that join its members.
#[derive(Clone, Debug)]
struct Union {
    members: Vec<Node>,
    pipes: Vec<usize>,
}

/// Which union a pipe joins in one parse: the return (`false`) or throws
/// (`true`) operand of the function whose head is at the given item, or the
/// top level.
type Owner = Option<(usize, bool)>;

/// For each token index in `pipes`, two groupings of the whole expression that
/// assign that pipe to different unions: first the parser's own grouping, then
/// the next complete parse (depth-first, keeping pipes inside the innermost
/// function first) that differs at that pipe. `None` when no second grouping
/// exists or the expression is not well formed.
pub(crate) fn groupings(tokens: &[SigToken<'_>], pipes: &[usize]) -> Vec<Option<[String; 2]>> {
    let Some(items) = items(tokens) else {
        return vec![None; pipes.len()];
    };
    let search = Search { items: &items };
    let parses: Vec<Union> = search
        .union(0, false)
        .into_iter()
        .filter(|(_, end)| *end == items.len())
        .map(|(union, _)| union)
        .collect();
    let Some(first) = parses.first() else {
        return vec![None; pipes.len()];
    };
    pipes
        .iter()
        .map(|&pipe| {
            let item = items
                .iter()
                .position(|item| matches!(item, Item::Pipe(token) if *token == pipe))?;
            let owner = owner_of(first, None, item)?;
            let other = parses
                .iter()
                .find(|parse| owner_of(parse, None, item) != Some(owner))?;
            Some([
                render_union(&items, first, false),
                render_union(&items, other, false),
            ])
        })
        .collect()
}

fn items(tokens: &[SigToken<'_>]) -> Option<Vec<Item>> {
    let mut items = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i].kind {
            TokenKind::Pipe => {
                items.push(Item::Pipe(i));
                i += 1;
            }
            TokenKind::Throws => {
                items.push(Item::Throws);
                i += 1;
            }
            TokenKind::LParen
                if matching_close(tokens, i).is_some_and(|end| {
                    tokens.get(end + 1).map(|t| t.kind) == Some(TokenKind::Arrow)
                }) =>
            {
                let end = matching_close(tokens, i)?;
                items.push(Item::Head(text(tokens, i, end)));
                i = end + 2;
            }
            _ => {
                let start = i;
                while i < tokens.len()
                    && !matches!(tokens[i].kind, TokenKind::Pipe | TokenKind::Throws)
                {
                    match tokens[i].kind {
                        TokenKind::Arrow => return None,
                        kind if is_open(kind) => i = matching_close(tokens, i)? + 1,
                        _ => i += 1,
                    }
                }
                items.push(Item::Atom(
                    text(tokens, start, i - 1),
                    wraps_function(tokens, start, i - 1),
                ));
            }
        }
    }
    Some(items)
}

/// Whether `tokens[start..=end]` is a function type in one or more pairs of
/// parentheses, like `((C) -> D)`. The parser treats it as a function member.
fn wraps_function(tokens: &[SigToken<'_>], mut start: usize, mut end: usize) -> bool {
    while tokens[start].kind == TokenKind::LParen && matching_close(tokens, start) == Some(end) {
        start += 1;
        end -= 1;
        if start > end {
            return false;
        }
        if tokens[start].kind == TokenKind::LParen
            && matching_close(tokens, start).is_some_and(|close| {
                tokens.get(close + 1).map(|t| t.kind) == Some(TokenKind::Arrow)
            })
        {
            return true;
        }
    }
    false
}

fn is_open(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace | TokenKind::Less
    )
}

/// Index of the token closing the group opened at `open`.
fn matching_close(tokens: &[SigToken<'_>], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, token) in tokens.iter().enumerate().skip(open) {
        match token.kind {
            kind if is_open(kind) => depth += 1,
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace | TokenKind::Greater => {
                depth = depth.checked_sub(1)?;
            }
            // Only reached when a nested generic closes with the outer one.
            TokenKind::GreaterGreater => depth = depth.checked_sub(2)?,
            _ => {}
        }
        if depth == 0 {
            return Some(i);
        }
    }
    None
}

fn text(tokens: &[SigToken<'_>], start: usize, end: usize) -> String {
    let mut out = String::new();
    for (i, token) in tokens[start..=end].iter().enumerate() {
        if i > 0 && token.space_before {
            out.push(' ');
        }
        out.push_str(token.text);
    }
    out
}

struct Search<'a> {
    items: &'a [Item],
}

impl Search<'_> {
    /// Every way to parse a union starting at `pos`, with the item index just
    /// past it. Inside a function operand a pipe may also end the union.
    /// Longer unions come first, so the first parse keeps each pipe in the
    /// innermost function, as the parser does.
    fn union(&self, pos: usize, in_function: bool) -> Vec<(Union, usize)> {
        let mut out = Vec::new();
        for (member, next) in self.member(pos) {
            if let Some(Item::Pipe(_)) = self.items.get(next) {
                for (rest, end) in self.union(next + 1, in_function) {
                    let mut members = vec![member.clone()];
                    members.extend(rest.members);
                    let mut pipes = vec![next];
                    pipes.extend(rest.pipes);
                    out.push((Union { members, pipes }, end));
                    if out.len() >= MAX_PARSES {
                        return out;
                    }
                }
            }
            if in_function || next == self.items.len() {
                out.push((
                    Union {
                        members: vec![member],
                        pipes: Vec::new(),
                    },
                    next,
                ));
            }
            if out.len() >= MAX_PARSES {
                return out;
            }
        }
        out
    }

    /// Every way to parse one union member at `pos`. A function takes a
    /// following `throws` first, and otherwise leaves it to an enclosing one.
    fn member(&self, pos: usize) -> Vec<(Node, usize)> {
        match self.items.get(pos) {
            Some(Item::Atom(..)) => vec![(Node::Atom(pos), pos + 1)],
            Some(Item::Head(_)) => {
                let mut out = Vec::new();
                for (ret, next) in self.union(pos + 1, true) {
                    if let Some(Item::Throws) = self.items.get(next) {
                        for (throws, end) in self.union(next + 1, true) {
                            out.push((
                                Node::Fn {
                                    head: pos,
                                    ret: ret.clone(),
                                    throws: Some(throws),
                                },
                                end,
                            ));
                        }
                    }
                    out.push((
                        Node::Fn {
                            head: pos,
                            ret,
                            throws: None,
                        },
                        next,
                    ));
                    if out.len() >= MAX_PARSES {
                        break;
                    }
                }
                out
            }
            _ => Vec::new(),
        }
    }
}

fn owner_of(union: &Union, owner: Owner, pipe: usize) -> Option<Owner> {
    if union.pipes.contains(&pipe) {
        return Some(owner);
    }
    union.members.iter().find_map(|member| match member {
        Node::Atom(_) => None,
        Node::Fn { head, ret, throws } => owner_of(ret, Some((*head, false)), pipe).or_else(|| {
            throws
                .as_ref()
                .and_then(|throws| owner_of(throws, Some((*head, true)), pipe))
        }),
    })
}

/// Render a union so it parses back to this grouping. Function members are
/// parenthesized, except the last member of a function operand, which the
/// operand's own parentheses already delimit.
fn render_union(items: &[Item], union: &Union, is_operand: bool) -> String {
    let last = union.members.len() - 1;
    union
        .members
        .iter()
        .enumerate()
        .map(|(i, member)| {
            let rendered = render_node(items, member);
            let wrap = matches!(member, Node::Fn { .. })
                && union.members.len() > 1
                && !(is_operand && i == last);
            if wrap {
                format!("({rendered})")
            } else {
                rendered
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_node(items: &[Item], node: &Node) -> String {
    match node {
        Node::Atom(item) => {
            let Item::Atom(text, _) = &items[*item] else {
                unreachable!("atoms index atom items")
            };
            text.clone()
        }
        Node::Fn { head, ret, throws } => {
            let Item::Head(params) = &items[*head] else {
                unreachable!("functions index head items")
            };
            let mut out = format!("{params} -> {}", render_operand(items, ret));
            if let Some(throws) = throws {
                out.push_str(" throws ");
                out.push_str(&render_operand(items, throws));
            }
            out
        }
    }
}

/// A return or throws operand. A lone function is parenthesized so a later
/// `throws` cannot attach to it; a union with a function member (including an
/// already parenthesized one) is parenthesized so its pipes cannot end the
/// function. Unions of plain types are unambiguous and stay bare.
fn render_operand(items: &[Item], union: &Union) -> String {
    let has_function = union.members.iter().any(|member| match member {
        Node::Fn { .. } => true,
        // A lone parenthesized function is already delimited.
        Node::Atom(item) => union.members.len() > 1 && matches!(items[*item], Item::Atom(_, true)),
    });
    let rendered = render_union(items, union, true);
    if has_function {
        format!("({rendered})")
    } else {
        rendered
    }
}
