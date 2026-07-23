pub use baml_db::baml_compiler_syntax::validated::*;
mod attributes;
mod declarations;
mod expressions;
mod pattern;
mod statements;
mod tokens;
mod types;
use crate::{
    printer::{PrintInfo, Printable, Printer, Shape},
    trivia_classifier::TriviaSliceExt as _,
};
pub use attributes::*;
pub use expressions::*;
use rowan::TextRange;
pub use tokens::*;
impl Printable for SourceFile {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        assert_eq!(shape.indent, 0);
        assert_eq!(shape.first_line_offset, 0);
        assert_eq!(shape.width, printer.config.line_width);
        for (idx, decl) in self.items.iter().enumerate() {
            if idx > 0 {
                printer.print_newline();
            }
            let (leading_trivia, trailing_trivia) = printer.trivia.get_for_element(decl);
            printer.print_trivia_with_newline(leading_trivia.trim_leading_blanks(), 0);
            printer.print(decl, shape.clone());
            printer.print_trivia_trailing(trailing_trivia);
            printer.print_newline();
        }
        for trivia in printer.trivia.get_for_eof() {
            printer.print_trivia(trivia);
            printer.print_newline();
        }
        PrintInfo::default_multi_lined()
    }
    /// May return [`TextRange::default()`] if there are no items.
    fn leftmost_token(&self) -> TextRange {
        self.items
            .first()
            .map(Printable::leftmost_token)
            .unwrap_or_default()
    }
    /// May return [`TextRange::default()`] if there are no items.
    fn rightmost_token(&self) -> TextRange {
        self.items
            .last()
            .map(Printable::rightmost_token)
            .unwrap_or_default()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use baml_db::{
        baml_compiler_parser::parse_green,
        baml_compiler_syntax::{SyntaxElement, SyntaxNode},
    };
    use baml_project::ProjectDatabase;
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
        let parsed = parse_green(&db, file);
        let syntax_tree = SyntaxNode::new_root(parsed);
        let source_file = SourceFile::from_cst(SyntaxElement::Node(syntax_tree)).unwrap();
        assert_eq!(source_file.items.len(), 2);
    }
    #[test]
    fn test_colon_without_type_is_error() {
        let source = r#"
            function BadParam(x:) -> int {
                1
            }
            "#;
        let mut db = ProjectDatabase::new();
        let file = db.add_file("test.baml", source);
        let parsed = parse_green(&db, file);
        let syntax_tree = SyntaxNode::new_root(parsed);
        let result = SourceFile::from_cst(SyntaxElement::Node(syntax_tree));
        assert!(
            result.is_err(),
            "Expected error for parameter with colon but no type"
        );
    }
}
