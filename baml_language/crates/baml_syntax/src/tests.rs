//! The tree exercised by hand, the way the parser will build it:
//! `let x = f(a, b` — a call whose closing paren is missing.

use text_size::TextSize;

use crate::{
    lexer::lex,
    nodes::{
        AnyDoc, AnyExpr, AnyItem, AnyPattern, Arenas, Arg, ArgList, BindingPattern, CallExpr,
        FileTree, LetItem, LetStmt, Name, NodeKind, PathExpr, SourceFile,
    },
    ops::{AssignOp, BinaryOp, PREFIX_BINDING_POWER},
    token::{Token, TokenAtOffset, TokenIdx, TokenKind, TokenSet, Tokens},
    tree::{AnyNodeId, Bogus, Id, Missing, SepItem, TokenRange},
};

const TEXT: &str = "let x = f(a, b";

/// The twelve tokens of [`TEXT`]: `let` ␠ `x` ␠ `=` ␠ `f` `(` `a` `,` ␠ `b`.
fn tokens() -> Tokens {
    let tokens = lex(TEXT);
    assert_eq!(tokens.len(), 12);
    tokens
}

fn tok<S: TokenSet>(tokens: &Tokens, i: u32) -> Token<S> {
    Token::new(TokenIdx(i), tokens.kind(TokenIdx(i)))
}

fn span(start: u32, end: u32) -> TokenRange {
    TokenRange::new(TokenIdx(start), TokenIdx(end))
}

fn path(arenas: &mut Arenas, tokens: &Tokens, i: u32) -> AnyExpr {
    let name = arenas.alloc(
        Name {
            token: tok(tokens, i),
        },
        span(i, i + 1),
    );
    let segments = arenas.alloc_list([]);
    AnyExpr::Path(arenas.alloc(
        PathExpr {
            head: name,
            segments,
        },
        span(i, i + 1),
    ))
}

fn build() -> FileTree {
    let tokens = tokens();
    let mut a = Arenas::default();
    let x = a.alloc(
        Name {
            token: tok(&tokens, 2),
        },
        span(2, 3),
    );
    let pattern = a.alloc(
        BindingPattern {
            introducer: None,
            name: x,
            ascription: None,
        },
        span(2, 3),
    );
    let callee = path(&mut a, &tokens, 6);
    let arg_a = path(&mut a, &tokens, 8);
    let arg_a = a.alloc(
        Arg {
            label: None,
            value: Ok(arg_a),
        },
        span(8, 9),
    );
    let arg_b = path(&mut a, &tokens, 11);
    let arg_b = a.alloc(
        Arg {
            label: None,
            value: Ok(arg_b),
        },
        span(11, 12),
    );
    let args = a.alloc_list([
        SepItem {
            item: arg_a,
            sep: Some(tok(&tokens, 9)),
        },
        SepItem {
            item: arg_b,
            sep: None,
        },
    ]);
    let arg_list = a.alloc(
        ArgList {
            l: tok(&tokens, 7),
            args,
            r: Err(Missing::new(TextSize::from(14))),
        },
        span(7, 12),
    );
    let call = a.alloc(
        CallExpr {
            callee,
            args: arg_list,
        },
        span(6, 12),
    );
    let stmt = a.alloc(
        LetStmt {
            introducer: tok(&tokens, 0),
            pattern: Ok(AnyPattern::Binding(pattern)),
            eq: Ok(tok(&tokens, 4)),
            init: Ok(AnyExpr::Call(call)),
            else_branch: None,
            semi: None,
        },
        span(0, 12),
    );
    let docs = a.alloc_list::<AnyDoc>([]);
    let item = a.alloc(LetItem { docs, stmt }, span(0, 12));
    let items = a.alloc_list([AnyItem::Let(item)]);
    let root = a.alloc(SourceFile { items }, span(0, 12));
    FileTree::new(TEXT.into(), tokens, a, root)
}

