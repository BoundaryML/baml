use std::{env, fs, path::Path};

use internal_baml_ast::{parse, MermaidDiagramGenerator};
use internal_baml_diagnostics::SourceFile;

fn main() {
    // Get command line arguments
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 || args.len() > 3 {
        eprintln!("Usage: {} [--fancy] <path-to-baml-file>", args[0]);
        eprintln!("Example: {} example.baml", args[0]);
        eprintln!("Example: {} --fancy example.baml", args[0]);
        std::process::exit(1);
    }

    let (baml_file_path, use_fancy) = if args.len() == 3 && args[1] == "--fancy" {
        (&args[2], true)
    } else {
        (&args[1], false)
    };

    // Check if file exists and has .baml extension
    let path = Path::new(baml_file_path);
    if !path.exists() {
        eprintln!("Error: File '{baml_file_path}' does not exist");
        std::process::exit(1);
    }

    if path.extension().and_then(|s| s.to_str()) != Some("baml") {
        eprintln!("Error: File '{baml_file_path}' does not have a .baml extension");
        std::process::exit(1);
    }

    // Read the BAML file
    let baml_source = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => {
            eprintln!("Error reading file '{baml_file_path}': {err}");
            std::process::exit(1);
        }
    };

    // Create a SourceFile
    let source = SourceFile::from((path.to_path_buf(), baml_source));
    let root_path = Path::new(".");

    // Parse the BAML source code
    match parse(root_path, &source) {
        Ok((ast, _diagnostics)) => {
            // Print the AST
            dbg!(&ast);

            // Generate Mermaid diagram with optional styling
            let mermaid_diagram =
                MermaidDiagramGenerator::generate_ast_diagram_with_styling(&ast, use_fancy);

            println!(
                "Generated Mermaid Diagram for '{}' (styling: {}):",
                baml_file_path,
                if use_fancy { "enabled" } else { "disabled" }
            );
            println!("{mermaid_diagram}");

            // You can copy the output and paste it into any Mermaid renderer
            println!("\nTo visualize this diagram:");
            println!("1. Copy the output above");
            println!("2. Go to https://mermaid.live/");
            println!("3. Paste the diagram code");
            println!("4. View the rendered AST diagram!");
        }
        Err(err) => {
            eprintln!("Failed to parse BAML source: {err:?}");
            std::process::exit(1);
        }
    }
}
