//! Reference: [baml_compiler_syntax::type_ref], though many of the types are grouped into [`Type::Path`] for us,
//! since we shouldn't need special treatment for things like `string` and `int` during formatting.
//! If this ever gets used for something else, we can split it up into multiple types.

use baml_compiler_syntax::{SyntaxElement, SyntaxKind};

use super::{FromCST, StrongAstError, tokens as t};
use rowan::TextRange;

/// Corresponds to a [`SyntaxKind::TYPE_EXPR`] node.
#[derive(Debug)]
pub enum Type {
    Path(PathType),
    String(StringType),
    Union(UnionType),
    Optional(OptionalType),
    Array(ArrayType),
    Generic(GenericType),
    Function(FunctionType),
    /// Types constrained by attributes.
    Constrained(TextRange), // TODO
    Unknown(TextRange),
}

impl FromCST for Type {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::TYPE_EXPR)?;

        // TYPE_EXPR contains tokens and nodes directly in a flat structure
        // We need to parse them into the appropriate Type variant

        // For now, implement a simplified version that handles common cases
        // and returns Unknown for complex cases
        let children: Vec<SyntaxElement> = node
            .children_with_tokens()
            .by_kind(|kind| !kind.is_trivia())
            .collect();

        if children.is_empty() {
            return Ok(Type::Unknown(node.text_range()));
        }

        // Try to parse the type expression
        parse_type_from_elements(&children, node.text_range())
    }
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
    pub first: Box<Type>,
    pub rest: Vec<(t::Pipe, Box<Type>)>,
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
    pub open_angle: t::Less,
    pub params: TextRange, // TODO
    pub close_angle: t::Greater,
}

#[derive(Debug)]
pub struct FunctionType {
    pub open_paren: t::LParen,
    pub params: Vec<FunctionTypeParam>,
    pub close_paren: t::RParen,
    pub arrow: t::Arrow,
    pub return_type: Box<Type>,
}

#[derive(Debug)]
pub struct FunctionTypeParam {
    pub name: Option<(t::Word, Option<t::Colon>)>,
    pub ty: Box<Type>,
    pub comma: Option<t::Comma>,
}

fn parse_type_from_elements(
    elements: &[SyntaxElement],
    full_range: TextRange,
) -> Result<Type, StrongAstError> {
    if elements.is_empty() {
        return Ok(Type::Unknown(full_range));
    }

    // First, handle function types (start with L_PAREN)
    if elements.first().map(|e| e.kind()) == Some(SyntaxKind::L_PAREN) {
        return parse_function_type(elements, full_range);
    }

    // Parse base type (could be Path, String, Integer, etc.)
    let (base_type, rest) = parse_base_type(elements, full_range)?;

    // Handle postfix operators: [], ?, |
    parse_postfix_type(base_type, rest, full_range)
}

fn parse_base_type(
    elements: &[SyntaxElement],
    full_range: TextRange,
) -> Result<(Type, &[SyntaxElement]), StrongAstError> {
    if elements.is_empty() {
        return Ok((Type::Unknown(full_range), elements));
    }

    let first = &elements[0];

    match first.kind() {
        SyntaxKind::WORD => {
            // Could be a simple path like "string" or dotted path like "User.Name"
            let mut path_parts = vec![t::Word {
                token_span: first.text_range(),
            }];
            let mut idx = 1;
            let mut dots = Vec::new();

            while idx + 1 < elements.len() {
                if elements[idx].kind() == SyntaxKind::DOT
                    && elements[idx + 1].kind() == SyntaxKind::WORD
                {
                    dots.push(t::Dot::new_from_span(elements[idx].text_range()));
                    path_parts.push(t::Word {
                        token_span: elements[idx + 1].text_range(),
                    });
                    idx += 2;
                } else {
                    break;
                }
            }

            if dots.is_empty() {
                Ok((
                    Type::Path(PathType {
                        first: path_parts[0].clone(),
                        rest: Vec::new(),
                    }),
                    &elements[idx..],
                ))
            } else {
                let first = path_parts.remove(0);
                let rest = dots.into_iter().zip(path_parts).collect();
                Ok((Type::Path(PathType { first, rest }), &elements[idx..]))
            }
        }
        SyntaxKind::STRING_LITERAL => {
            let token = StrongAstError::assert_is_token(first.clone())?;
            Ok((
                Type::String(StringType(t::QuotedString {
                    token_span: token.text_range(),
                })),
                &elements[1..],
            ))
        }
        SyntaxKind::INTEGER_LITERAL => {
            // Integer literals in types (like "200" for status codes)
            // Treat as a path for now
            Ok((
                Type::Path(PathType {
                    first: t::Word {
                        token_span: first.text_range(),
                    },
                    rest: Vec::new(),
                }),
                &elements[1..],
            ))
        }
        _ => Ok((Type::Unknown(full_range), elements)),
    }
}

