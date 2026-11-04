//! Expressions: a Pratt loop over [`BinaryOp`]'s table, with the postfix
//! forms (calls, members, indexing, `catch`, object literals) handled in the
//! same loop, and the primaries. Every operand is a node.
//!
//! How tokens may be read in a position arrives as [`Restrictions`], a
//! parameter: whether a line break can end the expression, whether a `{`
//! after it belongs to the enclosing construct, whether a postfix `catch`
//! does. A delimiter clears them all.
//!
//! [`BinaryOp`]: baml_syntax::ops::BinaryOp

use baml_syntax::{
    Token,
    nodes::{
        AnyElse, AnyExpr, AnyMatchScrutinee, AnyObjectCtor, AnyObjectKey, AnyObjectMember, Arg,
        ArgList, ArrayElement, ArrayLiteral, AwaitExpr, BacktickString, BinaryExpr, BreakExpr,
        CallExpr, CatchArm, CatchClause, CatchExpr, ContinueExpr, ElseBranch, EnvAccessExpr,
        FieldAccessExpr, FieldValue, GenericApplyExpr, GenericArgs, GenericParamList, IfExpr,
        IfLetExpr, IndexExpr, IsExpr, LambdaExpr, LiteralExpr, MapEntry, MapLiteral, MatchArm,
        MatchExpr, MatchGuard, MatchParen, ObjectField, ObjectLiteral, OptionalCallExpr,
        OptionalFieldAccessExpr, OptionalIndexExpr, ParenExpr, PathExpr, PathSegment, ReturnExpr,
        SpawnExpr, SpecExpr, SpreadElement, TaggedTemplateExpr, ThrowExpr, ThrowsClause,
        TraceBinding, TypeAscription, UnaryExpr, UpcastExpr, WithClause,
    },
    ops::{AssignOp, BinaryOp, IS_BINDING_POWER, PREFIX_BINDING_POWER, UnaryOpToken},
    token::{
        Ident, KindSet, TokenKind, TokenSet as _, ctx, kw, punct,
        sets::{
            AnyArrow, CatchKw, LetIntroducer, LiteralExprToken, MemberDot, NameToken, SpecSelector,
            StringToken,
        },
    },
    tree::{AnyNodeId, Bogus, Id, Missing},
};

use crate::{
    cursor::{Cursor, Mark},
    error::ParseError,
    items::{Annotation, StringSite, dotted_name, param_list, string_lit},
    lookahead::{
        PATH_SEGMENT, at_binding_intro, at_catch_clause_start, brace_literal_before_body,
        brace_looks_like_fields, looks_like_as_projection, looks_like_generic_args,
        looks_like_generic_lambda, looks_like_lambda, looks_like_map,
        looks_like_qualified_projection,
    },
    parser::{Parse, Parser},
    patterns::{pattern, pattern_after_introducer},
    stmts::{block, body_block},
    types::{MEMBER_NAME, any_type, qualified_path_expr},
};

bitflags::bitflags! {
    /// How the tokens of an expression may be read in the position it is
    /// parsed in. Not where the parser is: what the position permits.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct Restrictions: u8 {
        /// The position's items are separated by lines (a block's
        /// statements, a `match`'s arms, the file's items): a line break
        /// ends the expression when what follows could begin another one.
        /// Inside a delimiter a line break is whitespace.
        const LINE_SEPARATED = 1 << 0;
        /// A `{` after the expression opens the enclosing construct's body
        /// (`if c {`, `while c {`, `match x {`, `spawn n {`, `for … in xs {`).
        /// An object literal or a class destructure is taken there only when
        /// its matching `}` is followed by something that continues the
        /// expression, or by the body's own `{`.
        const BRACE_IS_BODY = 1 << 1;
        /// A postfix `catch` belongs to the prefix form around this
        /// expression (`throw x catch …` is `(throw x) catch …`), not to it.
        const NO_CATCH = 1 << 2;
    }
}

impl Restrictions {
    /// Statement position: a line break can end the expression.
    pub(crate) const STATEMENT: Self = Self::LINE_SEPARATED;

    /// The same position, with a `{` after the expression reserved for the
    /// enclosing construct's body.
    pub(crate) fn before_body(self) -> Self {
        self | Self::BRACE_IS_BODY
    }

    /// Whether the brace literal opening at significant index `open` may be
    /// taken here.
    fn brace_literal_allowed(self, cur: Cursor<'_>, open: usize) -> bool {
        !self.contains(Self::BRACE_IS_BODY) || brace_literal_before_body(cur, open)
    }
}

const NUMBER: KindSet = KindSet::new(&[TokenKind::Bigint, TokenKind::Integer, TokenKind::Float]);

/// A name in expression position: a word, a contextual keyword, or `client`.
pub(crate) const EXPR_NAME: KindSet = Ident::KINDS.insert(TokenKind::Client);
const _: () = assert!(EXPR_NAME.is_subset(NameToken::KINDS));

/// Every token that can begin an expression. `<` only begins a generic
/// lambda; see [`at_expression_start`].
pub(crate) const EXPR_START: KindSet =
    EXPR_NAME
        .union(StringToken::KINDS)
        .union(NUMBER)
        .union(KindSet::new(&[
            TokenKind::BacktickStart,
            TokenKind::LParen,
            TokenKind::LBracket,
            TokenKind::LBrace,
            TokenKind::If,
            TokenKind::Match,
            TokenKind::Throw,
            TokenKind::Return,
            TokenKind::Break,
            TokenKind::Continue,
            TokenKind::Less,
            TokenKind::Minus,
            TokenKind::Not,
            TokenKind::Tilde,
            TokenKind::PlusPlus,
            TokenKind::MinusMinus,
            TokenKind::Await,
            TokenKind::Spawn,
        ]));

