//! Type expressions: `int`, `map<K, V>`, `T?`, `T[]`, `A | B`, `(a: A) -> R`,
//! `(Base as Iface).Item`, literal types. Modifiers and unions are nodes,
//! built around the operand they apply to.
//!
//! Nothing crosses function boundaries. A type is parsed greedily (a function
//! type binds looser than `|`), and E0175, the one diagnostic that depends on
//! a construct's surroundings, is decided afterwards from the finished tree by
//! [`report_ambiguous_unions`].

use baml_syntax::{
    Token,
    nodes::{
        AnyLiteralTypeValue, AnyType, AnyTypeArg, Arenas, ArrayType, AssocTypeBinding,
        BacktickString, FunctionType, FunctionTypeParam, FunctionTypeParamName, LiteralType,
        NodeKind, OptionalType, ParenType, PathType, QualifiedPath, ThrowsClause, TypeArgs,
        UnionArm, UnionType,
    },
    token::{
        Ident, KindSet, TokenIdx, TokenKind, TokenSet as _, ctx, kw, lit, punct,
        sets::{LiteralTypeToken, NameToken, StringToken},
    },
    tree::{AnyNode as _, AnyNodeId, Bogus, Id, Missing, SepItem, TokenRange, Visitor},
};

use crate::{
    cursor::{Cursor, Mark},
    error::ParseError,
    expr::{Restrictions, at_expression_start, expr},
    function_union::{self, SigToken},
    items::{StringSite, path_segments, string_token_diagnostics},
    parser::{Parse, Parser},
};

/// Token kinds legal as a `.member` name: names plus the declaration keywords
/// a declaration may use as a member name.
pub(crate) const MEMBER_NAME: KindSet = Ident::KINDS.union(KindSet::new(&[
    TokenKind::Class,
    TokenKind::Enum,
    TokenKind::Function,
    TokenKind::Implements,
    TokenKind::Implement,
    TokenKind::Extends,
    TokenKind::Requires,
    TokenKind::Interface,
    TokenKind::Client,
]));
const _: () = assert!(MEMBER_NAME.is_subset(NameToken::KINDS));

use crate::lookahead::PATH_SEGMENT;
const _: () = assert!(PATH_SEGMENT.is_subset(NameToken::KINDS));

const NUMBER: KindSet = KindSet::new(&[TokenKind::Bigint, TokenKind::Integer, TokenKind::Float]);

/// What starts a type besides a name, a string or a number.
const TYPE_START_PUNCT: KindSet = KindSet::new(&[TokenKind::LParen, TokenKind::Less]);

/// After a type at depth zero, where a `>` closes nothing: each extra one is
/// skipped and all of them are reported once, at the first.
pub(crate) fn reject_unmatched_greater(p: &mut Parser<'_, '_>) {
    let mark = p.cur.mark();
    let mut extra = 0usize;
    while p.cur.at(TokenKind::Greater) {
        p.cur.skip();
        extra += 1;
    }
    if extra > 0 {
        let tokens = p.cur.range_from(mark);
        let range = p.cur.text_range(tokens);
        p.error(
            format!("unmatched `>` in type expression (found {extra} extra)"),
            range,
        );
    }
}

/// Whether the current token can start a type.
pub(crate) fn is_at_type_start(cur: Cursor<'_>) -> bool {
    cur.at_ident()
        || cur.at_in::<StringToken>()
        || cur.at_set(NUMBER)
        || cur.at_set(TYPE_START_PUNCT)
        || (cur.at(TokenKind::Minus) && cur.kind(1).is_some_and(|k| NUMBER.contains(k)))
}

/// The start token of a parsed type, for nodes wrapping it.
fn type_mark(p: &Parser<'_, '_>, ty: AnyType) -> Mark {
    let id = ty
        .any_node_id()
        .unwrap_or_else(|| unreachable!("every `AnyType` alternative is a node"));
    Mark::at(p.out.span(id).start())
}