fn parse_postfix_type(
    mut base: Type,
    mut rest: &[SyntaxElement],
    full_range: TextRange,
) -> Result<Type, StrongAstError> {
    loop {
        if rest.is_empty() {
            return Ok(base);
        }

        match rest[0].kind() {
            SyntaxKind::L_BRACKET => {
                // Array type
                if rest.len() >= 2 && rest[1].kind() == SyntaxKind::R_BRACKET {
                    base = Type::Array(ArrayType {
                        ty: Box::new(base),
                        brackets: vec![(
                            t::LBracket::new_from_span(rest[0].text_range()),
                            t::RBracket::new_from_span(rest[1].text_range()),
                        )],
                    });
                    rest = &rest[2..];
                } else {
                    return Ok(Type::Unknown(full_range));
                }
            }
            SyntaxKind::QUESTION => {
                // Optional type
                base = Type::Optional(OptionalType {
                    ty: Box::new(base),
                    question: t::Question::new_from_span(rest[0].text_range()),
                });
                rest = &rest[1..];
            }
            SyntaxKind::PIPE => {
                // Union type
                let pipe = t::Pipe::new_from_span(rest[0].text_range());
                rest = &rest[1..];

                let (next_type, remaining) = parse_base_type(rest, full_range)?;
                let next_type = parse_postfix_type(next_type, remaining, full_range)?;

                // Convert to union
                match base {
                    Type::Union(UnionType {
                        first,
                        rest: mut union_rest,
                    }) => {
                        union_rest.push((pipe, Box::new(next_type)));
                        base = Type::Union(UnionType {
                            first,
                            rest: union_rest,
                        });
                    }
                    _ => {
                        base = Type::Union(UnionType {
                            first: Box::new(base),
                            rest: vec![(pipe, Box::new(next_type))],
                        });
                    }
                }
                rest = &[];
            }
            SyntaxKind::LESS => {
                // Generic type
                if let Some(close_idx) = rest.iter().position(|e| e.kind() == SyntaxKind::GREATER) {
                    let params_start = rest[0].text_range().end();
                    let params_end = rest[close_idx].text_range().start();
                    base = Type::Generic(GenericType {
                        base: Box::new(base),
                        open_angle: t::Less::new_from_span(rest[0].text_range()),
                        params: TextRange::new(params_start, params_end), // TODO: parse params properly
                        close_angle: t::Greater::new_from_span(rest[close_idx].text_range()),
                    });
                    rest = &rest[close_idx + 1..];
                } else {
                    return Ok(Type::Unknown(full_range));
                }
            }
            _ => {
                // Unknown postfix, stop parsing
                return Ok(base);
            }
        }
    }
}

fn parse_function_type(
    elements: &[SyntaxElement],
    full_range: TextRange,
) -> Result<Type, StrongAstError> {
    // For now, return Unknown for function types
    // TODO: Implement full function type parsing
    Ok(Type::Unknown(full_range))
}