/// Item keywords no statement can begin with: inside a block or an arm list
/// one of these means the `}` was forgotten. `client` begins a path, `test`
/// and `testset` a nested test.
pub(crate) const ITEM_KEYWORD: KindSet = KindSet::new(&[
    TokenKind::Class,
    TokenKind::Enum,
    TokenKind::Interface,
    TokenKind::Function,
    TokenKind::Implements,
    TokenKind::Implement,
    TokenKind::Generator,
    TokenKind::RetryPolicy,
    TokenKind::TemplateString,
]);

/// Statement keywords: inside a brace literal one of these means the `}`
/// was forgotten.
pub(crate) const STATEMENT_KEYWORD: KindSet = ITEM_KEYWORD.union(KindSet::new(&[
    TokenKind::Let,
    TokenKind::Return,
    TokenKind::While,
    TokenKind::For,
    TokenKind::Break,
    TokenKind::Continue,
    TokenKind::Throw,
    TokenKind::Defer,
]));

/// Whether an expression can begin at the cursor. A `<` does only as a
/// generic lambda (otherwise it is a comparison continuing the expression
/// before it), and a `catch_all_panics` opens a clause, not an expression.
pub(crate) fn at_expression_start(cur: Cursor<'_>) -> bool {
    cur.at_set(EXPR_START)
        && (!cur.at(TokenKind::Less) || looks_like_generic_lambda(cur))
        && !at_catch_clause_start(cur)
}

/// An expression, or a reported `Missing` when nothing here begins one. An
/// assignment operator after it is an error here: an assignment is a
/// statement, which only [`stmt_expr`]'s callers read.
pub(crate) fn expr(p: &mut Parser<'_, '_>, r: Restrictions) -> Result<AnyExpr, Missing> {
    let expr = expr_bp(p, 0, r)?;
    reject_assignment(p, expr, r);
    Ok(expr)
}

/// An expression at the start of a statement line or of a `for` step, the
/// positions an assignment may stand in: an assignment operator after it is
/// left for the caller, which reads the assignment.
pub(crate) fn stmt_expr(p: &mut Parser<'_, '_>, r: Restrictions) -> Result<AnyExpr, Missing> {
    expr_bp(p, 0, r)
}

/// `(x = 5)`, `f(x = 5)`, `let y = x = 5`: an assignment where only a value
/// may stand. Reported once over the whole assignment; its right-hand side
/// is read for its extent only, so nothing inside it is reported or kept.
fn reject_assignment(p: &mut Parser<'_, '_>, lhs: AnyExpr, r: Restrictions) {
    if !p.cur.at_op::<AssignOp>() {
        return;
    }
    let mark = p.mark_of_any(&lhs);
    p.probe(|p| {
        p.cur.eat_op::<AssignOp>();
        if at_expression_start(p.cur) {
            let _ = expr(p, r);
        }
    });
    let range = p.cur.text_range(p.cur.range_from(mark));
    p.error(
        "assignment is not allowed in expression position; assignment is a statement",
        range,
    );
}

/// An expression whose operators bind at least `min_bp`.
fn expr_bp(p: &mut Parser<'_, '_>, min_bp: u8, r: Restrictions) -> Result<AnyExpr, Missing> {
    let lhs = prefix(p, r)?;
    Ok(infix(p, lhs, min_bp, r))
}

/// The expression continuing `lhs`, an operand the caller parsed: its
/// postfix and infix forms at full binding power.
pub(crate) fn expr_from(p: &mut Parser<'_, '_>, lhs: AnyExpr, r: Restrictions) -> AnyExpr {
    infix(p, lhs, 0, r)
}

/// An expression that must begin here, or a reported `Missing`.
fn operand(p: &mut Parser<'_, '_>, r: Restrictions) -> Result<AnyExpr, Missing> {
    if at_expression_start(p.cur) {
        expr(p, r)
    } else {
        Err(p.missing("expression"))
    }
}

