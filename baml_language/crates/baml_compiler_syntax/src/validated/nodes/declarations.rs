use crate::{
    SyntaxElement, SyntaxKind,
    validated::{
        FromCST, KnownKind, StrongAstError, SyntaxNodeIter,
        nodes::{BlockExpr, Expression, Type},
        tokens as t,
    },
};

#[derive(Debug)]
pub struct WithClause {
    pub keyword: t::With,
    pub expr: Expression,
}

#[derive(Debug)]
pub struct TestExprDecl {
    pub keyword: t::Test,
    pub name: Expression,
    pub with_clause: Option<WithClause>,
    pub body: BlockExpr,
}

impl FromCST for TestExprDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::TEST_EXPR_DEF)?;
        let mut it = SyntaxNodeIter::new(&node);
        let keyword = it.expect_parse()?;
        let name = Expression::from_cst(it.expect_next("a test name expression")?)?;
        let with_clause = if let Some(keyword) = it.next_if_kind(SyntaxKind::KW_WITH) {
            Some(WithClause {
                keyword: t::With::from_cst(keyword)?,
                expr: Expression::from_cst(it.expect_next("a runner expression")?)?,
            })
        } else {
            None
        };
        let body = it.expect_parse()?;
        it.expect_end()?;
        Ok(Self {
            keyword,
            name,
            with_clause,
            body,
        })
    }
}

impl KnownKind for TestExprDecl {
    fn kind() -> SyntaxKind {
        SyntaxKind::TEST_EXPR_DEF
    }
}

#[derive(Debug)]
pub struct TestSetDecl {
    pub keyword: t::TestSet,
    pub name: Expression,
    pub with_clause: Option<WithClause>,
    pub body: BlockExpr,
}

impl FromCST for TestSetDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::TESTSET_DEF)?;
        let mut it = SyntaxNodeIter::new(&node);
        let keyword = it.expect_parse()?;
        let name = Expression::from_cst(it.expect_next("a testset name expression")?)?;
        let with_clause = if let Some(keyword) = it.next_if_kind(SyntaxKind::KW_WITH) {
            Some(WithClause {
                keyword: t::With::from_cst(keyword)?,
                expr: Expression::from_cst(it.expect_next("a runner expression")?)?,
            })
        } else {
            None
        };
        let body = it.expect_parse()?;
        it.expect_end()?;
        Ok(Self {
            keyword,
            name,
            with_clause,
            body,
        })
    }
}

impl KnownKind for TestSetDecl {
    fn kind() -> SyntaxKind {
        SyntaxKind::TESTSET_DEF
    }
}
/// Corresponds to a [`SyntaxKind::PARAMETER_LIST`] node.
#[derive(Debug)]
pub struct FunctionParamList {
    pub open_paren: t::LParen,
    pub params: Vec<(FunctionParam, Option<t::Comma>)>,
    pub close_paren: t::RParen,
}
impl FromCST for FunctionParamList {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PARAMETER_LIST)?;

        let mut it = SyntaxNodeIter::new(&node);

        let open_paren = it.expect_parse()?;

        let mut params = Vec::new();

        let close_paren = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_PAREN, it.parent));
            };
            match elem.kind() {
                SyntaxKind::PARAMETER => {
                    let param = FunctionParam::from_cst(elem)?;
                    let comma = it
                        .next_if_kind(SyntaxKind::COMMA)
                        .map(t::Comma::from_cst)
                        .transpose()?;
                    params.push((param, comma));
                }
                SyntaxKind::R_PAREN => {
                    break t::RParen::from_cst(elem)?;
                }
                _ => {
                    return Err(StrongAstError::UnexpectedAdditionalElement {
                        parent: it.parent,
                        at: elem.text_range(),
                    });
                }
            }
        };

        it.expect_end()?;

        Ok(FunctionParamList {
            open_paren,
            params,
            close_paren,
        })
    }
}

impl KnownKind for FunctionParamList {
    fn kind() -> SyntaxKind {
        SyntaxKind::PARAMETER_LIST
    }
}

/// Corresponds to a [`SyntaxKind::PARAMETER`] node.
#[derive(Debug)]
pub struct FunctionParam {
    pub name: t::Word,
    /// Type annotation with optional colon (colon is optional per BEP-019).
    pub ty: Option<(Option<t::Colon>, Type)>,
    pub default: Option<(t::Equals, Expression)>,
}

impl FromCST for FunctionParam {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PARAMETER)?;

        let mut it = SyntaxNodeIter::new(&node);

        let name = it.expect_parse()?;

        let colon = it
            .next_if_kind(SyntaxKind::COLON)
            .map(t::Colon::from_cst)
            .transpose()?;
        let ty = if colon.is_some() {
            // Colon present - type is required
            let ty: Type = it.expect_parse()?;
            Some((colon, ty))
        } else if it.peek().map(SyntaxElement::kind) == Some(SyntaxKind::TYPE_EXPR) {
            // No colon but type present (BEP-019 optional colon)
            let elem = it.next().expect("peeked");
            Some((None, Type::from_cst(elem)?))
        } else {
            // No type annotation (e.g. `self`)
            None
        };

        let default = if let Some(equals) = it.next_if_kind(SyntaxKind::EQUALS) {
            let equals = t::Equals::from_cst(equals)?;
            let expr_elem = it
                .next()
                .ok_or_else(|| StrongAstError::missing_desc("default expression", it.parent))?;
            let expr = Expression::from_cst(expr_elem)?;
            Some((equals, expr))
        } else {
            None
        };

        it.expect_end()?;

        Ok(FunctionParam { name, ty, default })
    }
}

impl KnownKind for FunctionParam {
    fn kind() -> SyntaxKind {
        SyntaxKind::PARAMETER
    }
}
