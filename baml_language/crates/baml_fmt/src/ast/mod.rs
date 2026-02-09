mod attributes;
mod declarations;
mod expressions;
mod generics;
mod pattern;
mod statements;
mod tokens;
mod types;

use std::borrow::Cow;

use crate::printer::*;
pub use attributes::*;
use baml_compiler_syntax::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};
pub use declarations::*;
pub use expressions::*;
pub use generics::*;
pub use pattern::*;
use rowan::TextRange;
pub use statements::*;
pub use tokens::*;
pub use types::*;

use crate::printer::Printable;

pub trait FromCST: Sized {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StrongAstError {
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
    /// Returns [`StrongAstError::UnexpectedKind`] if the element not the expected kind.
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
    pub fn missing_desc(desc: impl Into<Cow<'static, str>>, parent: TextRange) -> Self {
        let desc = desc.into();
        Self::MissingExpectedElementDesc { desc, parent }
    }
    /// Easy way to create a [`StrongAstError::MissingExpectedElement`] error.
    pub const fn missing(expected: SyntaxKind, parent: TextRange) -> Self {
        Self::MissingExpectedElement { expected, parent }
    }
    /// Checks that the given element is a node.
    /// - Returns [`StrongAstError::ShouldBeNode`] if the element is a token.
    /// - Otherwise returns the node.
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
}

/// Helps walk through the non-trivia children of a [`SyntaxNode`].
/// Used for parsing CST nodes into strong AST nodes.
pub struct SyntaxNodeIter {
    it: Box<dyn Iterator<Item = SyntaxElement>>,
    parent: TextRange,
    peeked: Option<SyntaxElement>,
}
impl SyntaxNodeIter {
    pub fn new(parent_node: SyntaxNode) -> SyntaxNodeIter {
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
    /// - If the element is not a node, returns [`StrongAstError::ShouldBeNode`].
    /// - If the element is a node but not of the expected kind, returns [`StrongAstError::UnexpectedKind`].
    /// - Otherwise, returns the node.
    ///
    /// Consumes an element even if it returns an error.
    pub fn expect_node_of_kind(&mut self, kind: SyntaxKind) -> Result<SyntaxNode, StrongAstError> {
        let Some(elem) = self.next() else {
            return Err(StrongAstError::missing(kind, self.parent));
        };
        let SyntaxElement::Node(node) = elem else {
            return Err(StrongAstError::ShouldBeNode {
                at: elem.text_range(),
            });
        };

        if node.kind() == kind {
            Ok(node)
        } else {
            Err(StrongAstError::UnexpectedKind {
                expected: kind,
                found: node.kind(),
                at: node.text_range(),
            })
        }
    }

    /// Consumes the next element and checks it:
    /// - If there are no more elements, returns [`StrongAstError::MissingExpectedElement`].
    /// - If the element is not a token, returns [`StrongAstError::ShouldBeToken`].
    /// - If the element is a token but not of the expected kind, returns [`StrongAstError::UnexpectedKind`].
    /// - Otherwise, returns the token.
    ///
    /// Consumes an element even if it returns an error.
    pub fn expect_token_of_kind(
        &mut self,
        kind: SyntaxKind,
    ) -> Result<SyntaxToken, StrongAstError> {
        let Some(elem) = self.next() else {
            return Err(StrongAstError::missing(kind, self.parent));
        };
        let SyntaxElement::Token(token) = elem else {
            return Err(StrongAstError::ShouldBeToken {
                at: elem.text_range(),
            });
        };

        if token.kind() == kind {
            Ok(token)
        } else {
            Err(StrongAstError::UnexpectedKind {
                expected: kind,
                found: token.kind(),
                at: token.text_range(),
            })
        }
    }

    /// Checks that there are no more elements left.
    /// Returns [`StrongAstError::UnexpectedAdditionalElement`] if there are.
    ///
    /// If it returns an error, the next element has been consumed.
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
        if let Some(ref peeked) = self.peek() {
            if f(peeked) {
                return self.peeked.take();
            }
        }
        None
    }

    /// Peeks at the next element and:
    /// - If there is no next element, returns `None`.
    /// - Calls the given function with the next element, if it returns `Some(t)` then the element is consumed and `Some(t)` is returned.
    /// - Otherwise, the next element is not consumed and `None` is returned.
    pub fn next_if_and_map<T, F: FnOnce(&SyntaxElement) -> Option<T>>(
        &mut self,
        f: F,
    ) -> Option<T> {
        if let Some(ref peeked) = self.peek() {
            if let Some(t) = f(peeked) {
                self.peeked = None;
                return Some(t);
            }
        }
        None
    }
}
impl Iterator for SyntaxNodeIter {
    type Item = SyntaxElement;
    fn next(&mut self) -> Option<Self::Item> {
        self.peeked.take().or_else(|| self.it.next())
    }
}

#[derive(Debug)]
pub struct SourceFile {
    pub items: Vec<TopLevelDeclaration>,
}

impl FromCST for SourceFile {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::SOURCE_FILE)?;

        let mut it = SyntaxNodeIter::new(node);

        let mut items = Vec::new();
        while let Some(elem) = it.next() {
            let item = TopLevelDeclaration::from_cst(elem)?;
            items.push(item);
        }

        Ok(SourceFile { items })
    }
}

impl Printable for SourceFile {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        assert_eq!(shape.indent, 0);
        assert_eq!(shape.first_line_offset, 0);
        assert_eq!(shape.width, printer.config.line_width);

        for decl in &self.items {
            let _ = printer.print(decl, shape.clone());
            printer.print_newline();
        }

        printer.print_newline();

        PrintInfo::default_multi_lined()
    }
}

#[cfg(test)]
mod tests {
    use baml_compiler_parser::parse_green;
    use baml_compiler_syntax::SyntaxNode;
    use baml_project::ProjectDatabase;

    use super::*;

    #[test]
    fn test_parse_source_file() {
        let source = r#"
            function MyFunction(a: MyType) -> int {
                if (a > 0) {
                    1
                } else {1}
            }

            enum MyEnum {
                A,
                B
                C
            }
            "#;

        let mut db = ProjectDatabase::new();
        let file = db.add_file("test.baml", source);
        let parsed = parse_green(&mut db, file);
        let syntax_tree = SyntaxNode::new_root(parsed);
        let source_file = SourceFile::from_cst(SyntaxElement::Node(syntax_tree)).unwrap();

        assert_eq!(source_file.items.len(), 2);
    }
}