/// A type expression; `Err` when nothing here starts one. The entry point
/// from outside this module: once the whole type is built, its ambiguous
/// function/union boundaries are reported (E0175).
pub(crate) fn any_type(p: &mut Parser<'_, '_>) -> Result<AnyType, Missing> {
    let ty = type_expr(p)?;
    report_ambiguous_unions(p, ty);
    Ok(ty)
}

/// One union operand with its modifiers, for a position whose own `|` means
/// something else (a pattern alternative). A function type's return and
/// `throws` unions are still its own, and reported as ambiguous where they
/// are.
pub(crate) fn type_operand(p: &mut Parser<'_, '_>) -> Result<AnyType, Missing> {
    let ty = type_postfix(p)?;
    report_ambiguous_unions(p, ty);
    Ok(ty)
}

/// A type expression: one union operand, or several joined by `|`. Within
/// this module every type is parsed through here; [`any_type`] adds the
/// post-parse report.
fn type_expr(p: &mut Parser<'_, '_>) -> Result<AnyType, Missing> {
    let first = type_postfix(p)?;
    if !p.cur.at(TokenKind::Pipe) {
        return Ok(first);
    }
    let mark = type_mark(p, first);
    let mut arms = Vec::new();
    while let Some(pipe) = p.cur.eat::<punct::Pipe>() {
        let ty = type_postfix(p);
        let ended = ty.is_err();
        arms.push(UnionArm { pipe, ty });
        if ended {
            // The union ends on its trailing pipe; the member is reported.
            break;
        }
    }
    let mut arms = arms.into_iter();
    let first_arm = arms
        .next()
        .unwrap_or_else(|| unreachable!("entered at a pipe"));
    let arms = p.out.alloc_list1(first_arm, arms.collect());
    Ok(AnyType::Union(p.alloc(mark, UnionType { first, arms })))
}

/// One union operand with its postfix modifiers: `T`, `T?`, `T[]`, `T[]?`.
fn type_postfix(p: &mut Parser<'_, '_>) -> Result<AnyType, Missing> {
    let mut ty = type_primary(p)?;
    let mark = type_mark(p, ty);
    loop {
        if let Some(l) = p.cur.eat::<punct::LBracket>() {
            let r = p.expect::<punct::RBracket>();
            ty = AnyType::Array(p.alloc(mark, ArrayType { inner: ty, l, r }));
        } else if let Some(q) = p.cur.eat::<punct::Question>() {
            ty = AnyType::Optional(p.alloc(mark, OptionalType { inner: ty, q }));
        } else {
            return Ok(ty);
        }
    }
}

