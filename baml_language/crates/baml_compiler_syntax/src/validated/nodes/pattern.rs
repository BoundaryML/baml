use crate::{
    SyntaxElement, SyntaxKind, SyntaxNode,
    validated::{
        FromCST, KnownKind, StrongAstError, SyntaxNodeIter,
        nodes::{Expression, GenericArgs, Type, TypeArgs},
        tokens as t,
    },
};

/// Top-level pattern AST node - corresponds to a [`SyntaxKind::PATTERN`].
#[derive(Debug)]
pub enum MatchPattern {
    Wildcard(WildcardPattern),
    Binding(BindingPattern),
    Destructure(DestructurePattern),
    Array(ArrayPattern),
    Type(TypePattern),
    Unreflect(UnreflectPattern),
    Paren(ParenPattern),
    Union(UnionPattern),
    Chain(ChainPattern),
}

impl FromCST for MatchPattern {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PATTERN)?;

        let mut it = SyntaxNodeIter::new(&node);
        let inner = it.expect_next("pattern body")?;
        it.expect_end()?;
        MatchPattern::from_inner(inner)
    }
}

impl MatchPattern {
    /// Convert one of the inner pattern kinds (an atom, `UNION_PATTERN`, or
    /// `CHAIN_PATTERN`) into the rich enum.
    fn from_inner(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        match node.kind() {
            SyntaxKind::WILDCARD_PATTERN => {
                WildcardPattern::from_node(&node).map(MatchPattern::Wildcard)
            }
            SyntaxKind::BINDING_PATTERN => {
                BindingPattern::from_node(&node).map(MatchPattern::Binding)
            }
            SyntaxKind::TYPE_PATTERN => TypePattern::from_node(&node).map(MatchPattern::Type),
            SyntaxKind::UNREFLECT_PATTERN => {
                UnreflectPattern::from_node(&node).map(MatchPattern::Unreflect)
            }
            SyntaxKind::ARRAY_PATTERN => ArrayPattern::from_node(&node).map(MatchPattern::Array),
            SyntaxKind::PAREN_PATTERN => ParenPattern::from_node(&node).map(MatchPattern::Paren),
            SyntaxKind::UNION_PATTERN => UnionPattern::from_node(&node).map(MatchPattern::Union),
            SyntaxKind::CHAIN_PATTERN => ChainPattern::from_node(&node).map(MatchPattern::Chain),
            SyntaxKind::DESTRUCTURE_PATTERN => {
                DestructurePattern::from_node(&node).map(MatchPattern::Destructure)
            }
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "a pattern kind".into(),
                found,
                at: node.text_range(),
            }),
        }
    }
}

impl KnownKind for MatchPattern {
    fn kind() -> SyntaxKind {
        SyntaxKind::PATTERN
    }
}

/// `_`, `let _`, or `const _`.
#[derive(Debug)]
pub struct WildcardPattern {
    pub let_keyword: Option<t::BindingKeyword>,
    pub underscore: t::Word,
}

impl WildcardPattern {
    fn from_node(node: &SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let let_keyword = it
            .next_if(|elem| matches!(elem.kind(), SyntaxKind::KW_LET | SyntaxKind::KW_CONST))
            .map(t::BindingKeyword::from_cst)
            .transpose()?;
        let underscore_elem = it.expect_next("`_`")?;
        let underscore = t::Word::from_cst(underscore_elem)?;
        it.expect_end()?;
        Ok(Self {
            let_keyword,
            underscore,
        })
    }
}

/// `let WORD`/`const WORD` or `let WORD : <pattern>`/`const WORD : <pattern>` - name binding with an optional
/// sub-pattern. The sub-pattern can be a type ascription (`let x: int`),
/// another binding (`let x: let y`), a structural destructure
/// (`let x: [a, b]`, `let x: Class { f }`), etc. The parser folds the
/// `: <pattern>` directly into the [`SyntaxKind::BINDING_PATTERN`] node
/// (no `CHAIN_PATTERN` wrapper).
#[derive(Debug)]
pub struct BindingPattern {
    pub let_keyword: t::BindingKeyword,
    pub name: t::Word,
    pub subpat: Option<(t::Colon, Box<MatchPattern>)>,
}

