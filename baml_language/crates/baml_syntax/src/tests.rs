//! The macros exercised on a small tree: `f(a, b` — a call with a missing
//! closing paren — built by hand the way a parser would build it.

use text_size::TextSize;

use crate::{
    AnyNode, Node, Walk,
    token::{Ident, Token, TokenAtOffset, TokenIdx, TokenKind, Tokens, lit, punct},
    tree::{Alloc as _, AnyNodeId, Bogus, HasArena as _, Id, Missing, Sep, SepItem, TokenRange},
};

#[derive(Node, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct File {
    pub items: crate::tree::List<AnyItem>,
}

#[derive(Node, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Name {
    pub token: Token<Ident>,
}

#[derive(Node, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Call {
    pub callee: Id<Name>,
    pub l: Token<punct::LParen>,
    pub args: Sep<AnyItem, punct::Comma>,
    pub r: Result<Token<punct::RParen>, Missing>,
    pub label: Option<Label>,
}

/// An inline part, walked transparently as part of its parent.
#[derive(Walk, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Label {
    pub colon: Token<punct::Colon>,
    pub name: Id<Name>,
}

#[derive(AnyNode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AnyItem {
    Name(Id<Name>),
    Call(Id<Call>),
    Word(Token<lit::Word>),
    Bogus(Id<Bogus>),
}

crate::define_tree! {
    root: File,
    macro_name: mini_for_each_node_type,
    nodes { File, Name, Call, Bogus },
    pools { AnyItem, SepItem<AnyItem, punct::Comma> },
}

/// `f(a, b` as tokens: Word ( Word , Space Word.
fn tokens() -> (Tokens, &'static str) {
    let text = "f(a, b";
    let mut tokens = Tokens::with_capacity(6);
    for (kind, start) in [
        (TokenKind::Word, 0),
        (TokenKind::LParen, 1),
        (TokenKind::Word, 2),
        (TokenKind::Comma, 3),
        (TokenKind::Whitespace, 4),
        (TokenKind::Word, 5),
    ] {
        tokens.push(kind, TextSize::from(start));
    }
    tokens.finish(TextSize::from(6));
    (tokens, text)
}

fn tok<S: crate::token::TokenSet>(tokens: &Tokens, i: u32) -> Token<S> {
    Token::new(TokenIdx(i), tokens.kind(TokenIdx(i)))
}

fn build() -> FileTree {
    let (tokens, text) = tokens();
    let mut arenas = Arenas::default();
    let f = arenas.alloc(
        Name {
            token: tok(&tokens, 0),
        },
        TokenRange::new(TokenIdx(0), TokenIdx(1)),
    );
    let a = arenas.alloc(
        Name {
            token: tok(&tokens, 2),
        },
        TokenRange::new(TokenIdx(2), TokenIdx(3)),
    );
    let b = arenas.alloc(
        Name {
            token: tok(&tokens, 5),
        },
        TokenRange::new(TokenIdx(5), TokenIdx(6)),
    );
    let args = arenas.alloc_list([
        SepItem {
            item: AnyItem::Name(a),
            sep: Some(tok(&tokens, 3)),
        },
        SepItem {
            item: AnyItem::Name(b),
            sep: None,
        },
    ]);
    let call = arenas.alloc(
        Call {
            callee: f,
            l: tok(&tokens, 1),
            args,
            r: Err(Missing {
                at: TextSize::from(6),
            }),
            label: None,
        },
        TokenRange::new(TokenIdx(0), TokenIdx(6)),
    );
    let items = arenas.alloc_list([AnyItem::Call(call)]);
    let root = arenas.alloc(File { items }, TokenRange::new(TokenIdx(0), TokenIdx(6)));
    FileTree {
        text: text.into(),
        tokens,
        arenas,
        root,
    }
}

#[test]
fn debug_tree_shows_fields_tokens_and_missing_children() {
    let tree = build();
    let expected = "\
File@0..6
  items: Call@0..6
    callee: Name@0..1
      token: Word \"f\" @0..1
    l: LParen \"(\" @1..2
    args: Name@2..3
      token: Word \"a\" @2..3
    args: Comma \",\" @3..4
    args: Name@5..6
      token: Word \"b\" @5..6
    r: <missing @6>
";
    assert_eq!(tree.debug_tree(), expected);
}

