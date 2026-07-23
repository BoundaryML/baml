use super::token as t;
use super::{
    AstElement, AstShapeError, AstToken, BinaryOp, HeaderComment, Literal, SyntaxElement,
    SyntaxKind, SyntaxNode, SyntaxToken, TextRange, UnaryOp, ValidatedAttribute as Attribute,
    ValidatedBlockAttribute as BlockAttribute, ValidatedBreakStmt as BreakStmt,
    ValidatedContinueStmt as ContinueStmt,
};
use std::{borrow::Cow, path::Path};

macro_rules! parse_validated_field {
    ($iter:ident, $field:ident, required $ty:ty) => {
        $iter.expect_parse::<$ty>()?
    };
    ($iter:ident, $field:ident, boxed $ty:ty) => {
        Box::new(<$ty>::from_cst($iter.expect_next(stringify!($field))?)?)
    };
    ($iter:ident, $field:ident, optional $ty:ty) => {
        $iter
            .next_if_kind(<$ty as KnownKind>::kind())
            .map(<$ty>::from_cst)
            .transpose()?
    };
    ($iter:ident, $field:ident, optional_element $ty:ty) => {
        $iter
            .next_if(|element| <$ty as $crate::AstElement>::can_cast_element(element.kind()))
            .map(<$ty>::from_cst)
            .transpose()?
    };
    ($iter:ident, $field:ident, optional_boxed $ty:ty) => {
        $iter
            .next_if_kind(<$ty as KnownKind>::kind())
            .map(<$ty>::from_cst)
            .transpose()?
            .map(Box::new)
    };
    ($iter:ident, $field:ident, rest $ty:ty) => {{
        let mut values = Vec::new();
        for element in &mut $iter {
            values.push(<$ty>::from_cst(element)?);
        }
        values
    }};
}

