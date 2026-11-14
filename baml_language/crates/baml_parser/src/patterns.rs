//! Patterns: bindings, wildcards, class and array destructures, types as
//! patterns, and `|` alternatives. The `let`/`const` introducer of a binding
//! statement is the statement's; a pattern carries one only where it
//! introduces the binding itself (`match x { let y => … }`).

use baml_syntax::{
    Token,
    nodes::{
        AnyArrayPatternElement, AnyPattern, ArrayPattern, BindingPattern, DestructurePattern,
        FieldPattern, ParenPattern, PatternArm, PatternAscription, RestPattern, TuplePattern,
        TypeAscription, TypePattern, UnionPattern, WildcardPattern,
    },
    token::{
        Ident, KindSet, TokenKind, TokenSet as _, ctx, punct,
        sets::{LetIntroducer, NameToken, StringToken},
    },
    tree::{AnyNodeId, Id, Missing, SepItem},
};

use crate::{
    cursor::{Cursor, Mark},
    expr::{Restrictions, STATEMENT_KEYWORD},
    lookahead::{
        at_binding_intro, destructure_brace, looks_like_function_type_paren,
        looks_like_paren_type_suffix, looks_like_qualified_projection,
    },
    parser::Parser,
    types::{any_type, path_type, reject_unmatched_greater, type_operand},
};

const NUMBER: KindSet = KindSet::new(&[TokenKind::Bigint, TokenKind::Integer, TokenKind::Float]);

/// What begins a pattern besides a binding introducer.
const PATTERN_START: KindSet =
    Ident::KINDS
        .union(StringToken::KINDS)
        .union(NUMBER)
        .union(KindSet::new(&[
            TokenKind::Minus,
            TokenKind::LParen,
            TokenKind::LBracket,
            TokenKind::Less,
        ]));

pub(crate) fn at_pattern_start(cur: Cursor<'_>) -> bool {
    cur.at_set(PATTERN_START) || at_binding_intro(cur)
}

/// A pattern: alternatives joined by `|`, each an atom. `Err` (reported)
/// when nothing here begins one.
pub(crate) fn pattern(p: &mut Parser<'_, '_>, r: Restrictions) -> Result<AnyPattern, Missing> {
    p.enter_nesting("a pattern")?;
    let first = atom(p, r);
    let result = first.map(|first| union_rest(p, first, r));
    p.leave_nesting();
    result
}

/// The pattern after a statement's `let`/`const`, which is already consumed:
/// the binding shapes (`x`, `x: pattern`, `_`, `Class { … }`, `[…]`), then
/// any `|` alternatives.
pub(crate) fn pattern_after_introducer(
    p: &mut Parser<'_, '_>,
    r: Restrictions,
) -> Result<AnyPattern, Missing> {
    let first = introduced(p, None, r)?;
    Ok(union_rest(p, first, r))
}

/// `first` and the `| atom` arms after it, if any.
fn union_rest(p: &mut Parser<'_, '_>, first: AnyPattern, r: Restrictions) -> AnyPattern {
    let mut arms = Vec::new();
    while let Some(pipe) = p.cur.eat::<punct::Pipe>() {
        let pattern = atom(p, r);
        let ended = pattern.is_err();
        arms.push(PatternArm { pipe, pattern });
        if ended {
            break;
        }
    }
    let mut arms = arms.into_iter();
    let Some(first_arm) = arms.next() else {
        return first;
    };
    let mark = p.mark_of_any(&first);
    let arms = p.out.alloc_list1(first_arm, arms.collect());
    AnyPattern::Union(p.alloc(mark, UnionPattern { first, arms }))
}

