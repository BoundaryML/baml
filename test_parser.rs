// Quick test to verify Jinja parsing works

use baml_compiler_lexer::lex;
use baml_compiler_parser::parse_file;

fn main() {
    let source = r#"
function Test(input: string) -> string {
  client GPT4
  prompt #"
    Hello {{ input }}!
    {% if input %}
      You said something.
    {% endif %}
    {# This is a comment #}
    Plain text here.
  "#
}
"#;

    println!("Parsing test file...");
    let tokens = lex(source);
    let (green, errors) = parse_file(&tokens);

    println!("Parse errors: {}", errors.len());
    for error in &errors {
        println!("  - {:?}", error);
    }

    println!("\nSyntax tree:");
    println!("{:#?}", green);

    // Try to access the raw string and its children
    use rowan::NodeOrToken;
    use baml_compiler_syntax::{SyntaxNode, BamlLanguage, SyntaxKind};

    let root = SyntaxNode::new_root(green);

    fn print_tree(node: &SyntaxNode, indent: usize) {
        let indent_str = "  ".repeat(indent);
        println!("{}[{:?}]", indent_str, node.kind());

        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(n) => print_tree(&n, indent + 1),
                NodeOrToken::Token(t) => {
                    if matches!(t.kind(), SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE) {
                        // Skip whitespace for readability
                        continue;
                    }
                    println!("{}  {:?}: {:?}", indent_str, t.kind(), t.text());
                }
            }
        }
    }

    print_tree(&root, 0);
}
"#