fn type_primary(p: &mut Parser<'_, '_>) -> Result<AnyType, Missing> {
    // The removed `unreflect(expr)` still parses as a type atom so it recovers
    // cleanly: E0098 points at it, and the position is `Bogus`.
    if p.cur.at(TokenKind::Unreflect) && p.cur.nth_is(1, TokenKind::LParen) {
        return Ok(AnyType::Bogus(removed_unreflect(p)));
    }

    // Function values cannot declare their own generic parameters, so a
    // leading `<...>` on a function type is rejected; the list is skipped and
    // the `(...) -> R` parsed so the rest of the file still parses.
    if p.cur.at(TokenKind::Less) {
        p.error_here("a function type cannot declare generic parameters");
        skip_angle_group(&mut p.cur);
        if let Some(l) = p.cur.eat::<punct::LParen>() {
            return Ok(paren_group(p, l));
        }
        return Err(p.missing("function type parameter list"));
    }

    if let Some(token) = p.cur.eat_in::<LiteralTypeToken>() {
        let mark = Mark::of(token);
        let kind = p.cur.tokens().kind(token.idx);
        if StringToken::KINDS.contains(kind) {
            string_token_diagnostics(p, token.idx, StringSite::Literal);
        } else if kind == TokenKind::Float {
            reject_float_literal_type(p, "", token.idx);
        }
        return Ok(AnyType::Literal(p.alloc(
            mark,
            LiteralType {
                minus: None,
                value: AnyLiteralTypeValue::Scalar(token),
            },
        )));
    }
    if let Some(open) = p.cur.eat::<lit::BacktickStart>() {
        let value = AnyLiteralTypeValue::Backtick(BacktickString::parse(p, open));
        return Ok(AnyType::Literal(
            p.alloc(Mark::of(open), LiteralType { minus: None, value }),
        ));
    }

    // Negative numeric literal type: `-42`. Floats still error, as the
    // positive case does.
    if p.cur.at(TokenKind::Minus)
        && p.cur.kind(1).is_some_and(|k| NUMBER.contains(k))
        && let Some(minus) = p.cur.eat::<punct::Minus>()
        && let Some(number) = p.cur.eat_set::<LiteralTypeToken>(NUMBER)
    {
        if p.cur.tokens().kind(number.idx) == TokenKind::Float {
            reject_float_literal_type(p, "-", number.idx);
        }
        let value = AnyLiteralTypeValue::Scalar(number);
        return Ok(AnyType::Literal(p.alloc(
            Mark::of(minus),
            LiteralType {
                minus: Some(minus),
                value,
            },
        )));
    }

    if let Some(head) = p.cur.eat_set::<NameToken>(Ident::KINDS) {
        return Ok(AnyType::Path(path_type(p, head)));
    }
    if let Some(l) = p.cur.eat::<punct::LParen>() {
        return Ok(paren_group(p, l));
    }
    Err(p.missing("type"))
}

/// "float literal values are not supported" for the float token `idx`;
/// `sign` prefixes the quoted text.
fn reject_float_literal_type(p: &mut Parser<'_, '_>, sign: &str, idx: TokenIdx) {
    let text = p.cur.text_of(idx);
    let range = p.cur.token_range(idx);
    p.error(
        format!("float literal values are not supported: {sign}{text}"),
        range,
    );
}

/// `segment<Args>` after the path's first name.
pub(crate) fn path_type(p: &mut Parser<'_, '_>, head: Token![NameToken]) -> Id<PathType> {
    let mark = Mark::of(head);
    let head = p.name(head);
    let segments = path_segments(p, |p| {
        p.expect_name(PATH_SEGMENT, "type name segment after '.'")
    });
    // After a dangling `.` nothing more is read for this path.
    let complete = segments.last().is_none_or(|segment| segment.name.is_ok());
    let segments = p.out.alloc_list(segments);
    let args = if complete && let Some(l) = p.cur.eat::<punct::Less>() {
        Some(TypeArgs::parse(p, l))
    } else {
        None
    };
    p.alloc(
        mark,
        PathType {
            head,
            segments,
            args,
        },
    )
}

/// `T, Item = U>` on a type, after its `<`.
impl Parse for TypeArgs {
    type Head = Token!["<"];
    fn parse(p: &mut Parser<'_, '_>, l: Self::Head) -> Id<Self> {
        if p.cur.at(TokenKind::Greater) {
            p.unexpected("type");
        }
        let args = p.sep_list(TokenKind::Greater, type_arg_or_assoc_binding);
        let r = p.expect::<punct::Greater>();
        p.alloc(Mark::of(l), TypeArgs { l, args, r })
    }
}

/// `None` (reported) when nothing here starts a type argument.
fn type_arg_or_assoc_binding(p: &mut Parser<'_, '_>) -> Option<AnyTypeArg> {
    // `Iterator<Item = int>`: the name is the associated type bound.
    if p.cur.at_ident()
        && p.cur.nth_is(1, TokenKind::Equals)
        && let Some(name) = p.eat_name(Ident::KINDS)
        && let Some(eq) = p.cur.eat::<punct::Equals>()
    {
        let ty = type_expr(p);
        let mark = p.mark_of(AnyNodeId::of(name));
        return Some(AnyTypeArg::Binding(
            p.alloc(mark, AssocTypeBinding { name, eq, ty }),
        ));
    }
    type_expr(p).ok().map(AnyTypeArg::Type)
}

