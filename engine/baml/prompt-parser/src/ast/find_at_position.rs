// use crate::ast::{self, top_idx_to_top_id, traits::*};

// impl ast::PromptAst {
//     /// Find the AST node at the given position (byte offset).
//     pub fn find_at_position(&self, position: usize) -> SchemaPosition<'_> {
//         self.find_top_at_position(position)
//             .map(|top_id| match top_id {
//                 _ => SchemaPosition::TopLevel,
//             })
//             // If no top matched, we're in between top-level items. This is normal and expected.
//             .unwrap_or(SchemaPosition::TopLevel)
//     }

//     /// Do a binary search for the `Top` at the given byte offset.
//     pub fn find_top_at_position(&self, position: usize) -> Option<ast::TopId> {
//         use std::cmp::Ordering;

//         let top_idx = self.tops.binary_search_by(|top| {
//             let span = top.span();

//             if span.start > position {
//                 Ordering::Greater
//             } else if span.end < position {
//                 Ordering::Less
//             } else {
//                 Ordering::Equal
//             }
//         });

//         top_idx
//             .map(|idx| top_idx_to_top_id(idx, &self.tops[idx]))
//             .ok()
//     }
// }

// // /// A cursor position in a schema.
// // #[derive(Debug)]
// // pub enum SchemaPosition<'ast> {
// //     /// In-between top-level items
// //     TopLevel,
// // }