impl BindingPattern {
    fn from_node(node: &SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let let_keyword = t::BindingKeyword::from_cst(it.expect_next("binding introducer")?)?;
        let name = it.expect_parse()?;
        let subpat = if let Some(colon_elem) = it.next_if_kind(SyntaxKind::COLON) {
            let colon = t::Colon::from_cst(colon_elem)?;
            let pattern: MatchPattern = it.expect_parse()?;
            Some((colon, Box::new(pattern)))
        } else {
            None
        };
        it.expect_end()?;
        Ok(Self {
            let_keyword,
            name,
            subpat,
        })
    }
}

/// `(let|const)? path.Class { field, renamed: <pattern>, ... }`.
#[derive(Debug)]
pub struct DestructurePattern {
    pub let_keyword: Option<t::BindingKeyword>,
    pub first: t::Word,
    pub rest: Vec<(t::Dot, t::Word)>,
    pub generic_args: Option<DestructureTypeArgs>,
    pub open_brace: t::LBrace,
    pub fields: Vec<(FieldPattern, Option<t::Comma>)>,
    pub close_brace: t::RBrace,
}

impl DestructurePattern {
    fn from_node(node: &SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let let_keyword = it
            .next_if(|element| matches!(element.kind(), SyntaxKind::KW_LET | SyntaxKind::KW_CONST))
            .map(t::BindingKeyword::from_cst)
            .transpose()?;
        let first = it.expect_parse()?;
        let mut rest = Vec::new();
        while let Some(dot) = it.next_if_kind(SyntaxKind::DOT) {
            rest.push((t::Dot::from_cst(dot)?, it.expect_parse()?));
        }
        let generic_args = it
            .next_if(|element| {
                matches!(
                    element.kind(),
                    SyntaxKind::GENERIC_ARGS | SyntaxKind::TYPE_ARGS
                )
            })
            .map(DestructureTypeArgs::from_cst)
            .transpose()?;
        let open_brace = it.expect_parse()?;
        let mut fields = Vec::new();
        let close_brace = loop {
            let Some(element) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_BRACE, it.parent));
            };
            if element.kind() == SyntaxKind::R_BRACE {
                break t::RBrace::from_cst(element)?;
            }
            let field = FieldPattern::from_cst(element)?;
            let comma = it
                .next_if_kind(SyntaxKind::COMMA)
                .map(t::Comma::from_cst)
                .transpose()?;
            fields.push((field, comma));
        };
        it.expect_end()?;
        Ok(Self {
            let_keyword,
            first,
            rest,
            generic_args,
            open_brace,
            fields,
            close_brace,
        })
    }
}

#[derive(Debug)]
pub enum DestructureTypeArgs {
    Generic(GenericArgs),
    Type(TypeArgs),
}

impl FromCST for DestructureTypeArgs {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::GENERIC_ARGS => GenericArgs::from_cst(elem).map(Self::Generic),
            SyntaxKind::TYPE_ARGS => TypeArgs::from_cst(elem).map(Self::Type),
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "GENERIC_ARGS or TYPE_ARGS".into(),
                found,
                at: elem.text_range(),
            }),
        }
    }
}

/// A single field inside a destructure pattern.
#[derive(Debug)]
pub struct FieldPattern {
    pub name: t::Word,
    pub pattern: Option<(t::Colon, MatchPattern)>,
}

impl FromCST for FieldPattern {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::FIELD_PATTERN)?;
        let mut it = SyntaxNodeIter::new(&node);
        let name = it.expect_parse()?;
        let pattern = if let Some(colon_elem) = it.next_if_kind(SyntaxKind::COLON) {
            let colon = t::Colon::from_cst(colon_elem)?;
            let pattern = it.expect_parse()?;
            Some((colon, pattern))
        } else {
            None
        };
        it.expect_end()?;
        Ok(Self { name, pattern })
    }
}

#[derive(Debug)]
pub struct ArrayPattern {
    pub open_bracket: t::LBracket,
    pub elements: Vec<(ArrayPatternElement, Option<t::Comma>)>,
    pub close_bracket: t::RBracket,
    /// `[...]: T` - optional type ascription folded into the
    /// [`SyntaxKind::ARRAY_PATTERN`] node by the parser.
    pub ascription: Option<(t::Colon, Type)>,
}