#[test]
fn debug_tree_shows_fields_tokens_and_missing_children() {
    let expected = r#"SourceFile@0..14
  items: LetItem@0..14
    stmt: LetStmt@0..14
      introducer: Let "let" @0..3
      pattern: BindingPattern@4..5
        name: Name@4..5
          token: Word "x" @4..5
      eq: Equals "=" @6..7
      init: CallExpr@8..14
        callee: PathExpr@8..9
          head: Name@8..9
            token: Word "f" @8..9
        args: ArgList@9..14
          l: LParen "(" @9..10
          args: Arg@10..11
            value: PathExpr@10..11
              head: Name@10..11
                token: Word "a" @10..11
          args: Comma "," @11..12
          args: Arg@13..14
            value: PathExpr@13..14
              head: Name@13..14
                token: Word "b" @13..14
          r: <missing @14>
"#;
    assert_eq!(build().debug_tree(), expected);
}

#[test]
fn typed_access_and_separated_lists() {
    let tree = build();
    let file = tree.get(tree.root());
    let [AnyItem::Let(stmt)] = tree.list(file.items) else {
        panic!("one let item");
    };
    let Ok(AnyExpr::Call(call)) = tree.get(tree.get(*stmt).stmt).init else {
        panic!("the initialiser is a call");
    };
    let args = tree.get(tree.get(call).args);
    let items = tree.list(args.args);
    assert_eq!(items.len(), 2);
    assert!(
        items[0].sep.is_some() && items[1].sep.is_none(),
        "a trailing separator is observable"
    );
    assert_eq!(args.r, Err(Missing::new(TextSize::from(14))));
}

#[test]
fn any_node_ids_round_trip_through_cast() {
    let tree = build();
    let call: Id<CallExpr> = tree
        .arenas()
        .ids::<CallExpr>()
        .next()
        .unwrap_or_else(|| unreachable!("one call"));
    let any = AnyNodeId::of(call);
    assert_eq!(Arenas::kind(any), NodeKind::CallExpr);
    assert_eq!(any.cast::<CallExpr>(), Some(call));
    assert_eq!(any.cast::<Name>(), None);
    assert_eq!(
        NodeKind::from_raw(NodeKind::Bogus.to_raw()),
        Some(NodeKind::Bogus)
    );
}

#[test]
fn nodes_and_handles_stay_small() {
    // A union is a tag plus an index; an id is an index.
    assert_eq!(size_of::<Id<CallExpr>>(), 4);
    assert_eq!(size_of::<AnyExpr>(), 8);
    assert!(
        size_of::<LetStmt>() <= 48,
        "LetStmt is {} bytes",
        size_of::<LetStmt>()
    );
}

#[test]
fn bogus_walks_its_tokens() {
    let tokens = tokens();
    let mut a = Arenas::default();
    let bogus = a.alloc(Bogus::new(span(6, 8)), span(6, 8));
    let items = a.alloc_list([AnyItem::Bogus(bogus)]);
    let root = a.alloc(SourceFile { items }, span(6, 8));
    let tree = FileTree::new(TEXT.into(), tokens, a, root);
    assert_eq!(
        tree.debug_tree(),
        "SourceFile@8..10\n  items: Bogus@8..10\n    tokens: Word \"f\" @8..9\n    tokens: LParen \"(\" @9..10\n"
    );
}

#[test]
fn token_navigation() {
    let tokens = tokens();
    assert_eq!(
        tokens.token_at_offset(TextSize::from(0)),
        TokenAtOffset::Single(TokenIdx(0))
    );
    assert_eq!(
        tokens.token_at_offset(TextSize::from(9)),
        TokenAtOffset::Between {
            left: TokenIdx(6),
            right: TokenIdx(7)
        }
    );
    assert_eq!(
        tokens.token_at_offset(TextSize::from(14)),
        TokenAtOffset::None
    );
    assert_eq!(tokens.prev_significant(TokenIdx(11)), Some(TokenIdx(9)));
    assert_eq!(tokens.next_significant(TokenIdx(9)), Some(TokenIdx(11)));
    assert_eq!(tokens.next_significant(TokenIdx(11)), None);
    assert!(tokens.joined_to_prev(TokenIdx(7)), "`f(` are adjacent");
    assert!(
        !tokens.joined_to_prev(TokenIdx(11)),
        "`, b` are separated by trivia"
    );
    assert!(TokenKind::Type.is_contextual_keyword() && !TokenKind::Type.is_keyword());
    assert_eq!(TokenKind::Arrow.describe(), "->");
}