/// One element of a parenthesized group, before the group's kind is known.
struct GroupElement {
    mark: Mark,
    end: TokenIdx,
    name: Option<FunctionTypeParamName>,
    ty: Result<AnyType, Missing>,
    sep: Option<Token![","]>,
}

/// After a `(`: a parenthesized type `(A | B)`, a function type
/// `(a: A, B) -> R throws E`, or a qualified projection `(Base as Iface).Item`.
/// The group is parsed once and the wrapper is decided by what is actually
/// there: `as` after a first unnamed type makes it a projection, `->` after
/// the `)` a function type, a single unnamed element a parenthesized type;
/// anything else is an error. No lookahead decides anything a later consume
/// has to trust.
fn paren_group(p: &mut Parser<'_, '_>, l: Token!["("]) -> AnyType {
    let mark = Mark::of(l);
    match p.attempt(|p| paren_group_contents(p, l)) {
        Ok(ty) => ty,
        Err(error) => AnyType::Bogus(p.bogus_since(mark, error)),
    }
}

/// The group opened by `l` as the node it turns out to be, or `Err` with the
/// diagnostic when it is none of them: its elements were parsed for their own
/// diagnostics and are discarded.
fn paren_group_contents(p: &mut Parser<'_, '_>, l: Token!["("]) -> Result<AnyType, ParseError> {
    let mark = Mark::of(l);

    let mut elements: Vec<GroupElement> = Vec::new();
    let mut had_named = false;
    if !p.cur.at(TokenKind::RParen) {
        loop {
            let element_mark = p.cur.mark();
            let name = function_type_param_name(p);
            let ty = type_expr(p);
            if elements.is_empty()
                && name.is_none()
                && let Ok(base) = ty
                && let Some(as_kw) = p.cur.eat::<ctx::As>()
            {
                return Ok(AnyType::Qualified(qualified_projection(p, l, base, as_kw)));
            }
            had_named |= name.is_some();
            if name.is_some() {
                reject_function_type_default(p);
            }
            let end = p.cur.end();
            let sep = p.cur.eat::<punct::Comma>();
            let more = sep.is_some() && !p.cur.at(TokenKind::RParen) && !p.cur.at_end();
            elements.push(GroupElement {
                mark: element_mark,
                end,
                name,
                ty,
                sep,
            });
            if !more {
                break;
            }
        }
    }
    recover_to_paren_close(p);
    let r = p.expect::<punct::RParen>();

    if let Some(arrow) = p.cur.eat::<punct::Arrow>() {
        let params: Vec<SepItem<Id<FunctionTypeParam>, punct::Comma>> = elements
            .into_iter()
            .map(|element| SepItem {
                item: p.alloc_to(
                    element.mark,
                    element.end,
                    FunctionTypeParam {
                        name: element.name,
                        ty: element.ty,
                    },
                ),
                sep: element.sep,
            })
            .collect();
        let params = p.out.alloc_list(params);
        // Bare return and `throws` unions are owned by the function. A pipe
        // followed by another function head is reported as ambiguous once the
        // whole type is built, instead of silently choosing this grouping.
        let ret = type_expr(p);
        let throws = p.cur.eat::<kw::Throws>().map(|kw| throws_clause(p, kw));
        return Ok(AnyType::Function(p.alloc(
            mark,
            FunctionType {
                l,
                params,
                r,
                arrow,
                ret,
                throws,
            },
        )));
    }

    if let [element] = elements.as_slice()
        && element.name.is_none()
        && element.sep.is_none()
    {
        let inner = element.ty;
        return Ok(AnyType::Paren(p.alloc(mark, ParenType { l, inner, r })));
    }

    // Named parameters are only legal in a function type; a group of several
    // types is neither a type nor a tuple. The element types were parsed for
    // their diagnostics; the group itself is recovered as one `Bogus`.
    let message = if had_named {
        "Named parameters in type expression require `->` to form a function type"
    } else {
        "a parenthesized type holds one type; a parameter list needs `->` to form a function type"
    };
    let error = ParseError::InvalidSyntax {
        message: message.to_string(),
        range: p.cur.error_range(),
    };
    Err(error)
}

