use baml_base::{SourceFile, FileId};
use baml_lexer::lex_lossless;
use baml_parser::parse_file;
use baml_syntax::SyntaxNode;

fn main() {
    let source = r#"enum Status {
  ACTIVE
  INACTIVE
}

class User {
  name string
  age int
}
"#;
    
    let tokens = lex_lossless(source, FileId::new(0));
    let (green, errors) = parse_file(&tokens);
    let root = SyntaxNode::new_root(green);
    
    println!("Parse tree:");
    println!("{:#?}", root);
    println!("\nErrors: {:?}", errors);
}