/// The postfix and infix forms after `lhs`, each binding at least `min_bp`.
fn infix(p: &mut Parser<'_, '_>, mut lhs: AnyExpr, min_bp: u8, r: Restrictions) -> AnyExpr {
    loop {
        // A `//#` line is a statement: it ends the expression before it.
        if p.cur.header_ahead() || p.cur.at_end() || line_break_ends_expr(p.cur, r) {
            break;
        }
        let mark = p.mark_of_any(&lhs);
        if !r.contains(Restrictions::NO_CATCH)
            && let Some(kw) = p.cur.eat_in::<CatchKw>()
        {
            lhs = catch_expr(p, lhs, kw);
        } else if p.cur.at(TokenKind::Less)
            && looks_like_generic_args(p.cur)
            && let Some(l) = p.cur.eat::<punct::Less>()
        {
            let args = GenericArgs::parse(p, l);
            lhs = AnyExpr::GenericApply(p.alloc(mark, GenericApplyExpr { base: lhs, args }));
        } else if let Some(l) = p.cur.eat::<punct::LParen>() {
            let args = ArgList::parse(p, l);
            lhs = AnyExpr::Call(p.alloc(mark, CallExpr { callee: lhs, args }));
        } else if p.cur.at(TokenKind::BacktickStart)
            && !p.cur.has_newline_ahead()
            && let Some(open) = p.cur.eat::<baml_syntax::token::lit::BacktickStart>()
        {
            // A template on the next line is a statement of its own.
            let template = BacktickString::parse(p, open);
            lhs = AnyExpr::TaggedTemplate(p.alloc(mark, TaggedTemplateExpr { tag: lhs, template }));
        } else if let Some(l) = p.cur.eat::<punct::LBracket>() {
            let index = operand(p, Restrictions::empty());
            let r = p.expect::<punct::RBracket>();
            lhs = AnyExpr::Index(p.alloc(
                mark,
                IndexExpr {
                    base: lhs,
                    l,
                    index,
                    r,
                },
            ));
        } else if let Some(qdot) = p.cur.eat::<punct::QuestionDot>() {
            lhs = if let Some(l) = p.cur.eat::<punct::LParen>() {
                let args = ArgList::parse(p, l);
                AnyExpr::OptionalCall(p.alloc(
                    mark,
                    OptionalCallExpr {
                        callee: lhs,
                        qdot,
                        args,
                    },
                ))
            } else if let Some(l) = p.cur.eat::<punct::LBracket>() {
                let index = operand(p, Restrictions::empty());
                let r = p.expect::<punct::RBracket>();
                AnyExpr::OptionalIndex(p.alloc(
                    mark,
                    OptionalIndexExpr {
                        base: lhs,
                        qdot,
                        l,
                        index,
                        r,
                    },
                ))
            } else {
                let name = p.expect_name(MEMBER_NAME, "field name, `[`, or `(` after `?.`");
                AnyExpr::OptionalFieldAccess(p.alloc(
                    mark,
                    OptionalFieldAccessExpr {
                        base: lhs,
                        qdot,
                        name,
                    },
                ))
            };
        } else if p.cur.at(TokenKind::At)
            && !p.cur.has_newline_ahead()
            && p.cur.nth_in::<SpecSelector>(1)
            && let Some(at) = p.cur.eat::<punct::At>()
        {
            // `F@spec`: an attribute on the next line is never absorbed.
            let selector = p.expect_in::<SpecSelector>("companion selector");
            lhs = AnyExpr::Spec(p.alloc(
                mark,
                SpecExpr {
                    base: lhs,
                    at,
                    selector,
                },
            ));
        } else if looks_like_as_projection(p.cur)
            && let Some(dot) = p.cur.eat::<punct::Dot>()
            && let Some(as_kw) = p.cur.eat::<ctx::As>()
        {
            let args = match p.cur.eat::<punct::Less>() {
                Some(l) => Ok(GenericArgs::parse(p, l)),
                None => Err(p.missing("'<'")),
            };
            lhs = AnyExpr::Upcast(p.alloc(
                mark,
                UpcastExpr {
                    base: lhs,
                    dot,
                    as_kw,
                    args,
                },
            ));
        } else if let Some(dot) = p.cur.eat_in::<MemberDot>() {
            let expected = if p.cur.tokens().kind(dot.idx) == TokenKind::Dollar {
                "field name after `'$'`"
            } else {
                "field name after `'.'`"
            };
            let name = p.expect_name(MEMBER_NAME, expected);
            lhs = AnyExpr::FieldAccess(p.alloc(
                mark,
                FieldAccessExpr {
                    base: lhs,
                    dot,
                    name,
                },
            ));
        } else if p.cur.at(TokenKind::LBrace) {
            let ctor = match lhs {
                AnyExpr::Path(id) => AnyObjectCtor::Path(id),
                AnyExpr::GenericApply(id) => AnyObjectCtor::Generic(id),
                _ => break,
            };
            let open = p.cur.pos();
            if !(brace_looks_like_fields(p.cur, open) && r.brace_literal_allowed(p.cur, open)) {
                break;
            }
            let Some(l) = p.cur.eat::<punct::LBrace>() else {
                break;
            };
            lhs = AnyExpr::Object(object_literal(p, mark, ctor, l));
        } else if p.cur.at(TokenKind::Is) {
            if IS_BINDING_POWER.0 < min_bp {
                break;
            }
            let Some(is_kw) = p.cur.eat::<kw::Is>() else {
                break;
            };
            let pattern = pattern(p, r);
            lhs = AnyExpr::Is(p.alloc(
                mark,
                IsExpr {
                    expr: lhs,
                    is_kw,
                    pattern,
                },
            ));
        } else if p.cur.at(TokenKind::Not) && !p.cur.has_newline_ahead() {
            // A `!` after a complete expression is a non-null assertion from
            // another language; on the next line it is a prefix operator.
            p.error_here(
                "unexpected '!'; BAML has no non-null assertion operator — unwrap optionals with '?? <default>' or 'if let x: T = opt { ... }'",
            );
            p.cur.skip();
        } else {
            // An assignment operator ends the expression (`>>=` before `>>`).
            if p.cur.at_op::<AssignOp>() {
                break;
            }
            let Some(binding_power) = p
                .cur
                .nth(0)
                .and_then(|idx| BinaryOp::at(p.cur.tokens(), idx))
                .map(BinaryOp::binding_power)
            else {
                break;
            };
            let (left_bp, right_bp) = binding_power;
            if left_bp < min_bp {
                break;
            }
            let Some(op) = p.cur.eat_op::<BinaryOp>() else {
                break;
            };
            let rhs = expr_bp(p, right_bp, r);
            lhs = AnyExpr::Binary(p.alloc(mark, BinaryExpr { lhs, op, rhs }));
        }
    }
    lhs
}