#[test]
fn typed_access_and_lists() {
    let tree = build();
    let file = tree.get(tree.root);
    let items = tree.list(file.items);
    let AnyItem::Call(call_id) = items[0] else {
        panic!("the only item is the call");
    };
    let call = tree.get(call_id);
    assert_eq!(tree.token_text(tree.get(call.callee).token.idx), "f");
    let args = tree.list(call.args);
    assert_eq!(args.len(), 2);
    assert!(
        args[0].sep.is_some() && args[1].sep.is_none(),
        "trailing separator is observable"
    );
    assert_eq!(
        call.r,
        Err(Missing {
            at: TextSize::from(6)
        })
    );
    assert_eq!(
        tree.span(call_id),
        TokenRange::new(TokenIdx(0), TokenIdx(6))
    );
}

#[test]
fn any_node_ids_round_trip_through_cast() {
    let tree = build();
    let call_id = tree
        .arenas
        .call
        .ids()
        .next()
        .unwrap_or_else(|| unreachable!("one call"));
    let any = AnyNodeId::of(call_id);
    assert_eq!(Arenas::kind(any), NodeKind::Call);
    assert_eq!(any.cast::<Call>(), Some(call_id));
    assert_eq!(any.cast::<Name>(), None);
    assert!(matches!(tree.arenas.node(any), AnyNodeRef::Call(_)));
    assert_eq!(
        NodeKind::from_raw(NodeKind::Bogus.to_raw()),
        Some(NodeKind::Bogus)
    );
    assert_eq!(NodeKind::ALL.len(), 4);
}

#[test]
fn any_node_payloads_report_their_node_id_or_none() {
    use crate::tree::AnyNode as _;
    let tree = build();
    let name_id = tree
        .arenas
        .name
        .ids()
        .next()
        .unwrap_or_else(|| unreachable!("names exist"));
    assert_eq!(
        AnyItem::Name(name_id).any_node_id(),
        Some(AnyNodeId::of(name_id))
    );
    assert_eq!(AnyItem::Word(tok(&tree.tokens, 0)).any_node_id(), None);
}

#[test]
fn for_each_node_type_lists_every_node() {
    macro_rules! count {
        ($($t:ident),*) => { [$(stringify!($t)),*].len() };
    }
    assert_eq!(mini_for_each_node_type!(count), 4);
}

#[test]
fn token_navigation() {
    let (tokens, _) = tokens();
    assert_eq!(tokens.len(), 6);
    assert_eq!(
        tokens.token_at_offset(TextSize::from(0)),
        TokenAtOffset::Single(TokenIdx(0))
    );
    assert_eq!(
        tokens.token_at_offset(TextSize::from(1)),
        TokenAtOffset::Between {
            left: TokenIdx(0),
            right: TokenIdx(1)
        }
    );
    assert_eq!(
        tokens.token_at_offset(TextSize::from(6)),
        TokenAtOffset::None
    );
    assert_eq!(
        tokens.token_at_offset(TextSize::from(7)),
        TokenAtOffset::None
    );
    // `, b`: the whitespace is skipped by the significant-token walks.
    assert_eq!(tokens.prev_significant(TokenIdx(5)), Some(TokenIdx(3)));
    assert_eq!(tokens.next_significant(TokenIdx(3)), Some(TokenIdx(5)));
    assert_eq!(tokens.next_significant(TokenIdx(5)), None);
    // `f(` are adjacent; `, b` are not.
    assert!(tokens.joined_to_prev(TokenIdx(1)));
    assert!(!tokens.joined_to_prev(TokenIdx(5)));
    assert!(!tokens.joined_to_prev(TokenIdx(0)));
    assert!(TokenKind::Whitespace.is_trivia() && !TokenKind::Word.is_trivia());
    assert!(TokenKind::Type.is_contextual_keyword() && !TokenKind::Type.is_keyword());
    assert_eq!(TokenKind::Arrow.describe(), "->");
}

#[test]
fn bogus_walks_its_tokens() {
    let (tokens, text) = tokens();
    let mut arenas = Arenas::default();
    let bogus = arenas.alloc(
        Bogus {
            tokens: TokenRange::new(TokenIdx(2), TokenIdx(4)),
        },
        TokenRange::new(TokenIdx(2), TokenIdx(4)),
    );
    let items = arenas.alloc_list([AnyItem::Bogus(bogus)]);
    let root = arenas.alloc(File { items }, TokenRange::new(TokenIdx(0), TokenIdx(6)));
    let tree = FileTree {
        text: text.into(),
        tokens,
        arenas,
        root,
    };
    let dump = tree.debug_tree();
    assert!(
        dump.contains(
            "items: Bogus@2..4\n    tokens: Word \"a\" @2..3\n    tokens: Comma \",\" @3..4\n"
        ),
        "{dump}"
    );
    let _ = tree.arenas.arena().span(bogus);
}