#[test]
fn composite_operators_are_read_from_adjacent_tokens() {
    let at = |text: &str, i: u32| BinaryOp::at(&lex(text), TokenIdx(i));
    assert_eq!(at("a ?? b", 2), Some(BinaryOp::NullCoalesce));
    // `? ?` — not adjacent, so not an operator.
    assert_eq!(at("? ?", 0), None);
    // `>>` and a lone `>`.
    assert_eq!(at(">>", 0), Some(BinaryOp::Shr));
    assert_eq!(at("> >", 0), Some(BinaryOp::Gt));
    // `>>=` is an assignment, which the parser asks for first; read as a
    // binary operator it would be `>>`.
    let assign_at = |text: &str, i: u32| AssignOp::at(&lex(text), TokenIdx(i));
    assert_eq!(assign_at(">>=", 0), Some(AssignOp::ShrAssign));
    assert_eq!(at(">>=", 0), Some(BinaryOp::Shr));
    assert_eq!(assign_at(">>", 0), None);
    assert_eq!(assign_at("a += b", 2), Some(AssignOp::AddAssign));
    assert_eq!(assign_at("a == b", 2), None);
}

#[test]
fn precedence_matches_the_current_parser() {
    assert_eq!(BinaryOp::NullCoalesce.binding_power(), (4, 5));
    assert_eq!(BinaryOp::Or.binding_power(), (6, 7));
    assert_eq!(BinaryOp::Add.binding_power(), (22, 23));
    assert_eq!(BinaryOp::Mul.binding_power(), (24, 25));
    // Ported as-is (see the BUG note on the constant).
    assert!(PREFIX_BINDING_POWER < BinaryOp::Mul.binding_power().0);
}

#[test]
fn token_macro_resolves_spellings_kinds_and_sets() {
    use std::marker::PhantomData;

    use crate::token::{Ident, ctx, kw, lit, punct, sets::LetIntroducer};

    /// Compiles only if both arguments have the same type.
    fn same<T>(_: PhantomData<T>, _: PhantomData<T>) {}

    same(
        PhantomData::<Token!["{"]>,
        PhantomData::<Token<punct::LBrace>>,
    );
    same(
        PhantomData::<Token!["?."]>,
        PhantomData::<Token<punct::QuestionDot>>,
    );
    same(
        PhantomData::<Token!["function"]>,
        PhantomData::<Token<kw::Function>>,
    );
    same(PhantomData::<Token!["map"]>, PhantomData::<Token<ctx::Map>>);
    same(
        PhantomData::<Token!["_"]>,
        PhantomData::<Token<ctx::Underscore>>,
    );
    same(PhantomData::<Token![Word]>, PhantomData::<Token<lit::Word>>);
    same(
        PhantomData::<Token![LetIntroducer]>,
        PhantomData::<Token<LetIntroducer>>,
    );
    same(PhantomData::<Token![Ident]>, PhantomData::<Token<Ident>>);
}

#[test]
fn operator_token_counts_follow_the_token_table() {
    assert_eq!(BinaryOp::NullCoalesce.token_len(), 2);
    assert_eq!(BinaryOp::Shr.token_len(), 2);
    assert_eq!(AssignOp::ShrAssign.token_len(), 3);
    assert_eq!(AssignOp::ShlAssign.token_len(), 1);
    assert_eq!(AssignOp::Assign.token_len(), 1);
    assert_eq!(BinaryOp::Ge.token_len(), 2);
    assert_eq!(BinaryOp::Le.token_len(), 1);
    assert_eq!(BinaryOp::Instanceof.token_len(), 1);
}

#[test]
fn every_operator_reads_back_from_its_own_tokens() {
    use crate::ops::UnaryOp;
    for &op in BinaryOp::ALL {
        let tokens = lex(op.spelling());
        assert_eq!(tokens.len(), op.token_len(), "{}", op.spelling());
        assert_eq!(
            BinaryOp::at(&tokens, TokenIdx(0)),
            Some(op),
            "{}",
            op.spelling()
        );
        let joined: String = op.tokens().iter().map(|kind| kind.describe()).collect();
        assert_eq!(joined, op.spelling());
    }
    for &op in UnaryOp::ALL {
        assert_eq!(UnaryOp::from_kind(op.token()), Some(op));
        assert_eq!(op.token().describe(), op.spelling());
    }
}