/// Whether a line break before the current token ends the expression parsed
/// so far. Calls and indexing never attach across a line break. Where lines
/// separate items, a token that can begin an expression ends the previous
/// one — except a `-` not followed by a blank: `a\n- b` subtracts, `a\n-b`
/// is two expressions. Inside a delimiter a line break is whitespace.
fn line_break_ends_expr(cur: Cursor<'_>, r: Restrictions) -> bool {
    if !cur.has_newline_ahead() {
        return false;
    }
    if cur.at(TokenKind::LParen) || cur.at(TokenKind::LBracket) {
        return true;
    }
    if !r.contains(Restrictions::LINE_SEPARATED) {
        return false;
    }
    if cur.at(TokenKind::Minus) {
        return !cur.nth(0).is_some_and(|idx| cur.blank_after(idx));
    }
    at_expression_start(cur)
}

// ----- prefix and primary -----------------------------------------------------------

/// A prefix operator and its operand, or a primary. The operand of a prefix
/// operator is parsed at [`PREFIX_BINDING_POWER`], so postfix forms bind
/// tighter: `!x.f()` is `!(x.f())`.
fn prefix(p: &mut Parser<'_, '_>, r: Restrictions) -> Result<AnyExpr, Missing> {
    if let Some(op) = p.cur.eat_in::<UnaryOpToken>() {
        let operand = expr_bp(p, PREFIX_BINDING_POWER, r);
        return Ok(AnyExpr::Unary(
            p.alloc(Mark::of(op), UnaryExpr { op, operand }),
        ));
    }
    if let Some(kw) = p.cur.eat::<kw::Await>() {
        // `await f catch …` catches what the await re-throws: the clause
        // belongs to the whole `await f`.
        let operand = expr_bp(p, PREFIX_BINDING_POWER, r | Restrictions::NO_CATCH);
        return Ok(AnyExpr::Await(
            p.alloc(Mark::of(kw), AwaitExpr { kw, operand }),
        ));
    }
    if let Some(kw) = p.cur.eat::<kw::Spawn>() {
        return Ok(AnyExpr::Spawn(spawn_expr(p, kw, r)));
    }
    primary(p, r)
}

/// Whether the current token ends a statement: a diverging form's operand
/// does not begin at one.
fn at_statement_end(cur: Cursor<'_>) -> bool {
    cur.at_end()
        || matches!(
            cur.kind(0),
            Some(TokenKind::Semicolon | TokenKind::Comma | TokenKind::RBrace)
        )
}

fn primary(p: &mut Parser<'_, '_>, r: Restrictions) -> Result<AnyExpr, Missing> {
    if let Some(token) = p.cur.eat_in::<LiteralExprToken>() {
        return Ok(AnyExpr::Literal(
            p.alloc(Mark::of(token), LiteralExpr { token }),
        ));
    }
    if let Some(lit) = string_lit(p, StringSite::Expr) {
        return Ok(AnyExpr::String(lit));
    }
    if let Some(open) = p.cur.eat::<baml_syntax::token::lit::BacktickStart>() {
        return Ok(AnyExpr::Backtick(BacktickString::parse(p, open)));
    }
    if let Some(kw) = p.cur.eat::<kw::Throw>() {
        let payload = if at_statement_end(p.cur) {
            Err(p.missing("expression after 'throw'"))
        } else {
            expr(p, r | Restrictions::NO_CATCH)
        };
        return Ok(AnyExpr::Throw(
            p.alloc(Mark::of(kw), ThrowExpr { kw, payload }),
        ));
    }
    if let Some(kw) = p.cur.eat::<kw::Return>() {
        // `return x catch …` returns the caught value: a `return` cannot
        // throw, so the clause can only be the value's.
        let value = if at_statement_end(p.cur) {
            None
        } else {
            expr(p, r).ok()
        };
        return Ok(AnyExpr::Return(
            p.alloc(Mark::of(kw), ReturnExpr { kw, value }),
        ));
    }
    if let Some(kw) = p.cur.eat::<kw::Break>() {
        return Ok(AnyExpr::Break(p.alloc(Mark::of(kw), BreakExpr { kw })));
    }
    if let Some(kw) = p.cur.eat::<kw::Continue>() {
        return Ok(AnyExpr::Continue(
            p.alloc(Mark::of(kw), ContinueExpr { kw }),
        ));
    }
    if p.cur.at(TokenKind::Map)
        && p.cur.nth_is(1, TokenKind::LBrace)
        && let Some(map_kw) = p.cur.eat::<ctx::Map>()
        && let Some(l) = p.cur.eat::<punct::LBrace>()
    {
        return Ok(AnyExpr::Map(MapLiteral::parse(p, (Some(map_kw), l))));
    }
    // `env.NAME` sugar; `env.get(…)` is an ordinary path.
    if p.cur.at(TokenKind::Env)
        && p.cur.nth_is(1, TokenKind::Dot)
        && p.cur.nth_is(2, TokenKind::Word)
        && !p.cur.nth_is(3, TokenKind::LParen)
        && let Some(env) = p.cur.eat::<ctx::Env>()
        && let Some(dot) = p.cur.eat::<punct::Dot>()
    {
        let name = p.expect_name(KindSet::single(TokenKind::Word), "identifier after 'env.'");
        return Ok(AnyExpr::EnvAccess(
            p.alloc(Mark::of(env), EnvAccessExpr { env, dot, name }),
        ));
    }
    if let Some(kw) = p.cur.eat::<kw::If>() {
        return Ok(if_expr(p, kw, r).into());
    }
    if let Some(kw) = p.cur.eat::<kw::Match>() {
        return Ok(AnyExpr::Match(match_expr(p, kw, r)));
    }
    let lambda = p.cur.at(TokenKind::LParen) && looks_like_lambda(p.cur);
    let projection =
        p.cur.at(TokenKind::LParen) && !lambda && looks_like_qualified_projection(p.cur);
    if let Some(l) = p.cur.eat::<punct::LParen>() {
        if lambda {
            return Ok(AnyExpr::Lambda(LambdaExpr::parse(p, (None, l))));
        }
        if projection {
            return Ok(AnyExpr::QualifiedPath(qualified_path_expr(p, l)));
        }
        let inner = operand(p, Restrictions::empty());
        let r = p.expect::<punct::RParen>();
        return Ok(AnyExpr::Paren(
            p.alloc(Mark::of(l), ParenExpr { l, inner, r }),
        ));
    }
    if let Some(l) = p.cur.eat::<punct::LBracket>() {
        return Ok(AnyExpr::Array(ArrayLiteral::parse(p, l)));
    }
    let map = p.cur.at(TokenKind::LBrace) && looks_like_map(p.cur);
    if let Some(l) = p.cur.eat::<punct::LBrace>() {
        return Ok(if map {
            AnyExpr::Map(MapLiteral::parse(p, (None, l)))
        } else {
            AnyExpr::Block(block(p, l))
        });
    }
    if p.cur.at(TokenKind::Less)
        && looks_like_generic_lambda(p.cur)
        && let Some(lt) = p.cur.eat::<punct::Less>()
    {
        // Lambdas are function values and cannot declare generic parameters:
        // the list is parsed for its diagnostics and kept as a `Bogus`.
        let error = ParseError::InvalidSyntax {
            message: "a lambda cannot declare generic parameters".to_string(),
            range: p.cur.token_range(lt.idx),
        };
        p.discard(|p| {
            let _rejected = GenericParamList::parse(p, lt);
        });
        let rejected = p.bogus_since(Mark::of(lt), error);
        return Ok(match p.cur.eat::<punct::LParen>() {
            Some(l) => AnyExpr::Lambda(LambdaExpr::parse(p, (Some(rejected), l))),
            None => AnyExpr::Bogus(rejected),
        });
    }
    if let Some(head) = p.cur.eat_set::<NameToken>(EXPR_NAME) {
        return Ok(AnyExpr::Path(PathExpr::parse(p, head)));
    }
    Err(p.missing("expression"))
}

