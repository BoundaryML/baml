use std::{marker::PhantomData, ops::Range};

use rowan::ast::AstNode;

use super::StrongAstError;
use crate::{BamlLanguage, SyntaxElement, SyntaxKind, SyntaxNode, TextRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct ElementId(u32);

#[derive(Debug)]
struct NodeRecord {
    syntax: SyntaxNode,
    elements: Range<u32>,
    fields: Range<u32>,
}

#[derive(Debug)]
pub(super) struct ElementRecord {
    element: SyntaxElement,
    child: Option<NodeId>,
}

#[derive(Debug, Clone, Default)]
struct FieldRecord {
    elements: Range<u32>,
}

#[derive(Debug)]
pub struct ValidatedTree {
    nodes: Vec<NodeRecord>,
    elements: Vec<ElementRecord>,
    fields: Vec<FieldRecord>,
    field_elements: Vec<ElementId>,
    root: NodeId,
}

#[derive(Debug)]
pub struct Validated<'tree, N> {
    tree: &'tree ValidatedTree,
    id: NodeId,
    marker: PhantomData<fn() -> N>,
}

impl<N> Clone for Validated<'_, N> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<N> Copy for Validated<'_, N> {}

#[derive(Debug, Clone, Copy)]
pub struct ValidatedElement<'tree> {
    tree: &'tree ValidatedTree,
    id: ElementId,
}

pub struct ValidatedElements<'tree> {
    tree: &'tree ValidatedTree,
    range: Range<u32>,
    position: u32,
}

pub struct ValidatedDirectElements<'tree> {
    tree: &'tree ValidatedTree,
    range: Range<u32>,
    position: u32,
}

impl<'tree> Iterator for ValidatedDirectElements<'tree> {
    type Item = ValidatedElement<'tree>;

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.range.start.checked_add(self.position)?;
        if index >= self.range.end {
            return None;
        }
        self.position += 1;
        Some(ValidatedElement {
            tree: self.tree,
            id: ElementId(index),
        })
    }
}

impl<'tree> Iterator for ValidatedElements<'tree> {
    type Item = ValidatedElement<'tree>;

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.range.start.checked_add(self.position)?;
        if index >= self.range.end {
            return None;
        }
        self.position += 1;
        let id = self.tree.field_elements[index as usize];
        Some(ValidatedElement {
            tree: self.tree,
            id,
        })
    }
}

impl<'tree> ValidatedElement<'tree> {
    #[must_use]
    pub fn kind(&self) -> SyntaxKind {
        self.tree.element(self.id).element.kind()
    }

    #[must_use]
    pub fn text_range(&self) -> TextRange {
        self.tree.element(self.id).element.text_range()
    }

    #[must_use]
    pub fn node<N>(&self) -> Option<Validated<'tree, N>>
    where
        N: AstNode<Language = BamlLanguage>,
    {
        let element = self.tree.element(self.id);
        let id = element.child?;
        N::can_cast(element.element.kind()).then(|| Validated::new(self.tree, id))
    }

    #[must_use]
    pub fn token(&self) -> Option<ValidatedSyntaxToken> {
        self.tree
            .element(self.id)
            .element
            .as_token()
            .map(|token| ValidatedSyntaxToken {
                kind: token.kind(),
                range: token.text_range(),
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValidatedSyntaxToken {
    kind: SyntaxKind,
    range: TextRange,
}

impl ValidatedSyntaxToken {
    #[must_use]
    pub const fn kind(self) -> SyntaxKind {
        self.kind
    }

    #[must_use]
    pub const fn text_range(self) -> TextRange {
        self.range
    }
}

impl super::ValidatedToken for ValidatedSyntaxToken {
    fn span(&self) -> TextRange {
        self.range
    }
}

pub struct ValidatedChildren<'tree, N> {
    tree: &'tree ValidatedTree,
    range: Range<u32>,
    position: u32,
    marker: PhantomData<fn() -> N>,
}

impl<'tree, N> Iterator for ValidatedChildren<'tree, N>
where
    N: AstNode<Language = BamlLanguage>,
{
    type Item = Validated<'tree, N>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let index = self.range.start.checked_add(self.position)?;
            if index >= self.range.end {
                return None;
            }
            self.position += 1;
            let element = self.tree.element(self.tree.field_elements[index as usize]);
            let Some(id) = element.child else {
                continue;
            };
            if N::can_cast(element.element.kind()) {
                return Some(Validated::new(self.tree, id));
            }
        }
    }
}

impl ValidatedTree {
    pub fn new(root: SyntaxNode) -> Result<Self, StrongAstError> {
        let mut builder = ArenaBuilder::default();
        let root = builder.add_node(root)?;
        Ok(Self {
            nodes: builder.nodes,
            elements: builder.elements,
            fields: builder.fields,
            field_elements: builder.field_elements,
            root,
        })
    }

    pub fn root<N>(&self) -> Option<Validated<'_, N>>
    where
        N: AstNode<Language = BamlLanguage>,
    {
        let root = self.root;
        N::can_cast(self.node(root).syntax.kind()).then(|| Validated::new(self, root))
    }

    fn node(&self, id: NodeId) -> &NodeRecord {
        &self.nodes[id.0 as usize]
    }

    fn element(&self, id: ElementId) -> &ElementRecord {
        &self.elements[id.0 as usize]
    }

