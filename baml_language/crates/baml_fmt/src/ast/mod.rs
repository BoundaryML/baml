mod attributes;
mod declarations;
mod expressions;
mod pattern;
mod statements;
mod tokens;
mod types;

pub use baml_db::baml_compiler_syntax::{
    FromCST, KnownKind, StrongAstError, SyntaxNodeIter, validated::nodes::*,
};
use baml_db::baml_compiler_syntax::{ast as syntax_ast, validated::Validated};
use rowan::TextRange;
pub use tokens::*;

use crate::{
    printer::{PrintInfo, Printable, Printer, Shape},
    trivia_classifier::TriviaSliceExt as _,
};

impl Printable for Validated<'_, syntax_ast::SourceFile> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        assert_eq!(shape.indent, 0);
        assert_eq!(shape.first_line_offset, 0);
        assert_eq!(shape.width, printer.config.line_width);

        for (index, declaration) in self.top_level_declaration().enumerate() {
            if index > 0 {
                printer.print_newline();
            }

            let (leading_trivia, trailing_trivia) = printer.trivia.get_for_element(&declaration);
            printer.print_trivia_with_newline(leading_trivia.trim_leading_blanks(), 0);
            printer.print(&declaration, shape.clone());
            printer.print_trivia_trailing(trailing_trivia);
            printer.print_newline();
        }
        for trivia in printer.trivia.get_for_eof() {
            printer.print_trivia(trivia);
            printer.print_newline();
        }

        PrintInfo::default_multi_lined()
    }

    fn leftmost_token(&self) -> TextRange {
        self.top_level_declaration()
            .next()
            .map(|declaration| declaration.text_range())
            .unwrap_or_default()
    }

    fn rightmost_token(&self) -> TextRange {
        self.top_level_declaration()
            .last()
            .map(|declaration| declaration.text_range())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use baml_db::{
        baml_compiler_parser::parse_green,
        baml_compiler_syntax::{SyntaxNode, ast as syntax_ast, validated::ValidatedTree},
    };
    use baml_project::ProjectDatabase;

    #[test]
    fn parses_source_file() {
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
        let parsed = parse_green(&db, file);
        let syntax_tree = SyntaxNode::new_root(parsed);
        let tree = ValidatedTree::new(syntax_tree).unwrap();
        let source_file = tree.root::<syntax_ast::SourceFile>().unwrap();
        assert_eq!(source_file.top_level_declaration().count(), 2);
    }

    #[test]
    fn colon_without_type_is_error() {
        let source = r#"
            function BadParam(x:) -> int {
                1
            }
            "#;

        let mut db = ProjectDatabase::new();
        let file = db.add_file("test.baml", source);
        let parsed = parse_green(&db, file);
        let syntax_tree = SyntaxNode::new_root(parsed);

        assert!(ValidatedTree::new(syntax_tree).is_err());
    }
}