/// `x`, `a.b.c`: a name and the `.segment`s that follow it. The first segment
/// must be a path-shaped word; later ones may be any member name, so
/// `reflect.Class.extends` is one path. A `.` followed by nothing path-like
/// right after the head is left to the member-access loop.
impl Parse for PathExpr {
    type Head = Token![NameToken];
    fn parse(p: &mut Parser<'_, '_>, head: Self::Head) -> Id<Self> {
        let mark = Mark::of(head);
        let head = p.name(head);
        let mut segments = Vec::new();
        loop {
            if looks_like_as_projection(p.cur) {
                break;
            }
            let allowed = if segments.is_empty() {
                PATH_SEGMENT.insert(TokenKind::Client)
            } else {
                PATH_SEGMENT.union(MEMBER_NAME)
            };
            let continues =
                p.cur.at(TokenKind::Dot) && p.cur.kind(1).is_some_and(|k| allowed.contains(k));
            if !continues && segments.is_empty() {
                break;
            }
            let Some(dot) = p.cur.eat::<punct::Dot>() else {
                break;
            };
            let name = p.expect_name(allowed, "path segment after '.'");
            let ended = name.is_err();
            segments.push(PathSegment { dot, name });
            if ended {
                break;
            }
        }
        let segments = p.out.alloc_list(segments);
        p.alloc(mark, PathExpr { head, segments })
    }
}

// ----- calls and literals --------------------------------------------------------------

impl Parse for ArgList {
    type Head = Token!["("];
    fn parse(p: &mut Parser<'_, '_>, l: Self::Head) -> Id<Self> {
        let args = p.sep_list(TokenKind::RParen, arg);
        let r = p.expect::<punct::RParen>();
        p.alloc(Mark::of(l), ArgList { l, args, r })
    }
}

/// `value` or `label = value`; `None` (reported) when nothing here starts one.
fn arg(p: &mut Parser<'_, '_>) -> Option<Id<Arg>> {
    let mark = p.cur.mark();
    let label = if p.cur.at_set(EXPR_NAME)
        && p.cur.nth_is(1, TokenKind::Equals)
        && let Some(name) = p.eat_name(EXPR_NAME)
        && let Some(eq) = p.cur.eat::<punct::Equals>()
    {
        Some(baml_syntax::nodes::ArgLabel { name, eq })
    } else {
        None
    };
    if label.is_none() && !at_expression_start(p.cur) {
        p.unexpected("expression");
        return None;
    }
    let value = operand(p, Restrictions::empty());
    Some(p.alloc(mark, Arg { label, value }))
}

/// Expression-side `<T, U>`.
impl Parse for GenericArgs {
    type Head = Token!["<"];
    fn parse(p: &mut Parser<'_, '_>, l: Self::Head) -> Id<Self> {
        let args = p.sep_list(TokenKind::Greater, |p| any_type(p).ok());
        let r = p.expect::<punct::Greater>();
        p.alloc(Mark::of(l), GenericArgs { l, args, r })
    }
}

/// `[a, b c]` — commas between elements are optional.
impl Parse for ArrayLiteral {
    type Head = Token!["["];
    fn parse(p: &mut Parser<'_, '_>, l: Self::Head) -> Id<Self> {
        let mut elements = Vec::new();
        while !p.cur.at(TokenKind::RBracket) && !p.cur.at_end() {
            let Ok(value) = operand(p, Restrictions::empty()) else {
                // Reported; the token that is not an expression is skipped.
                p.cur.skip();
                continue;
            };
            let comma = p.cur.eat::<punct::Comma>();
            elements.push(ArrayElement { value, comma });
        }
        let elements = p.out.alloc_list(elements);
        let r = p.expect::<punct::RBracket>();
        p.alloc(Mark::of(l), ArrayLiteral { l, elements, r })
    }
}