/// One alternative.
fn atom(p: &mut Parser<'_, '_>, r: Restrictions) -> Result<AnyPattern, Missing> {
    let mark = p.cur.mark();
    if p.cur.at(TokenKind::LParen) {
        // A `(` opens a parenthesized pattern, or a type: a function type
        // when `->` follows the `)`, a modified type when `[`/`?` does, a
        // projection when `as` is inside.
        if looks_like_qualified_projection(p.cur) || looks_like_function_type_paren(p.cur) {
            let ty = type_operand(p)?;
            return Ok(AnyPattern::Type(p.alloc(mark, TypePattern { ty })));
        }
        if looks_like_paren_type_suffix(p.cur) {
            let ty = any_type(p)?;
            return Ok(AnyPattern::Type(p.alloc(mark, TypePattern { ty })));
        }
    }
    if let Some(l) = p.cur.eat::<punct::LParen>() {
        let inner = pattern(p, Restrictions::empty());
        if let Some(sep) = p.cur.eat::<punct::Comma>() {
            let mut elements = Vec::new();
            if let Ok(item) = inner {
                elements.push(SepItem {
                    item,
                    sep: Some(sep),
                });
            }
            while !p.cur.at(TokenKind::RParen) && !p.cur.at_end() {
                let Ok(item) = pattern(p, Restrictions::empty()) else {
                    break;
                };
                let sep = p.cur.eat::<punct::Comma>();
                elements.push(SepItem { item, sep });
                if sep.is_none() {
                    break;
                }
            }
            let elements = p.out.alloc_list(elements);
            let r = p.expect::<punct::RParen>();
            return Ok(AnyPattern::Tuple(
                p.alloc(mark, TuplePattern { l, elements, r }),
            ));
        }
        let r = p.expect::<punct::RParen>();
        return Ok(AnyPattern::Paren(
            p.alloc(mark, ParenPattern { l, inner, r }),
        ));
    }
    if at_binding_intro(p.cur)
        && let Some(introducer) = p.cur.eat_in::<LetIntroducer>()
    {
        return introduced(p, Some(introducer), r);
    }
    if let Some(l) = p.cur.eat::<punct::LBracket>() {
        return Ok(AnyPattern::Array(array_pattern(p, l)));
    }
    if let Some(underscore) = p.cur.eat::<ctx::Underscore>() {
        return Ok(AnyPattern::Wildcard(p.alloc(
            mark,
            WildcardPattern {
                introducer: None,
                underscore,
            },
        )));
    }
    if p.cur.at_ident()
        && let Some(open) = destructure_brace(p.cur)
        && brace_allowed(p.cur, open, r)
        && let Some(head) = p.cur.eat_set::<NameToken>(Ident::KINDS)
    {
        return Ok(destructure(p, mark, None, head));
    }
    if p.cur.at_set(PATTERN_START) {
        let ty = type_operand(p)?;
        return Ok(AnyPattern::Type(p.alloc(mark, TypePattern { ty })));
    }
    Err(p.missing("pattern"))
}

/// Whether a destructure's `{` at significant index `open` may be taken
/// here: always, unless a `{` after the pattern is the enclosing
/// construct's body (`if x is Class {`), when only a `{` whose match is
/// followed by the body's own `{` or an expression continuation is.
fn brace_allowed(cur: Cursor<'_>, open: usize, r: Restrictions) -> bool {
    !r.contains(Restrictions::BRACE_IS_BODY)
        || crate::lookahead::brace_literal_before_body(cur, open)
}

/// The shapes a binding introducer takes: `_`, `Class { … }`, `name`,
/// `name: pattern`. `introducer` is the token when the pattern introduces
/// the binding itself, `None` when the statement did (`let x = …`) — then an
/// array pattern may follow too, since the statement's introducer covers it.
fn introduced(
    p: &mut Parser<'_, '_>,
    introducer: Option<Token![LetIntroducer]>,
    r: Restrictions,
) -> Result<AnyPattern, Missing> {
    let mark = introducer.map_or_else(|| p.cur.mark(), Mark::of);
    if introducer.is_none() && p.cur.at(TokenKind::LParen) {
        return atom(p, r);
    }
    if let Some(underscore) = p.cur.eat::<ctx::Underscore>() {
        return Ok(AnyPattern::Wildcard(p.alloc(
            mark,
            WildcardPattern {
                introducer,
                underscore,
            },
        )));
    }
    if introducer.is_none()
        && let Some(l) = p.cur.eat::<punct::LBracket>()
    {
        return Ok(AnyPattern::Array(array_pattern(p, l)));
    }
    let destructures = p.cur.at_ident()
        && destructure_brace(p.cur).is_some_and(|open| brace_allowed(p.cur, open, r));
    let Some(head) = p.cur.eat_set::<NameToken>(Ident::KINDS) else {
        return Err(p.missing("identifier after 'let'"));
    };
    if destructures {
        return Ok(destructure(p, mark, introducer, head));
    }
    let name = p.name(head);
    let ascription = p.cur.eat::<punct::Colon>().map(|colon| {
        let pattern = pattern(p, r);
        PatternAscription { colon, pattern }
    });
    Ok(AnyPattern::Binding(p.alloc(
        mark,
        BindingPattern {
            introducer,
            name,
            ascription,
        },
    )))
}

