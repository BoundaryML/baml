use crate::ast::{self, top_idx_to_top_id, traits::*};

impl ast::SchemaAst {
    /// Find the AST node at the given position (byte offset).
    pub fn find_at_position(&self, position: usize) -> SchemaPosition<'_> {
        self.find_top_at_position(position)
            .map(|top_id| match top_id {
                ast::TopId::Enum(enum_id) => {
                    SchemaPosition::Enum(enum_id, EnumPosition::new(&self[enum_id], position))
                }
                // Falling back to TopLevel as "not implemented"
                _ => SchemaPosition::TopLevel,
            })
            // If no top matched, we're in between top-level items. This is normal and expected.
            .unwrap_or(SchemaPosition::TopLevel)
    }

    /// Do a binary search for the `Top` at the given byte offset.
    pub fn find_top_at_position(&self, position: usize) -> Option<ast::TopId> {
        use std::cmp::Ordering;

        let top_idx = self.tops.binary_search_by(|top| {
            let span = top.span();

            if span.start > position {
                Ordering::Greater
            } else if span.end < position {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        });

        top_idx
            .map(|idx| top_idx_to_top_id(idx, &self.tops[idx]))
            .ok()
    }
}

/// A cursor position in a schema.
#[derive(Debug)]
pub enum SchemaPosition<'ast> {
    /// In-between top-level items
    TopLevel,
    /// In an enum
    Enum(ast::EnumId, EnumPosition<'ast>),
}

/// A cursor position in a context.
#[derive(Debug)]
pub enum EnumPosition<'ast> {
    /// In the enum, but not somewhere more specific.
    Enum,
    // /// In an attribute (attr name, attr index, position).
    // EnumAttribute(&'ast str, usize, AttributePosition<'ast>),
    /// In a value.
    Value(ast::EnumValueId, EnumValuePosition<'ast>),
}

impl<'ast> EnumPosition<'ast> {
    fn new(r#enum: &'ast ast::Enum, position: usize) -> Self {
        for (enum_value_id, value) in r#enum.iter_values() {
            if value.span().contains(position) {
                return EnumPosition::Value(enum_value_id, EnumValuePosition::new(value, position));
            }
        }

        // for (attr_id, attr) in r#enum.attributes.iter().enumerate() {
        //     if attr.span().contains(position) {
        //         return EnumPosition::EnumAttribute(&attr.name.name, attr_id, AttributePosition::new(attr, position));
        //     }
        // }

        EnumPosition::Enum
    }
}

/// In an enum value.
#[derive(Debug)]
pub enum EnumValuePosition<'ast> {
    /// Nowhere specific inside the value
    Value,
    /// In an attribute. (name, idx, optional arg)
    Attribute(&'ast str, usize, Option<&'ast str>),
}

impl<'ast> EnumValuePosition<'ast> {
    fn new(_value: &'ast ast::EnumValue, _position: usize) -> EnumValuePosition<'ast> {
        // for (attr_idx, attr) in value.attributes.iter().enumerate() {
        //     if attr.span().contains(position) {
        //         // We can't go by Span::contains() because we also care about the empty space
        //         // between arguments and that's hard to capture in the pest grammar.
        //         let mut spans: Vec<(Option<&str>, ast::Span)> = attr
        //             .arguments
        //             .iter()
        //             .map(|arg| (arg.name.as_ref().map(|n| n.name.as_str()), arg.span()))
        //             .chain(
        //                 attr.arguments
        //                     .empty_arguments
        //                     .iter()
        //                     .map(|arg| (Some(arg.name.name.as_str()), arg.name.span())),
        //             )
        //             .collect();
        //         spans.sort_by_key(|(_, span)| span.start);
        //         let mut arg_name = None;

        //         for (name, _) in spans.iter().take_while(|(_, span)| span.start < position) {
        //             arg_name = Some(*name);
        //         }

        //         // If the cursor is after a trailing comma, we're not in an argument.
        //         if let Some(span) = attr.arguments.trailing_comma {
        //             if position > span.start {
        //                 arg_name = None;
        //             }
        //         }

        //         return EnumValuePosition::Attribute(attr.name(), attr_idx, arg_name.flatten());
        //     }
        // }

        EnumValuePosition::Value
    }
}