/// `Base as Interface).member` in expression position after its `(`, the
/// lookahead having seen the shape: the group is the projection's. The
/// shape the lookahead read is read again here with `eat`/`expect`, so a
/// disagreement is a diagnostic, never a wrong node.
pub(crate) fn qualified_path_expr(p: &mut Parser<'_, '_>, l: Token!["("]) -> Id<QualifiedPath> {
    let Ok(base) = type_expr(p) else {
        unreachable!("the lookahead saw a type before `as`")
    };
    let Some(as_kw) = p.cur.eat::<ctx::As>() else {
        unreachable!("the lookahead saw `as` after the type")
    };
    qualified_projection(p, l, base, as_kw)
}

/// The rest of `(Base as Interface).Member` once `base` and the `as` are
/// consumed. One node serves type position (an associated type) and
/// expression position (a qualified item): same spelling, same structure.
fn qualified_projection(
    p: &mut Parser<'_, '_>,
    l: Token!["("],
    base: AnyType,
    as_kw: Token!["as"],
) -> Id<QualifiedPath> {
    let iface = type_expr(p);
    let r = p.expect::<punct::RParen>();
    let dot = p.expect::<punct::Dot>();
    let member = p.expect_name(MEMBER_NAME, "projected item name");
    p.alloc(
        Mark::of(l),
        QualifiedPath {
            l,
            base,
            as_kw,
            iface,
            r,
            dot,
            member,
        },
    )
}

/// `E` after a function type's `throws`: the operand is part of the type
/// being built.
fn throws_clause(p: &mut Parser<'_, '_>, kw: Token!["throws"]) -> Id<ThrowsClause> {
    let ty = type_expr(p);
    p.alloc(Mark::of(kw), ThrowsClause { kw, ty })
}

/// `E` after a declaration's `throws`: a type expression of its own.
impl Parse for ThrowsClause {
    type Head = Token!["throws"];
    fn parse(p: &mut Parser<'_, '_>, kw: Self::Head) -> Id<Self> {
        let ty = any_type(p);
        p.alloc(Mark::of(kw), ThrowsClause { kw, ty })
    }
}

/// `name:` / `name?:` opening a named function-type parameter, if the next
/// tokens have that shape. The shape is read with `eat` after the look, so a
/// disagreement is a diagnostic, never a wrong node.
fn function_type_param_name(p: &mut Parser<'_, '_>) -> Option<FunctionTypeParamName> {
    let named = p.cur.at_set(crate::expr::EXPR_NAME)
        && (p.cur.nth_is(1, TokenKind::Colon)
            || (p.cur.nth_is(1, TokenKind::Question) && p.cur.nth_is(2, TokenKind::Colon)));
    if !named {
        return None;
    }
    crate::items::reject_reserved_trace_parameter(p);
    let name = p.eat_name(crate::expr::EXPR_NAME)?;
    let question = p.cur.eat::<punct::Question>();
    let Some(colon) = p.cur.eat::<punct::Colon>() else {
        unreachable!("the shape was checked: a colon follows")
    };
    Some(FunctionTypeParamName {
        name,
        question,
        colon,
    })
}