impl ArrayPattern {
    fn from_node(node: &SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let open_bracket = it.expect_parse()?;
        let mut elements = Vec::new();
        let close_bracket = loop {
            let Some(element) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_BRACKET, it.parent));
            };
            if element.kind() == SyntaxKind::R_BRACKET {
                break t::RBracket::from_cst(element)?;
            }
            let pattern = ArrayPatternElement::from_cst(element)?;
            let comma = it
                .next_if_kind(SyntaxKind::COMMA)
                .map(t::Comma::from_cst)
                .transpose()?;
            elements.push((pattern, comma));
        };
        let ascription = if let Some(colon) = it.next_if_kind(SyntaxKind::COLON) {
            Some((t::Colon::from_cst(colon)?, it.expect_parse()?))
        } else {
            None
        };
        it.expect_end()?;
        Ok(Self {
            open_bracket,
            elements,
            close_bracket,
            ascription,
        })
    }
}

#[derive(Debug)]
pub struct ArrayPatternElement {
    pub rest: Option<t::DotDot>,
    pub pattern: Option<MatchPattern>,
}

impl FromCST for ArrayPatternElement {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ARRAY_PATTERN_ELEMENT)?;
        let mut it = SyntaxNodeIter::new(&node);
        let rest = it
            .next_if_kind(SyntaxKind::DOT_DOT)
            .map(t::DotDot::from_cst)
            .transpose()?;
        let pattern = it.next().map(MatchPattern::from_cst).transpose()?;
        it.expect_end()?;
        Ok(Self { rest, pattern })
    }
}

/// Bare type-expression pattern (literals, paths, generics, function types,
/// arrays, etc).
#[derive(Debug)]
pub struct TypePattern {
    pub ty: Type,
}

impl TypePattern {
    fn from_node(node: &SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let ty = it.expect_parse()?;
        it.expect_end()?;
        Ok(Self { ty })
    }
}

/// `unreflect(expr)` runtime identity pattern.
#[derive(Debug)]
pub struct UnreflectPattern {
    pub marker: t::Word,
    pub open_paren: t::LParen,
    pub operand: Box<Expression>,
    pub close_paren: t::RParen,
}

impl UnreflectPattern {
    fn from_node(node: &SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let marker = it.expect_parse()?;
        let open_paren = it.expect_parse()?;
        let operand = Box::new(Expression::from_cst(it.expect_next("unreflect operand")?)?);
        let close_paren = it.expect_parse()?;
        it.expect_end()?;
        Ok(Self {
            marker,
            open_paren,
            operand,
            close_paren,
        })
    }
}

/// `( PATTERN )` - explicit grouping.
#[derive(Debug)]
pub struct ParenPattern {
    pub open_paren: t::LParen,
    pub pattern: Box<MatchPattern>,
    pub close_paren: t::RParen,
}

impl ParenPattern {
    fn from_node(node: &SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let open_paren = it.expect_parse()?;
        let pattern = it.expect_parse()?;
        let close_paren = it.expect_parse()?;
        it.expect_end()?;
        Ok(Self {
            open_paren,
            pattern: Box::new(pattern),
            close_paren,
        })
    }
}

/// Union alternation: `A | B | C`. Each member is a pattern (typically an atom,
/// since `|` binds tighter than `:`).
#[derive(Debug)]
pub struct UnionPattern {
    pub first: Box<MatchPattern>,
    pub rest: Vec<(t::Pipe, MatchPattern)>,
}

impl UnionPattern {
    fn from_node(node: &SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let first_elem = it.expect_next("a pattern atom")?;
        let first = MatchPattern::from_inner(first_elem)?;
        let mut rest = Vec::new();
        while let Some(pipe_elem) = it.next() {
            let pipe = t::Pipe::from_cst(pipe_elem)?;
            let next_elem = it.expect_next("a pattern atom after `|`")?;
            let next = MatchPattern::from_inner(next_elem)?;
            rest.push((pipe, next));
        }
        Ok(Self {
            first: Box::new(first),
            rest,
        })
    }
}

/// Type-narrowing chain: `A : B : C`. Each link is a pattern (atom or union).
#[derive(Debug)]
pub struct ChainPattern {
    pub first: Box<MatchPattern>,
    pub rest: Vec<(t::Colon, MatchPattern)>,
}

impl ChainPattern {
    fn from_node(node: &SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let first_elem = it.expect_next("a pattern atom")?;
        let first = MatchPattern::from_inner(first_elem)?;
        let mut rest = Vec::new();
        while let Some(colon_elem) = it.next() {
            let colon = t::Colon::from_cst(colon_elem)?;
            let next_elem = it.expect_next("a pattern atom after `:`")?;
            let next = MatchPattern::from_inner(next_elem)?;
            rest.push((colon, next));
        }
        Ok(Self {
            first: Box::new(first),
            rest,
        })
    }
}