/// `{ k: v }` / `map { k: v }`: keys and values are expressions, colons
/// required, commas canonical but omissible before an unambiguous next key.
impl Parse for MapLiteral {
    type Head = (Option<Token!["map"]>, Token!["{"]);
    fn parse(p: &mut Parser<'_, '_>, (map_kw, l): Self::Head) -> Id<Self> {
        let mark = map_kw.map_or(Mark::of(l), Mark::of);
        let entries = p.members(TokenKind::RBrace, STATEMENT_KEYWORD, EXPR_START, |p| {
            if !at_expression_start(p.cur) {
                p.unexpected("map key or '}'");
                return None;
            }
            let mark = p.cur.mark();
            let key = expr(p, Restrictions::empty()).ok()?;
            let colon = p.expect::<punct::Colon>();
            let value = match colon {
                Ok(_) if p.cur.at_set(STATEMENT_KEYWORD) => Err(p.missing("map value")),
                Ok(_) if at_expression_start(p.cur) => expr(p, Restrictions::empty()),
                Ok(_) => Err(p.missing("map value")),
                Err(missing) => Err(missing),
            };
            let comma = p.cur.eat::<punct::Comma>();
            let missing_sep = comma.is_none().then_some("',' or '}' after map entry");
            Some((
                p.alloc(
                    mark,
                    MapEntry {
                        key,
                        colon,
                        value,
                        comma,
                    },
                ),
                missing_sep,
            ))
        });
        let entries = p.out.alloc_list(entries);
        let r = p.expect::<punct::RBrace>();
        p.alloc(
            mark,
            MapLiteral {
                map_kw,
                l,
                entries,
                r,
            },
        )
    }
}

/// What may begin an object-literal member, for recovery.
const OBJECT_MEMBER_START: KindSet = EXPR_NAME.union(StringToken::KINDS).union(KindSet::new(&[
    TokenKind::BacktickStart,
    TokenKind::DotDotDot,
]));

/// `Ctor { field: value, ...spread }` after the constructor and its `{`;
/// `mark` is where the constructor began.
fn object_literal(
    p: &mut Parser<'_, '_>,
    mark: Mark,
    ctor: AnyObjectCtor,
    l: Token!["{"],
) -> Id<ObjectLiteral> {
    let members = p.members(
        TokenKind::RBrace,
        STATEMENT_KEYWORD,
        OBJECT_MEMBER_START,
        |p| {
            let mark = p.cur.mark();
            if let Some(dots) = p.cur.eat::<punct::DotDotDot>() {
                let value = operand(p, Restrictions::empty());
                let comma = p.cur.eat::<punct::Comma>();
                let missing_sep = comma.is_none().then_some("',' or '}' after spread element");
                return Some((
                    AnyObjectMember::Spread(p.alloc(mark, SpreadElement { dots, value, comma })),
                    missing_sep,
                ));
            }
            let (key, shorthand) = if let Some(head) = p.cur.eat_set::<NameToken>(EXPR_NAME) {
                let key = dotted_name(p, head, Ident::KINDS, "identifier");
                let shorthand = p.out.get(key).segments.is_empty()
                    && matches!(p.cur.kind(0), Some(TokenKind::Comma | TokenKind::RBrace));
                (AnyObjectKey::Name(key), shorthand)
            } else if let Some(lit) = string_lit(p, StringSite::Expr) {
                (AnyObjectKey::String(lit), false)
            } else if let Some(open) = p.cur.eat::<baml_syntax::token::lit::BacktickStart>() {
                (
                    AnyObjectKey::Backtick(BacktickString::parse(p, open)),
                    false,
                )
            } else {
                p.unexpected("field name, spread element, or '}'");
                return None;
            };
            let value = if shorthand {
                None
            } else {
                p.cur.eat::<punct::Colon>().map(|colon| {
                    let value = if p.cur.at_set(STATEMENT_KEYWORD) || !at_expression_start(p.cur) {
                        Err(p.missing("field value"))
                    } else {
                        expr(p, Restrictions::empty())
                    };
                    FieldValue { colon, value }
                })
            };
            if !shorthand && value.is_none() {
                p.unexpected("':'");
            }
            let comma = p.cur.eat::<punct::Comma>();
            let missing_sep = comma.is_none().then_some("',' or '}' after object field");
            Some((
                AnyObjectMember::Field(p.alloc(mark, ObjectField { key, value, comma })),
                missing_sep,
            ))
        },
    );
    let members = p.out.alloc_list(members);
    let r = p.expect::<punct::RBrace>();
    p.alloc(
        mark,
        ObjectLiteral {
            ctor,
            l,
            members,
            r,
        },
    )
}

// ----- lambdas ------------------------------------------------------------------------

/// `params) -> R throws E { … }` after its `(`, and after a rejected generic
/// parameter list if there was one.
impl Parse for LambdaExpr {
    type Head = (Option<Id<Bogus>>, Token!["("]);
    fn parse(p: &mut Parser<'_, '_>, (rejected_generics, l): Self::Head) -> Id<Self> {
        let mark = rejected_generics.map_or(Mark::of(l), |bogus| p.mark_of(AnyNodeId::of(bogus)));
        let params = param_list(p, l, Annotation::Optional);
        let arrow = p.expect_in::<AnyArrow>("'->' after lambda parameters");
        let ret = if p.cur.at(TokenKind::LBrace) || p.cur.at(TokenKind::Throws) {
            None
        } else {
            any_type(p).ok()
        };
        let throws = p
            .cur
            .eat::<kw::Throws>()
            .map(|kw| ThrowsClause::parse(p, kw));
        let body = body_block(p, "lambda body '{'");
        p.alloc(
            mark,
            LambdaExpr {
                rejected_generics,
                params,
                arrow,
                ret,
                throws,
                body,
            },
        )
    }
}