/// A `= default` after a named function-type parameter: consumed and
/// rejected; the tokens stay unowned, the expression's own diagnostics stay.
fn reject_function_type_default(p: &mut Parser<'_, '_>) {
    if !p.cur.at(TokenKind::Equals) {
        return;
    }
    p.cur.skip();
    let default = p.cur.mark();
    if at_expression_start(p.cur) {
        p.discard(|p| {
            let _rejected = expr(p, Restrictions::empty());
        });
    }
    let range = p.cur.text_range(p.cur.range_from(default));
    p.error(
        "default expressions are not allowed in function types",
        range,
    );
}

/// If not at `)`: report the stray token and skip to the balancing `)` (left
/// for the caller), stopping at a `}` or an unbalanced `]`.
fn recover_to_paren_close(p: &mut Parser<'_, '_>) {
    if p.cur.at(TokenKind::RParen) {
        return;
    }
    if let Some(idx) = p.cur.nth(0) {
        let message = format!("Unexpected '{}' in type expression", p.cur.text_of(idx));
        let range = p.cur.token_range(idx);
        p.error(message, range);
    }
    let mut paren_depth = 1u32;
    let mut bracket_depth = 0u32;
    while let Some(kind) = p.cur.kind(0) {
        match kind {
            TokenKind::LParen => paren_depth += 1,
            TokenKind::RParen => {
                paren_depth -= 1;
                if paren_depth == 0 {
                    return;
                }
            }
            TokenKind::LBracket => bracket_depth += 1,
            TokenKind::RBracket => {
                if bracket_depth == 0 {
                    return;
                }
                bracket_depth -= 1;
            }
            TokenKind::RBrace => return,
            _ => {}
        }
        p.cur.skip();
    }
}

/// Skip a `<…>` group (a rejected generic parameter list), never past the
/// bracket or separator that encloses it.
fn skip_angle_group(cur: &mut Cursor<'_>) {
    debug_assert!(cur.at(TokenKind::Less));
    let mut depth = 0u32;
    while let Some(kind) = cur.kind(0) {
        match kind {
            TokenKind::Less => depth += 1,
            TokenKind::Greater => {
                depth -= 1;
                cur.skip();
                if depth == 0 {
                    return;
                }
                continue;
            }
            TokenKind::LParen
            | TokenKind::RParen
            | TokenKind::LBrace
            | TokenKind::RBrace
            | TokenKind::RBracket
            | TokenKind::Comma => return,
            _ => {}
        }
        cur.skip();
    }
}

/// The removed `unreflect(expr)` type atom: E0098, consumed as a `Bogus`.
/// The operand is parsed for its own diagnostics and discarded.
fn removed_unreflect(p: &mut Parser<'_, '_>) -> Id<Bogus> {
    let mark = p.cur.mark();
    let error = ParseError::RemovedFeature {
        message: "`unreflect` was removed; bind the runtime type first with `let type T = <expr>;` and write `T` here".to_string(),
        range: p.cur.error_range(),
    };
    p.cur.skip(); // unreflect
    p.cur.skip(); // (
    if p.cur.at(TokenKind::RParen) {
        p.error_here("`unreflect` requires an operand");
    } else if at_expression_start(p.cur) {
        p.discard(|p| {
            let _operand = expr(p, Restrictions::empty());
        });
    }
    if p.cur.at(TokenKind::RParen) {
        p.cur.skip();
    } else {
        p.unexpected("')'");
    }
    p.bogus_since(mark, error)
}

// ----- ambiguous function/union boundaries (E0175) ---------------------------------