/// `Class<T> { field, field: pattern }` from the class path's first name. If
/// no `{` follows the path after all, the path was a type pattern.
fn destructure(
    p: &mut Parser<'_, '_>,
    mark: Mark,
    introducer: Option<Token![LetIntroducer]>,
    head: Token![NameToken],
) -> AnyPattern {
    let path = path_type(p, head);
    let Some(l) = p.cur.eat::<punct::LBrace>() else {
        let ty = baml_syntax::nodes::AnyType::Path(path);
        return AnyPattern::Type(p.alloc(mark, TypePattern { ty }));
    };
    let fields = p.members(TokenKind::RBrace, STATEMENT_KEYWORD, Ident::KINDS, |p| {
        let Some(name) = p.eat_name(Ident::KINDS) else {
            p.unexpected("field name");
            return None;
        };
        let sub = p.cur.eat::<punct::Colon>().map(|colon| {
            let pattern = pattern(p, Restrictions::empty());
            PatternAscription { colon, pattern }
        });
        let item = p.alloc(p.mark_of(AnyNodeId::of(name)), FieldPattern { name, sub });
        let sep = p.cur.eat::<punct::Comma>();
        let missing_sep = sep.is_none().then_some("',' or '}' after field pattern");
        Some((SepItem { item, sep }, missing_sep))
    });
    let fields = p.out.alloc_list(fields);
    let r = p.expect::<punct::RBrace>();
    AnyPattern::Destructure(p.alloc(
        mark,
        DestructurePattern {
            introducer,
            path,
            l,
            fields,
            r,
        },
    ))
}

/// `a, ..rest]: T[]` after its `[`. Slots are patterns, so a binding is
/// written `let a`.
fn array_pattern(p: &mut Parser<'_, '_>, l: Token!["["]) -> Id<ArrayPattern> {
    let mut seen_rest = false;
    let elements = p.members(TokenKind::RBracket, KindSet::EMPTY, KindSet::EMPTY, |p| {
        let element = if let Some(dots) = p.cur.eat::<punct::DotDot>() {
            if seen_rest {
                p.unexpected("only one '..' rest pattern is allowed in an array pattern");
            }
            seen_rest = true;
            let pattern =
                (!p.cur.at(TokenKind::Comma) && !p.cur.at(TokenKind::RBracket) && !p.cur.at_end())
                    .then(|| pattern(p, Restrictions::empty()).ok())
                    .flatten();
            AnyArrayPatternElement::Rest(p.alloc(Mark::of(dots), RestPattern { dots, pattern }))
        } else if at_pattern_start(p.cur) {
            AnyArrayPatternElement::Pattern(pattern(p, Restrictions::empty()).ok()?)
        } else {
            p.unexpected("pattern");
            return None;
        };
        let sep = p.cur.eat::<punct::Comma>();
        let missing_sep = sep
            .is_none()
            .then_some("',' or ']' after array pattern element");
        Some((SepItem { item: element, sep }, missing_sep))
    });
    let elements = p.out.alloc_list(elements);
    let r = p.expect::<punct::RBracket>();
    let ascription = p.cur.eat::<punct::Colon>().map(|colon| {
        let ty = any_type(p);
        reject_unmatched_greater(p);
        p.alloc(
            Mark::of(colon),
            TypeAscription {
                colon: Some(colon),
                ty,
            },
        )
    });
    p.alloc(
        Mark::of(l),
        ArrayPattern {
            l,
            elements,
            r,
            ascription,
        },
    )
}