macro_rules! validated_ast_node {
    (
        $(#[$meta:meta])*
        $name:ident, $kind:ident {
            $(
                $(#[$field_meta:meta])*
                $field:ident: $mode:ident $field_ty:ty;
            )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug)]
        pub struct $name {
            $(
                $(#[$field_meta])*
                pub $field: validated_ast_node!(@field_type $mode $field_ty),
            )*
        }

        impl FromCST for $name {
            fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
                let node = StrongAstError::assert_is_node(elem)?;
                StrongAstError::assert_kind_node(&node, SyntaxKind::$kind)?;
                let mut iter = SyntaxNodeIter::new(&node);
                $(
                    let $field = parse_validated_field!(iter, $field, $mode $field_ty);
                )*
                iter.expect_end()?;
                Ok(Self { $($field,)* })
            }
        }

        impl KnownKind for $name {
            fn kind() -> SyntaxKind {
                SyntaxKind::$kind
            }
        }
    };
    (@field_type required $field_ty:ty) => {
        $field_ty
    };
    (@field_type boxed $field_ty:ty) => {
        Box<$field_ty>
    };
    (@field_type optional $field_ty:ty) => {
        Option<$field_ty>
    };
    (@field_type optional_element $field_ty:ty) => {
        Option<$field_ty>
    };
    (@field_type optional_boxed $field_ty:ty) => {
        Option<Box<$field_ty>>
    };
    (@field_type rest $field_ty:ty) => {
        Vec<$field_ty>
    };
    (
        $(#[$meta:meta])*
        custom $name:ident, $kind:ident, $parser:ident {
            $(
                $(#[$field_meta:meta])*
                $field:ident: $field_ty:ty,
            )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug)]
        pub struct $name {
            $(
                $(#[$field_meta])*
                pub $field: $field_ty,
            )*
        }

        impl FromCST for $name {
            fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
                $parser(elem)
            }
        }

        impl KnownKind for $name {
            fn kind() -> SyntaxKind {
                SyntaxKind::$kind
            }
        }
    };
    (
        custom $name:ident, $kind:ident, $parser:ident,
        $item:item
    ) => {
        #[derive(Debug)]
        $item

        impl FromCST for $name {
            fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
                $parser(elem)
            }
        }

        impl KnownKind for $name {
            fn kind() -> SyntaxKind {
                SyntaxKind::$kind
            }
        }
    };
}

macro_rules! validated_ast_data {
    ($item:item) => {
        #[derive(Debug)]
        $item
    };
}

mod declarations;
mod expressions;
mod pattern;
mod statements;
mod types;

pub use declarations::*;
pub use expressions::*;
pub use pattern::*;
pub use statements::*;
pub use types::*;

pub trait FromCST: Sized {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError>;
}
impl<T: AstElement> FromCST for T {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        T::cast_element(elem).map_err(Into::into)
    }
}
/// This AST node only ever has exactly one [`SyntaxKind`].
///
/// Helps with conveniently printing error messages.
pub trait KnownKind {
    /// Should be constant, but we can't use `const` because it's a trait.
    fn kind() -> SyntaxKind;
}
/// Errors that can occur when parsing from a [`SyntaxNode`] with [`FromCST`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror :: Error)]
pub enum StrongAstError {
    #[error("{0}")]
    AstShape(#[from] AstShapeError),
    /// When an element is expected (of a specific [`SyntaxKind`]) but was found to be of a different kind.
    #[error("Expected token/node of kind {expected:?}, but found {found:?} at {at:?}")]
    UnexpectedKind {
        expected: SyntaxKind,
        found: SyntaxKind,
        at: TextRange,
    },
    /// When an element is expected but was found to be of a different kind.
    #[error("Expected token/node {expected_desc}, but found {found:?} at {at:?}")]
    UnexpectedKindDesc {
        expected_desc: Cow<'static, str>,
        found: SyntaxKind,
        at: TextRange,
    },
    /// When an element is expected (of a specific [`SyntaxKind`]) but there were no more children left.
    #[error("Expected token/node of kind {expected:?}, but was unable to find it in {parent:?}")]
    MissingExpectedElement {
        expected: SyntaxKind,
        parent: TextRange,
    },
    /// When an element is expected (not of a single specific [`SyntaxKind`]) but there were no more children left.
    #[error("Expected token/node {desc}, but was unable to find it in {parent:?}")]
    MissingExpectedElementDesc {
        desc: Cow<'static, str>,
        parent: TextRange,
    },
    /// When the node isn't expected to have any more children (e.g. a statement found a `;`) but there are still children left.
    #[error("Unexpected additional element at {at:?} in {parent:?}")]
    UnexpectedAdditionalElement { parent: TextRange, at: TextRange },
    /// When an element is expected to be a node but it's actually a token.
    #[error("An element at {at:?} was a node when it should have been a token.")]
    ShouldBeNode { at: TextRange },
    /// When an element is expected to be a token but it's actually a node.
    #[error("An element at {at:?} was a token when it should have been a node.")]
    ShouldBeToken { at: TextRange },
}
impl StrongAstError {
    /// Checks that the given node is of the specified [`SyntaxKind`].
    ///
    /// # Errors
    /// Returns [`StrongAstError::UnexpectedKind`] if the element not the expected kind.
    pub fn assert_kind_node(node: &SyntaxNode, expected: SyntaxKind) -> Result<(), Self> {
        if node.kind() == expected {
            Ok(())
        } else {
            Err(Self::UnexpectedKind {
                expected,
                found: node.kind(),
                at: node.text_range(),
            })
        }
    }
    /// Checks that the given token is of the specified [`SyntaxKind`].
    ///
    /// # Errors
    /// Returns [`StrongAstError::UnexpectedKind`] if the element not the expected kind.
    #[allow(unused_must_use)]
    pub fn assert_kind_token(token: &SyntaxToken, expected: SyntaxKind) -> Result<(), Self> {
        if token.kind() == expected {
            Ok(())
        } else {
            Err(Self::UnexpectedKind {
                expected,
                found: token.kind(),
                at: token.text_range(),
            })
        }
    }
    /// Easy way to create a [`StrongAstError::MissingExpectedElementDesc`] error.
    #[must_use]
    pub fn missing_desc(desc: impl Into<Cow<'static, str>>, parent: TextRange) -> Self {
        let desc = desc.into();
        Self::MissingExpectedElementDesc { desc, parent }
    }
    /// Easy way to create a [`StrongAstError::MissingExpectedElement`] error.
    #[must_use]
    pub const fn missing(expected: SyntaxKind, parent: TextRange) -> Self {
        Self::MissingExpectedElement { expected, parent }
    }
    /// Checks that the given element is a node.
    /// - Returns [`StrongAstError::ShouldBeNode`] if the element is a token.
    /// - Otherwise returns the node.
    #[allow(unused_must_use)]
    pub fn assert_is_node(element: SyntaxElement) -> Result<SyntaxNode, Self> {
        match element {
            SyntaxElement::Node(node) => Ok(node),
            SyntaxElement::Token(token) => Err(Self::ShouldBeNode {
                at: token.text_range(),
            }),
        }
    }
    /// Checks that the given element is a token.
    /// - Returns [`StrongAstError::ShouldBeToken`] if the element is a node.
    /// - Otherwise returns the token.
    pub fn assert_is_token(element: SyntaxElement) -> Result<SyntaxToken, Self> {
        match element {
            SyntaxElement::Node(node) => Err(Self::ShouldBeToken {
                at: node.text_range(),
            }),
            SyntaxElement::Token(token) => Ok(token),
        }
    }
    /// A more human-readable error message.
    /// Includes file name and line/column numbers instead of just byte offsets.
    pub fn print_with_file_context(&self, file_path: impl AsRef<Path>, source: &str) -> String {
        fn get_line_and_column(source: &str, byte_offset: usize) -> Option<(usize, usize)> {
            let (before, _) = source.split_at_checked(byte_offset)?;
            let line = before.lines().count();
            let column = before.lines().last()?.len() + 1;
            Some((line, column))
        }
        match self {
            StrongAstError::AstShape(error) => error.to_string(),
            StrongAstError::UnexpectedKind {
                expected,
                found,
                at,
            } => {
                let Some((line, column)) = get_line_and_column(source, at.start().into()) else {
                    return self.to_string();
                };
                format!(
                    "Expected token/node of kind {expected:?}, but found {found:?} at {}:{}:{}",
                    file_path.as_ref().display(),
                    line,
                    column
                )
            }
            StrongAstError::UnexpectedKindDesc {
                expected_desc,
                found,
                at,
            } => {
                let Some((line, column)) = get_line_and_column(source, at.start().into()) else {
                    return self.to_string();
                };
                format!(
                    "Expected token/node {expected_desc}, but found {found:?} at {}:{}:{}",
                    file_path.as_ref().display(),
                    line,
                    column
                )
            }
            StrongAstError::MissingExpectedElement { expected, parent } => {
                let Some((line, column)) = get_line_and_column(source, parent.start().into())
                else {
                    return self.to_string();
                };
                format!(
                    "Expected token/node {expected:?}, but was unable to find it in {}:{}:{}",
                    file_path.as_ref().display(),
                    line,
                    column
                )
            }
            StrongAstError::MissingExpectedElementDesc { desc, parent } => {
                let Some((line, column)) = get_line_and_column(source, parent.start().into())
                else {
                    return self.to_string();
                };
                format!(
                    "Expected token/node {desc}, but was unable to find it in {}:{}:{}",
                    file_path.as_ref().display(),
                    line,
                    column
                )
            }
            StrongAstError::UnexpectedAdditionalElement { at, .. } => {
                let Some((line, column)) = get_line_and_column(source, at.start().into()) else {
                    return self.to_string();
                };
                format!(
                    "Unexpected additional element at {}:{}:{}",
                    file_path.as_ref().display(),
                    line,
                    column,
                )
            }
            StrongAstError::ShouldBeNode { at } => {
                let Some((line, column)) = get_line_and_column(source, at.start().into()) else {
                    return self.to_string();
                };
                format!(
                    "An element at {}:{}:{} was a node when it should have been a token.",
                    file_path.as_ref().display(),
                    line,
                    column,
                )
            }
            StrongAstError::ShouldBeToken { at } => {
                let Some((line, column)) = get_line_and_column(source, at.start().into()) else {
                    return self.to_string();
                };
                format!(
                    "An element at {}:{}:{} was a token when it should have been a node.",
                    file_path.as_ref().display(),
                    line,
                    column,
                )
            }
        }
    }
}
/// Helps walk through the non-trivia children of a [`SyntaxNode`].
/// Used for parsing CST nodes into strong AST nodes.
pub struct SyntaxNodeIter {
    it: Box<dyn Iterator<Item = SyntaxElement>>,
    parent: TextRange,
    peeked: Option<SyntaxElement>,
}
impl SyntaxNodeIter {
    /// Creates a new iterator to walk through the non-trivia children of a [`SyntaxNode`].
    #[must_use]
    pub fn new(parent_node: &SyntaxNode) -> SyntaxNodeIter {
        let it = parent_node
            .children_with_tokens()
            .by_kind(|kind| !kind.is_trivia());
        SyntaxNodeIter {
            it: Box::new(it),
            parent: parent_node.text_range(),
            peeked: None,
        }
    }
    /// Consumes the next element, returning [`StrongAstError::MissingExpectedElementDesc`] if it's not found, with the given description.
    /// Otherwise, returns the element.
    pub fn expect_next(
        &mut self,
        desc: impl Into<Cow<'static, str>>,
    ) -> Result<SyntaxElement, StrongAstError> {
        self.next()
            .ok_or_else(|| StrongAstError::missing_desc(desc.into(), self.parent))
    }
    /// Consumes the next element, returning [`StrongAstError::MissingExpectedElementDesc`] if it's not found, with the given description.
    /// Returns [`StrongAstError::ShouldBeNode`] if the element is not a node.
    /// Otherwise, returns the node.
    ///
    /// Consumes an element even if it returns an error.
    pub fn expect_node(
        &mut self,
        desc: impl Into<Cow<'static, str>>,
    ) -> Result<SyntaxNode, StrongAstError> {
        let Some(elem) = self.next() else {
            return Err(StrongAstError::missing_desc(desc.into(), self.parent));
        };
        let SyntaxElement::Node(node) = elem else {
            return Err(StrongAstError::ShouldBeNode {
                at: elem.text_range(),
            });
        };
        Ok(node)
    }
    /// Consumes the next element, returning [`StrongAstError::MissingExpectedElementDesc`] if it's not found, with the given description.
    /// Returns [`StrongAstError::ShouldBeToken`] if the element is not a token.
    /// Otherwise, returns the token.
    ///
    /// Consumes an element even if it returns an error.
    pub fn expect_token(
        &mut self,
        desc: impl Into<Cow<'static, str>>,
    ) -> Result<SyntaxToken, StrongAstError> {
        let Some(elem) = self.next() else {
            return Err(StrongAstError::missing_desc(desc.into(), self.parent));
        };
        let SyntaxElement::Token(token) = elem else {
            return Err(StrongAstError::ShouldBeToken {
                at: elem.text_range(),
            });
        };
        Ok(token)
    }
    /// Consumes the next element and checks it:
    /// - If there are no more elements, returns [`StrongAstError::MissingExpectedElement`].
    /// - Otherwise, the element will parse as the given type.
    ///
    /// Consumes an element even if it returns an error.
    pub fn expect_parse<T: FromCST>(&mut self) -> Result<T, StrongAstError> {
        let Some(elem) = self.next() else {
            return Err(StrongAstError::missing_desc(
                std::any::type_name::<T>(),
                self.parent,
            ));
        };
        T::from_cst(elem)
    }
    /// Checks that there are no more elements left.
    /// Returns [`StrongAstError::UnexpectedAdditionalElement`] if there are.
    ///
    /// If it returns an error, the next element has been consumed.
    ///
    /// # Errors
    /// Returns [`StrongAstError::UnexpectedAdditionalElement`] if there is any more elements.
    pub fn expect_end(&mut self) -> Result<(), StrongAstError> {
        let Some(elem) = self.next() else {
            return Ok(());
        };
        Err(StrongAstError::UnexpectedAdditionalElement {
            parent: self.parent,
            at: elem.text_range(),
        })
    }
    /// Peek at the next element without consuming it.
    /// Returns `None` if there are no more elements.
    pub fn peek(&mut self) -> Option<&SyntaxElement> {
        if let Some(ref peeked) = self.peeked {
            Some(peeked)
        } else {
            let next = self.next();
            self.peeked = next;
            self.peeked.as_ref()
        }
    }
    /// Peeks at the next element and:
    /// - If there is no next element, returns `None`.
    /// - Calls the given function with the next element, if it returns `true` then the element is consumed and `Some(next)` is returned.
    /// - Otherwise, the next element is not consumed and `None` is returned.
    pub fn next_if<F: FnOnce(&SyntaxElement) -> bool>(&mut self, f: F) -> Option<SyntaxElement> {
        if let Some(peeked) = self.peek()
            && f(peeked)
        {
            return self.peeked.take();
        }
        None
    }
    /// Peeks at the next element and:
    /// - If there is no next element, returns `None`.
    /// - If the kind matches, returns the element.
    /// - Otherwise, returns `None`.
    ///
    /// This is a convenience method equivalent to [`SyntaxNodeIter::next_if`] with `elem.kind() == kind`.
    pub fn next_if_kind(&mut self, kind: SyntaxKind) -> Option<SyntaxElement> {
        self.next_if(|elem| elem.kind() == kind)
    }
}
impl Iterator for SyntaxNodeIter {
    type Item = SyntaxElement;
    fn next(&mut self) -> Option<Self::Item> {
        self.peeked.take().or_else(|| self.it.next())
    }
}
validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::SOURCE_FILE`] node.
    ///
    /// This is the root node of the AST.
    SourceFile, SOURCE_FILE {
        items: rest TopLevelDeclaration;
    }
}