/// Report every pipe in `root` that could belong to two owners.
///
/// A function type is greedy and binds looser than `|`, so `(A) -> B | C` is
/// `(A) -> (B | C)`. When the member after a pipe in a function's return or
/// `throws` operand is itself a function type, `(A) -> B | (C) -> D`, the
/// other reading is just as complete, and the user is asked to parenthesize
/// one of them. The question is answered from the finished tree: a union is
/// an operand when it is a function type's return or `throws` type; the
/// function-typed members of an operand are undelimited, so their own
/// operands belong to the same outermost function — the *owner*, whose text
/// the two groupings are rendered for. Parentheses, type arguments and
/// parameter lists start a fresh owner. Only a syntax-complete member counts:
/// an incomplete one already has its diagnostic.
fn report_ambiguous_unions(p: &mut Parser<'_, '_>, root: AnyType) {
    let mut by_owner: Vec<(Id<FunctionType>, Vec<TokenIdx>)> = Vec::new();
    collect_ambiguous_pipes(p, root, None, &mut by_owner);
    for (owner, mut pipes) in by_owner {
        pipes.sort_unstable();
        let span = p.out.node_span(owner);
        render_ambiguous_unions(p, span, &pipes);
    }
}

/// The ambiguous pipes under `ty`, grouped by owner. `owner` is the function
/// whose operand territory `ty` is in, if any.
fn collect_ambiguous_pipes(
    p: &Parser<'_, '_>,
    ty: AnyType,
    owner: Option<Id<FunctionType>>,
    out: &mut Vec<(Id<FunctionType>, Vec<TokenIdx>)>,
) {
    match ty {
        AnyType::Literal(_) | AnyType::Bogus(_) => {}
        AnyType::Path(id) => {
            if let Some(args) = p.out.get(id).args {
                for arg in p.out.list(p.out.get(args).args) {
                    match arg.item {
                        AnyTypeArg::Type(ty) => collect_ambiguous_pipes(p, ty, None, out),
                        AnyTypeArg::Binding(binding) => {
                            if let Ok(ty) = p.out.get(binding).ty {
                                collect_ambiguous_pipes(p, ty, None, out);
                            }
                        }
                        AnyTypeArg::Bogus(_) => {}
                    }
                }
            }
        }
        AnyType::Paren(id) => {
            if let Ok(inner) = p.out.get(id).inner {
                collect_ambiguous_pipes(p, inner, None, out);
            }
        }
        AnyType::Qualified(id) => {
            let projection = p.out.get(id);
            collect_ambiguous_pipes(p, projection.base, None, out);
            if let Ok(iface) = projection.iface {
                collect_ambiguous_pipes(p, iface, None, out);
            }
        }
        AnyType::Optional(id) => collect_ambiguous_pipes(p, p.out.get(id).inner, owner, out),
        AnyType::Array(id) => collect_ambiguous_pipes(p, p.out.get(id).inner, owner, out),
        AnyType::Union(id) => {
            let union = *p.out.get(id);
            collect_ambiguous_pipes(p, union.first, owner, out);
            for arm in p.out.list(union.arms.as_list()) {
                if let Ok(ty) = arm.ty {
                    collect_ambiguous_pipes(p, ty, owner, out);
                }
            }
        }
        AnyType::Function(id) => {
            let function = *p.out.get(id);
            for param in p.out.list(function.params) {
                if let Ok(ty) = p.out.get(param.item).ty {
                    collect_ambiguous_pipes(p, ty, None, out);
                }
            }
            let owner = owner.unwrap_or(id);
            if let Ok(ret) = function.ret {
                collect_operand_pipes(p, ret, owner, out);
            }
            if let Some(throws) = function.throws
                && let Ok(ty) = p.out.get(throws).ty
            {
                collect_operand_pipes(p, ty, owner, out);
            }
        }
    }
}

/// A function's return or `throws` operand: a pipe followed by a function
/// type is ambiguous, and the whole operand is `owner`'s territory.
fn collect_operand_pipes(
    p: &Parser<'_, '_>,
    operand: AnyType,
    owner: Id<FunctionType>,
    out: &mut Vec<(Id<FunctionType>, Vec<TokenIdx>)>,
) {
    if let AnyType::Union(id) = operand {
        for arm in p.out.list(p.out.get(id).arms.as_list()) {
            if let Ok(ty) = arm.ty
                && is_function_shaped(p, ty)
                && is_complete(p, ty)
            {
                match out.iter_mut().find(|(candidate, _)| *candidate == owner) {
                    Some((_, pipes)) => pipes.push(arm.pipe.idx),
                    None => out.push((owner, vec![arm.pipe.idx])),
                }
            }
        }
    }
    collect_ambiguous_pipes(p, operand, Some(owner), out);
}

