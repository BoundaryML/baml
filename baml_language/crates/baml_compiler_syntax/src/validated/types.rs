use super::{
    Attribute, FromCST, KnownKind, Literal, StrongAstError, SyntaxElement, SyntaxKind,
    SyntaxNodeIter, TextRange, ThrowsClause, t,
};

/// Corresponds to a [`SyntaxKind::TYPE_EXPR`] node.
#[derive(Debug)]
pub enum Type {
    Paren(ParenType),
    Path(PathType),
    /// Generally only string literals are used in normal types,
    /// but other literals are valid in some contexts like match bindings.
    Literal(Literal),
    Union(UnionType),
    Optional(OptionalType),
    Array(ArrayType),
    Generic(GenericType),
    AssociatedProjection(AssociatedProjectionType),
    Function(FunctionType),
    /// Types constrained by attributes.
    Constrained(ConstrainedType<Type>),
    Unknown(TextRange),
}
impl Type {
    #[must_use]
    pub const fn multi_line_is_indented(&self) -> bool {
        match self {
            Self::Paren(_)
            | Self::Literal(_)
            | Self::Generic(_)
            | Self::AssociatedProjection(_) => false,
            Self::Path(_)
            | Self::Union(_)
            | Self::Function(_)
            | Self::Constrained(_)
            | Self::Unknown(_) => true,
            Self::Optional(inner) => inner.ty.multi_line_is_indented(),
            Self::Array(inner) => inner.ty.multi_line_is_indented(),
        }
    }
}
impl FromCST for Type {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::TYPE_EXPR)?;
        let mut it = SyntaxNodeIter::new(&node);
        let first = UnionTypeMember::take(&mut it)?;
        let mut rest = Vec::new();
        while let Some(pipe) = it.next_if_kind(SyntaxKind::PIPE) {
            let pipe = t::Pipe::from_cst(pipe)?;
            let next = UnionTypeMember::take(&mut it)?;
            rest.push((pipe, next));
        }
        it.expect_end()?;
        match rest.pop() {
            None => Ok(first.into()),
            Some((pipe, UnionTypeMember::Constrained(constrained))) => {
                let ConstrainedType { ty, attrs } = constrained;
                rest.push((pipe, *ty));
                Ok(Type::Constrained(ConstrainedType {
                    ty: Box::new(Type::Union(UnionType {
                        first: Box::new(first),
                        rest,
                    })),
                    attrs,
                }))
            }
            Some(other) => {
                rest.push(other);
                Ok(Type::Union(UnionType {
                    first: Box::new(first),
                    rest,
                }))
            }
        }
    }
}
impl KnownKind for Type {
    fn kind() -> SyntaxKind {
        SyntaxKind::TYPE_EXPR
    }
}
#[derive(Debug)]
pub struct ParenType {
    pub open_paren: t::LParen,
    /// Will have a [`SyntaxKind::FUNCTION_TYPE_PARAM`] with a [`SyntaxKind::TYPE_EXPR`] inside for some reason
    pub ty: Box<Type>,
    pub close_paren: t::RParen,
}
#[derive(Debug)]
pub struct PathType {
    pub first: t::Word,
    pub rest: Vec<(t::Dot, t::Word)>,
}
#[derive(Debug)]
pub struct StringType(pub t::QuotedString);
#[derive(Debug)]
pub struct UnionType {
    pub first: Box<UnionTypeMember>,
    pub rest: Vec<(t::Pipe, UnionTypeMember)>,
}
#[derive(Debug)]
pub enum UnionTypeMember {
    Paren(ParenType),
    Path(PathType),
    Literal(Literal),
    Optional(OptionalType),
    Array(ArrayType),
    Generic(GenericType),
    AssociatedProjection(AssociatedProjectionType),
    Function(FunctionType),
    /// Types constrained by attributes.
    Constrained(ConstrainedType<UnionTypeMember>),
    Unknown(TextRange),
}
impl UnionTypeMember {
    /// Take a base type (no postfix operators).
    /// If there are postix operators, they will remain in the iterator.
    ///
    /// So Paren, Path, String, or Function.
    fn take_base_type(it: &mut SyntaxNodeIter) -> Result<Self, StrongAstError> {
        let first = it.expect_next("a type")?;
        match first.kind() {
            SyntaxKind::L_PAREN => {
                let open_paren = t::LParen::from_cst(first)?;
                if it.peek().map(SyntaxElement::kind) == Some(SyntaxKind::TYPE_EXPR) {
                    let base: Type = it.expect_parse()?;
                    let as_token: t::As = it.expect_parse()?;
                    let interface: Type = it.expect_parse()?;
                    let close_paren: t::RParen = it.expect_parse()?;
                    let dot: t::Dot = it.expect_parse()?;
                    let member: t::Word = it.expect_parse()?;
                    return Ok(UnionTypeMember::AssociatedProjection(
                        AssociatedProjectionType {
                            open_paren,
                            base: Box::new(base),
                            as_token,
                            interface: Box::new(interface),
                            close_paren,
                            dot,
                            member,
                        },
                    ));
                }
                let mut params = Vec::new();
                let close_paren = loop {
                    let Some(elem) = it.next() else {
                        return Err(StrongAstError::missing(SyntaxKind::R_PAREN, it.parent));
                    };
                    match elem.kind() {
                        SyntaxKind::R_PAREN => {
                            break t::RParen::from_cst(elem)?;
                        }
                        SyntaxKind::FUNCTION_TYPE_PARAM => {
                            let param = FunctionTypeParam::from_cst(elem)?;
                            let comma = it
                                .next_if_kind(SyntaxKind::COMMA)
                                .map(t::Comma::from_cst)
                                .transpose()?;
                            params.push((param, comma));
                        }
                        _ => {
                            return Err(StrongAstError::UnexpectedKindDesc {
                                expected_desc: "FUNCTION_TYPE_PARAM or R_PAREN".into(),
                                found: elem.kind(),
                                at: elem.text_range(),
                            });
                        }
                    }
                };
                let must_be_func_type = params.len() != 1
                    || params
                        .iter()
                        .any(|item| item.0.name.is_some() || item.1.is_some());
                if must_be_func_type {
                    let arrow = it.expect_parse()?;
                    let return_ty: Type = it.expect_parse()?;
                    let throws =
                        if it.peek().map(SyntaxElement::kind) == Some(SyntaxKind::THROWS_CLAUSE) {
                            Some(Box::new(it.expect_parse()?))
                        } else {
                            None
                        };
                    Ok(UnionTypeMember::Function(FunctionType {
                        open_paren,
                        params,
                        close_paren,
                        arrow,
                        return_type: Box::new(return_ty),
                        throws,
                    }))
                } else if let Some(arrow) = it.next_if_kind(SyntaxKind::ARROW) {
                    let arrow = t::Arrow::from_cst(arrow)?;
                    let return_ty: Type = it.expect_parse()?;
                    let throws =
                        if it.peek().map(SyntaxElement::kind) == Some(SyntaxKind::THROWS_CLAUSE) {
                            Some(Box::new(it.expect_parse()?))
                        } else {
                            None
                        };
                    Ok(UnionTypeMember::Function(FunctionType {
                        open_paren,
                        params,
                        close_paren,
                        arrow,
                        return_type: Box::new(return_ty),
                        throws,
                    }))
                } else {
                    let (inner, _) = params
                        .pop()
                        .unwrap_or_else(|| unreachable!("we checked it has length 1"));
                    Ok(UnionTypeMember::Paren(ParenType {
                        open_paren,
                        ty: Box::new(inner.ty),
                        close_paren,
                    }))
                }
            }
            SyntaxKind::WORD => {
                let first = t::Word::from_cst(first)?;
                let mut rest = Vec::new();
                while let Some(dot) = it.next_if_kind(SyntaxKind::DOT) {
                    let dot = t::Dot::from_cst(dot)?;
                    let word: t::Word = it.expect_parse()?;
                    rest.push((dot, word));
                }
                Ok(UnionTypeMember::Path(PathType { first, rest }))
            }
            SyntaxKind::STRING_LITERAL
            | SyntaxKind::INTEGER_LITERAL
            | SyntaxKind::FLOAT_LITERAL => {
                let string = Literal::from_cst(first)?;
                Ok(UnionTypeMember::Literal(string))
            }
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "L_PAREN, WORD, STRING_LITERAL, INTEGER_LITERAL, or FLOAT_LITERAL"
                    .into(),
                found,
                at: first.text_range(),
            }),
        }
    }
    pub fn take(it: &mut SyntaxNodeIter) -> Result<Self, StrongAstError> {
        let mut ty = Self::take_base_type(it)?;
        loop {
            if it
                .peek()
                .is_some_and(|elem| elem.kind() == SyntaxKind::L_BRACKET)
            {
                let mut brackets = Vec::new();
                while let Some(open_bracket) = it.next_if_kind(SyntaxKind::L_BRACKET) {
                    let open_bracket = t::LBracket::from_cst(open_bracket)?;
                    let close_bracket: t::RBracket = it.expect_parse()?;
                    brackets.push((open_bracket, close_bracket));
                }
                ty = UnionTypeMember::Array(ArrayType {
                    ty: Box::new(ty.into()),
                    brackets,
                });
                continue;
            } else if let Some(question) = it.next_if_kind(SyntaxKind::QUESTION) {
                let question = t::Question::from_cst(question)?;
                ty = UnionTypeMember::Optional(OptionalType {
                    ty: Box::new(ty.into()),
                    question,
                });
                continue;
            } else if let Some(type_args) = it.next_if_kind(SyntaxKind::TYPE_ARGS) {
                let type_args = TypeArgs::from_cst(type_args)?;
                ty = UnionTypeMember::Generic(GenericType {
                    base: Box::new(ty.into()),
                    args: type_args,
                });
                continue;
            } else if let Some(attr) = it.next_if_kind(SyntaxKind::ATTRIBUTE) {
                let mut attrs = Vec::new();
                attrs.push(Attribute::from_cst(attr)?);
                while let Some(attr) = it.next_if_kind(SyntaxKind::ATTRIBUTE) {
                    attrs.push(Attribute::from_cst(attr)?);
                }
                ty = UnionTypeMember::Constrained(ConstrainedType {
                    ty: Box::new(ty),
                    attrs,
                });
                break;
            }
            break;
        }
        Ok(ty)
    }
}
impl From<UnionTypeMember> for Type {
    fn from(member: UnionTypeMember) -> Self {
        match member {
            UnionTypeMember::Paren(paren) => Type::Paren(paren),
            UnionTypeMember::Path(path) => Type::Path(path),
            UnionTypeMember::Literal(literal) => Type::Literal(literal),
            UnionTypeMember::Optional(optional) => Type::Optional(optional),
            UnionTypeMember::Array(array) => Type::Array(array),
            UnionTypeMember::Generic(generic) => Type::Generic(generic),
            UnionTypeMember::AssociatedProjection(projection) => {
                Type::AssociatedProjection(projection)
            }
            UnionTypeMember::Function(function) => Type::Function(function),
            UnionTypeMember::Constrained(constrained) => Type::Constrained(constrained.into()),
            UnionTypeMember::Unknown(range) => Type::Unknown(range),
        }
    }
}
#[derive(Debug)]
pub struct OptionalType {
    pub ty: Box<Type>,
    pub question: t::Question,
}
#[derive(Debug)]
pub struct ArrayType {
    pub ty: Box<Type>,
    pub brackets: Vec<(t::LBracket, t::RBracket)>,
}
#[derive(Debug)]
pub struct GenericType {
    pub base: Box<Type>,
    pub args: TypeArgs,
}
#[derive(Debug)]
pub struct AssociatedProjectionType {
    pub open_paren: t::LParen,
    pub base: Box<Type>,
    pub as_token: t::As,
    pub interface: Box<Type>,
    pub close_paren: t::RParen,
    pub dot: t::Dot,
    pub member: t::Word,
}
#[derive(Debug)]
pub enum TypeArg {
    Type(Type),
    Associated(AssociatedTypeArgBinding),
}
impl TypeArg {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::TYPE_EXPR => Type::from_cst(elem).map(TypeArg::Type),
            SyntaxKind::ASSOCIATED_TYPE_DECL => {
                AssociatedTypeArgBinding::from_cst(elem).map(TypeArg::Associated)
            }
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "TYPE_EXPR or ASSOCIATED_TYPE_DECL".into(),
                found,
                at: elem.text_range(),
            }),
        }
    }
}
#[derive(Debug)]
pub struct AssociatedTypeArgBinding {
    pub name: t::Word,
    pub equals: t::Equals,
    pub ty: Type,
}
impl FromCST for AssociatedTypeArgBinding {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ASSOCIATED_TYPE_DECL)?;
        let mut it = SyntaxNodeIter::new(&node);
        let name = it.expect_parse()?;
        let equals = it.expect_parse()?;
        let ty = it.expect_parse()?;
        it.expect_end()?;
        Ok(AssociatedTypeArgBinding { name, equals, ty })
    }
}
/// Corresponds to a [`SyntaxKind::TYPE_ARGS`] node.
#[derive(Debug)]
pub struct TypeArgs {
    pub open_angle: t::Less,
    pub first: Box<TypeArg>,
    pub rest: Vec<(t::Comma, TypeArg)>,
    pub close_angle: t::Greater,
}
impl FromCST for TypeArgs {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::TYPE_ARGS)?;
        let mut it = SyntaxNodeIter::new(&node);
        let open_angle: t::Less = it.expect_parse()?;
        let first = TypeArg::from_cst(it.expect_next("type argument")?)?;
        let mut rest = Vec::new();
        let close_angle = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::GREATER, it.parent));
            };
            match elem.kind() {
                SyntaxKind::COMMA => {
                    let comma = t::Comma::from_cst(elem)?;
                    let Some(next_elem) = it.peek() else {
                        return Err(StrongAstError::missing(SyntaxKind::GREATER, it.parent));
                    };
                    if next_elem.kind() == SyntaxKind::GREATER {
                        continue;
                    }
                    let next = TypeArg::from_cst(it.expect_next("type argument")?)?;
                    rest.push((comma, next));
                }
                SyntaxKind::GREATER => {
                    break t::Greater::from_cst(elem)?;
                }
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "COMMA or GREATER".into(),
                        found: elem.kind(),
                        at: elem.text_range(),
                    });
                }
            }
        };
        it.expect_end()?;
        Ok(TypeArgs {
            open_angle,
            first: Box::new(first),
            rest,
            close_angle,
        })
    }
}
impl KnownKind for TypeArgs {
    fn kind() -> SyntaxKind {
        SyntaxKind::TYPE_ARGS
    }
}
#[derive(Debug)]
pub struct FunctionType {
    pub open_paren: t::LParen,
    pub params: Vec<(FunctionTypeParam, Option<t::Comma>)>,
    pub close_paren: t::RParen,
    pub arrow: t::Arrow,
    pub return_type: Box<Type>,
    pub throws: Option<Box<ThrowsClause>>,
}
/// Corresponds to a [`SyntaxKind::FUNCTION_TYPE_PARAM`] node.
///
/// Exists in [`FunctionType`] but also in [`ParenType`] for some reason.
#[derive(Debug)]
pub struct FunctionTypeParam {
    pub name: Option<(t::Word, Option<t::Question>, Option<t::Colon>)>,
    pub ty: Type,
}
impl FromCST for FunctionTypeParam {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        let mut it = SyntaxNodeIter::new(&node);
        let name = if let Some(name) = it.next_if_kind(SyntaxKind::WORD) {
            let name = t::Word::new_from_span(name.text_range());
            let question = it
                .next_if_kind(SyntaxKind::QUESTION)
                .map(t::Question::from_cst)
                .transpose()?;
            let colon = it
                .next_if_kind(SyntaxKind::COLON)
                .map(t::Colon::from_cst)
                .transpose()?;
            Some((name, question, colon))
        } else {
            None
        };
        let ty: Type = it.expect_parse()?;
        it.expect_end()?;
        Ok(FunctionTypeParam { name, ty })
    }
}
/// The type argument is what type enumeration is being constrained.
/// Generally either use [`Type`] or [`UnionTypeMember`].
#[derive(Debug)]
pub struct ConstrainedType<T> {
    pub ty: Box<T>,
    /// Should not be empty: if it is, just use the inner type
    pub attrs: Vec<Attribute>,
}
impl From<ConstrainedType<UnionTypeMember>> for ConstrainedType<Type> {
    fn from(member: ConstrainedType<UnionTypeMember>) -> Self {
        ConstrainedType {
            ty: Box::new((*member.ty).into()),
            attrs: member.attrs,
        }
    }
}