// ----- control flow -----------------------------------------------------------------

/// What an `if` parsed to: the else-branch needs the typed id.
enum IfLike {
    If(Id<IfExpr>),
    IfLet(Id<IfLetExpr>),
}

impl From<IfLike> for AnyExpr {
    fn from(value: IfLike) -> Self {
        match value {
            IfLike::If(id) => AnyExpr::If(id),
            IfLike::IfLet(id) => AnyExpr::IfLet(id),
        }
    }
}

impl From<IfLike> for AnyElse {
    fn from(value: IfLike) -> Self {
        match value {
            IfLike::If(id) => AnyElse::If(id),
            IfLike::IfLet(id) => AnyElse::IfLet(id),
        }
    }
}

/// After `if`: `c { } else …` or `let p = s { } else …`. The condition is
/// parsed with a `{` reserved for the then-block.
fn if_expr(p: &mut Parser<'_, '_>, kw: Token!["if"], r: Restrictions) -> IfLike {
    let mark = Mark::of(kw);
    if at_binding_intro(p.cur)
        && let Some(introducer) = p.cur.eat_in::<LetIntroducer>()
    {
        // The pattern ends at `=`, so a destructure's `{` is never the body's.
        let pattern = pattern_after_introducer(p, Restrictions::empty());
        let eq = p.expect_as::<punct::Equals>("'=' after if-let pattern");
        let scrutinee = match eq {
            Ok(_) => expr(p, r.before_body()),
            Err(missing) => Err(missing),
        };
        let then_branch = body_block(p, "block after if-let scrutinee");
        let else_branch = else_branch(p, r);
        return IfLike::IfLet(p.alloc(
            mark,
            IfLetExpr {
                kw,
                introducer,
                pattern,
                eq,
                scrutinee,
                then_branch,
                else_branch,
            },
        ));
    }
    let cond = operand(p, r.before_body());
    IfLike::If(if_after_cond(p, mark, kw, cond, r))
}

/// The branches of an `if` whose condition the caller has parsed (a
/// template's `${if c}` tag is told from an if-expression by what follows
/// the condition).
pub(crate) fn if_after_cond(
    p: &mut Parser<'_, '_>,
    mark: Mark,
    kw: Token!["if"],
    cond: Result<AnyExpr, Missing>,
    r: Restrictions,
) -> Id<IfExpr> {
    let then_branch = body_block(p, "block after if condition");
    let else_branch = else_branch(p, r);
    p.alloc(
        mark,
        IfExpr {
            kw,
            cond,
            then_branch,
            else_branch,
        },
    )
}

fn else_branch(p: &mut Parser<'_, '_>, r: Restrictions) -> Option<ElseBranch> {
    let kw = p.cur.eat::<kw::Else>()?;
    let body = if let Some(if_kw) = p.cur.eat::<kw::If>() {
        Ok(if_expr(p, if_kw, r).into())
    } else if let Some(l) = p.cur.eat::<punct::LBrace>() {
        Ok(AnyElse::Block(block(p, l)))
    } else {
        Err(p.missing("'if' or block after 'else'"))
    };
    Some(ElseBranch { kw, body })
}

/// After `match`: `(x: T) { arms }` or `x { arms }`.
fn match_expr(p: &mut Parser<'_, '_>, kw: Token!["match"], r: Restrictions) -> Id<MatchExpr> {
    let scrutinee = if let Some(l) = p.cur.eat::<punct::LParen>() {
        let expr = operand(p, Restrictions::empty());
        let ascription = p.cur.eat::<punct::Colon>().map(|colon| {
            let ty = any_type(p);
            p.alloc(
                Mark::of(colon),
                TypeAscription {
                    colon: Some(colon),
                    ty,
                },
            )
        });
        let r = p.expect::<punct::RParen>();
        Ok(AnyMatchScrutinee::Paren(p.alloc(
            Mark::of(l),
            MatchParen {
                l,
                expr,
                ascription,
                r,
            },
        )))
    } else {
        operand(p, r.before_body()).map(AnyMatchScrutinee::Bare)
    };
    let l = p.expect_as::<punct::LBrace>("'{' after match scrutinee");
    let mut arms = Vec::new();
    let mut saw_line = false;
    if l.is_ok() {
        while !p.cur.at(TokenKind::RBrace) && !p.cur.at_end() && !p.cur.at_set(ITEM_KEYWORD) {
            saw_line = true;
            if !crate::patterns::at_pattern_start(p.cur) {
                p.unexpected("pattern");
                p.cur.skip();
                continue;
            }
            arms.push(match_arm(p));
        }
        if !saw_line {
            p.unexpected("at least one match arm");
        }
    }
    let arms = p.out.alloc_list(arms);
    let r = match l {
        Ok(_) => p.expect::<punct::RBrace>(),
        Err(missing) => Err(missing),
    };
    p.alloc(
        Mark::of(kw),
        MatchExpr {
            kw,
            scrutinee,
            l,
            arms,
            r,
        },
    )
}