    fn field(&self, node: NodeId, slot: usize) -> Range<u32> {
        let record = self.node(node);
        let field = &self.fields[record.fields.start as usize + slot];
        field.elements.clone()
    }
}

impl<'tree, N> Validated<'tree, N>
where
    N: AstNode<Language = BamlLanguage>,
{
    fn new(tree: &'tree ValidatedTree, id: NodeId) -> Self {
        debug_assert!(N::can_cast(tree.node(id).syntax.kind()));
        Self {
            tree,
            id,
            marker: PhantomData,
        }
    }

    #[must_use]
    pub fn syntax(&self) -> &SyntaxNode {
        &self.tree.node(self.id).syntax
    }

    #[must_use]
    pub fn text_range(&self) -> TextRange {
        self.syntax().text_range()
    }

    #[must_use]
    pub fn first_token_range(&self) -> TextRange {
        self.syntax()
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| !token.kind().is_trivia())
            .map_or_else(|| self.text_range(), |token| token.text_range())
    }

    #[must_use]
    pub fn last_token_range(&self) -> TextRange {
        self.syntax()
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| !token.kind().is_trivia())
            .last()
            .map_or_else(|| self.text_range(), |token| token.text_range())
    }

    pub fn direct_elements(&self) -> ValidatedDirectElements<'tree> {
        ValidatedDirectElements {
            tree: self.tree,
            range: self.tree.node(self.id).elements.clone(),
            position: 0,
        }
    }

    #[must_use]
    pub fn cast<M>(self) -> Option<Validated<'tree, M>>
    where
        M: AstNode<Language = BamlLanguage>,
    {
        M::can_cast(self.syntax().kind()).then(|| Validated::new(self.tree, self.id))
    }

    pub(crate) fn child<M>(&self, slot: usize) -> Option<Validated<'tree, M>>
    where
        M: AstNode<Language = BamlLanguage>,
    {
        self.children(slot).next()
    }

    pub(crate) fn elements(&self, slot: usize) -> ValidatedElements<'tree> {
        let range = self.tree.field(self.id, slot);
        ValidatedElements {
            tree: self.tree,
            range,
            position: 0,
        }
    }

    pub(crate) fn children<M>(&self, slot: usize) -> ValidatedChildren<'tree, M>
    where
        M: AstNode<Language = BamlLanguage>,
    {
        let range = self.tree.field(self.id, slot);
        ValidatedChildren {
            tree: self.tree,
            range,
            position: 0,
            marker: PhantomData,
        }
    }

    pub(crate) fn token(&self, slot: usize) -> Option<ValidatedSyntaxToken> {
        let range = self.tree.field(self.id, slot);
        self.tree.field_elements[range.start as usize..range.end as usize]
            .iter()
            .find_map(|id| {
                let element = self.tree.element(*id);
                element
                    .element
                    .as_token()
                    .map(|token| ValidatedSyntaxToken {
                        kind: token.kind(),
                        range: token.text_range(),
                    })
            })
    }
}

#[derive(Default)]
struct ArenaBuilder {
    nodes: Vec<NodeRecord>,
    elements: Vec<ElementRecord>,
    fields: Vec<FieldRecord>,
    field_elements: Vec<ElementId>,
}

impl ArenaBuilder {
    fn add_node(&mut self, syntax: SyntaxNode) -> Result<NodeId, StrongAstError> {
        let id = NodeId(u32::try_from(self.nodes.len()).expect("validated AST has too many nodes"));
        self.nodes.push(NodeRecord {
            syntax: syntax.clone(),
            elements: 0..0,
            fields: 0..0,
        });

        let mut direct = Vec::new();
        for element in syntax
            .children_with_tokens()
            .filter(|element| !element.kind().is_trivia())
        {
            let child = element
                .as_node()
                .map(|node| self.add_node(node.clone()))
                .transpose()?;
            direct.push(ElementRecord { element, child });
        }

        let element_start = u32::try_from(self.elements.len()).expect("validated AST is too large");
        self.elements.extend(direct);
        let element_end = u32::try_from(self.elements.len()).expect("validated AST is too large");
        let element_ids = (element_start..element_end)
            .map(ElementId)
            .collect::<Vec<_>>();

        let captures =
            super::generated_schema::validate_node(&syntax, &element_ids, &self.elements)?;
        let field_start = u32::try_from(self.fields.len()).expect("validated AST is too large");
        for capture in captures {
            let start =
                u32::try_from(self.field_elements.len()).expect("validated AST is too large");
            self.field_elements.extend(capture);
            let end = u32::try_from(self.field_elements.len()).expect("validated AST is too large");
            self.fields.push(FieldRecord {
                elements: start..end,
            });
        }
        let field_end = u32::try_from(self.fields.len()).expect("validated AST is too large");
        self.nodes[id.0 as usize] = NodeRecord {
            syntax,
            elements: element_start..element_end,
            fields: field_start..field_end,
        };
        Ok(id)
    }
}

pub(super) fn element_kind(elements: &[ElementRecord], id: ElementId) -> SyntaxKind {
    elements[id.0 as usize].element.kind()
}

pub(super) fn element_is_node(elements: &[ElementRecord], id: ElementId) -> bool {
    elements[id.0 as usize].child.is_some()
}
