use std::path::Path;

use internal_baml_ast::{diagram_generator, parse};
use internal_baml_diagnostics::SourceFile;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(input_path) = args.next() else {
        eprintln!("Usage: generate_mermaid_headers <path/to/file.baml>");
        std::process::exit(2);
    };

    let path = std::path::PathBuf::from(&input_path);
    let contents = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to read {input_path}: {e}");
            std::process::exit(1);
        }
    };

    let source = SourceFile::new_allocated(path.clone(), contents.into());
    let (ast, diags) = match parse(Path::new("."), &source) {
        Ok(res) => res,
        Err(diags) => {
            eprintln!("Parse errors:\n{}", diags.to_pretty_string());
            std::process::exit(1);
        }
    };
    if diags.has_errors() {
        eprintln!("Parse errors:\n{}", diags.to_pretty_string());
        std::process::exit(1);
    }

    // Nicely styled header graph
    let mermaid = diagram_generator::generate_with_styling(
        diagram_generator::MermaidGeneratorContext {
            use_fancy: true,
            function_filter: None,
        },
        &ast,
    );
    println!("{mermaid}");
}