/// `pattern if guard => body,` — the body is a block when a `{` follows the
/// arrow, else an expression that ends at the line's end.
fn match_arm(p: &mut Parser<'_, '_>) -> Id<MatchArm> {
    let mark = p.cur.mark();
    let pattern = pattern(p, Restrictions::empty());
    let guard = p.cur.eat::<kw::If>().map(|kw| {
        let cond = operand(p, Restrictions::STATEMENT);
        MatchGuard { kw, cond }
    });
    let arrow = p.expect_as::<punct::FatArrow>("'=>' after pattern");
    let body = arm_body(p);
    let comma = p.cur.eat::<punct::Comma>();
    p.alloc(
        mark,
        MatchArm {
            pattern,
            guard,
            arrow,
            body,
            comma,
        },
    )
}

/// The body of a `match`/`catch` arm after its `=>`.
fn arm_body(p: &mut Parser<'_, '_>) -> Result<AnyExpr, Missing> {
    if let Some(l) = p.cur.eat::<punct::LBrace>() {
        Ok(AnyExpr::Block(block(p, l)))
    } else {
        operand(p, Restrictions::STATEMENT)
    }
}

/// `body catch (e) { arms } catch_all (e) { arms }` after the first clause's
/// keyword: every clause that follows joins the one expression.
fn catch_expr(p: &mut Parser<'_, '_>, body: AnyExpr, kw: Token![CatchKw]) -> AnyExpr {
    let mark = p.mark_of_any(&body);
    let first = CatchClause::parse(p, kw);
    let mut rest = Vec::new();
    while let Some(kw) = p.cur.eat_in::<CatchKw>() {
        rest.push(CatchClause::parse(p, kw));
    }
    let clauses = p.out.alloc_list1(first, rest);
    AnyExpr::Catch(p.alloc(mark, CatchExpr { body, clauses }))
}

impl Parse for CatchClause {
    type Head = Token![CatchKw];
    fn parse(p: &mut Parser<'_, '_>, kw: Self::Head) -> Id<Self> {
        let l = p.expect::<punct::LParen>();
        let (binding, trace, r) = match l {
            Ok(_) => {
                let binding = p.expect_name(Ident::KINDS, "catch binding identifier");
                let trace = p.cur.eat::<punct::Comma>().map(|comma| {
                    let name = p.expect_name(Ident::KINDS, "identifier");
                    TraceBinding { comma, name }
                });
                let r = p.expect::<punct::RParen>();
                (binding, trace, r)
            }
            Err(missing) => (Err(missing), None, Err(missing)),
        };
        let lbrace = match l {
            Ok(_) => p.expect_as::<punct::LBrace>("catch clause body"),
            Err(missing) => Err(missing),
        };
        let mut arms = Vec::new();
        let mut saw_line = false;
        if lbrace.is_ok() {
            while !p.cur.at(TokenKind::RBrace) && !p.cur.at_end() && !p.cur.at_set(ITEM_KEYWORD) {
                saw_line = true;
                if !crate::patterns::at_pattern_start(p.cur) {
                    p.unexpected("pattern");
                    p.cur.skip();
                    continue;
                }
                arms.push(catch_arm(p));
            }
            if !saw_line {
                p.unexpected("at least one catch arm");
            }
        }
        let arms = p.out.alloc_list(arms);
        let rbrace = match lbrace {
            Ok(_) => p.expect::<punct::RBrace>(),
            Err(missing) => Err(missing),
        };
        p.alloc(
            Mark::of(kw),
            CatchClause {
                kw,
                l,
                binding,
                trace,
                r,
                lbrace,
                arms,
                rbrace,
            },
        )
    }
}

fn catch_arm(p: &mut Parser<'_, '_>) -> Id<CatchArm> {
    let mark = p.cur.mark();
    let pattern = pattern(p, Restrictions::empty());
    let arrow = p.expect_as::<punct::FatArrow>("'=>' after catch pattern");
    let body = match arrow {
        Ok(_) => arm_body(p),
        Err(missing) => {
            // Skip to where the next arm (or the body's end) can begin.
            while !p.cur.at_end()
                && !p.cur.at(TokenKind::RBrace)
                && !p.cur.at(TokenKind::Comma)
                && !crate::lookahead::looks_like_catch_arm_ahead(p.cur)
            {
                p.cur.skip();
            }
            Err(missing)
        }
    };
    let comma = p.cur.eat::<punct::Comma>();
    p.alloc(
        mark,
        CatchArm {
            pattern,
            arrow,
            body,
            comma,
        },
    )
}

/// `name with a, b { … }` after `spawn`: the name and modifiers end before
/// the body's `{`.
fn spawn_expr(p: &mut Parser<'_, '_>, kw: Token!["spawn"], r: Restrictions) -> Id<SpawnExpr> {
    let name = if p.cur.at(TokenKind::LBrace)
        || p.cur.at(TokenKind::With)
        || !at_expression_start(p.cur)
    {
        None
    } else {
        expr(p, r.before_body()).ok()
    };
    let with = p
        .cur
        .eat::<ctx::With>()
        .map(|kw| with_clause(p, kw, r.before_body()));
    let body = body_block(p, "'{' after spawn");
    p.alloc(
        Mark::of(kw),
        SpawnExpr {
            kw,
            name,
            with,
            body,
        },
    )
}

/// `a, b` before a `{` after `with`: on `spawn`, `test` and `testset`.
pub(crate) fn with_clause(
    p: &mut Parser<'_, '_>,
    kw: Token!["with"],
    r: Restrictions,
) -> Id<WithClause> {
    let items = p.sep_list(TokenKind::LBrace, |p| {
        if at_expression_start(p.cur) {
            expr(p, r).ok()
        } else {
            p.unexpected("expression");
            None
        }
    });
    p.alloc(Mark::of(kw), WithClause { kw, items })
}