/// Whether `ty` reads as a function type from the outside: one, or one under
/// postfix modifiers or whole-type parentheses, which do not settle whom a
/// pipe before it belongs to.
fn is_function_shaped(p: &Parser<'_, '_>, mut ty: AnyType) -> bool {
    loop {
        ty = match ty {
            AnyType::Function(_) => return true,
            AnyType::Optional(id) => p.out.get(id).inner,
            AnyType::Array(id) => p.out.get(id).inner,
            AnyType::Paren(id) => match p.out.get(id).inner {
                Ok(inner) => inner,
                Err(_) => return false,
            },
            AnyType::Path(_)
            | AnyType::Literal(_)
            | AnyType::Union(_)
            | AnyType::Qualified(_)
            | AnyType::Bogus(_) => return false,
        };
    }
}

/// Whether the tree under `ty` holds no `Missing` and no `Bogus`.
fn is_complete(p: &Parser<'_, '_>, ty: AnyType) -> bool {
    struct Completeness {
        complete: bool,
        pending: Vec<AnyNodeId>,
    }
    impl Visitor for Completeness {
        fn node(&mut self, _field: &'static str, id: AnyNodeId) {
            if Arenas::kind(id) == NodeKind::Bogus {
                self.complete = false;
            } else {
                self.pending.push(id);
            }
        }
        fn token(&mut self, _field: &'static str, _token: TokenIdx) {}
        fn range(&mut self, _field: &'static str, _range: TokenRange) {}
        fn missing(&mut self, _field: &'static str, _missing: Missing) {
            self.complete = false;
        }
    }
    if matches!(ty, AnyType::Bogus(_)) {
        return false;
    }
    let root = ty
        .any_node_id()
        .unwrap_or_else(|| unreachable!("every `AnyType` alternative is a node"));
    let arenas = p.out.arenas();
    let mut walk = Completeness {
        complete: true,
        pending: vec![root],
    };
    while let Some(id) = walk.pending.pop() {
        arenas.walk_node(id, &mut walk);
        if !walk.complete {
            return false;
        }
    }
    true
}

/// E0175 for each of `pipes`, with two explicit groupings of the owner
/// function type spanning `span`.
fn render_ambiguous_unions(p: &mut Parser<'_, '_>, span: TokenRange, pipes: &[TokenIdx]) {
    let cur = &p.cur;
    let range = cur.sig_range(span);
    let idx = |i: usize| {
        cur.sig(i)
            .unwrap_or_else(|| unreachable!("`span` is consumed tokens"))
    };
    let sig: Vec<SigToken<'_>> = range
        .clone()
        .map(|i| SigToken {
            kind: cur
                .kind_at_sig(i)
                .unwrap_or_else(|| unreachable!("consumed token")),
            text: cur.text_of(idx(i)),
            space_before: i > range.start && idx(i - 1).0 + 1 != idx(i).0,
        })
        .collect();
    let pipe_positions: Vec<usize> = pipes
        .iter()
        .map(|&pipe| cur.sig_index(pipe) - range.start)
        .collect();
    let groupings = function_union::groupings(&sig, &pipe_positions);
    let errors: Vec<ParseError> = pipes
        .iter()
        .zip(groupings)
        .map(|(&pipe, groupings)| ParseError::AmbiguousUnion {
            range: cur.token_range(pipe),
            groupings: groupings.map(Vec::from).unwrap_or_default(),
        })
        .collect();
    for error in errors {
        p.out.report(error);
    }
}
