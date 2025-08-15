use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Context, Result};
use baml_types::{expr::ExprMetadata, BamlValueWithMeta};
use clap::Args;
use internal_baml_core::ast::Span;
use reedline::{DefaultPrompt, FileBackedHistory, Reedline, Signal};

use crate::BamlRuntime;
use baml_compiler::{hir::Hir, thir::typecheck::typecheck};
use internal_baml_core::internal_baml_diagnostics::Diagnostics;

#[derive(Args, Clone, Debug)]
pub struct ReplArgs {
    #[arg(
        long,
        help = "Initial BAML source directory to load",
        default_value = "./baml_src"
    )]
    pub from: PathBuf,
}

struct ReplState {
    runtime: Option<BamlRuntime>,
    variables: HashMap<String, BamlValueWithMeta<ExprMetadata>>,
    env_vars: HashMap<String, String>,
}

impl ReplState {
    fn new() -> Self {
        Self {
            runtime: None,
            variables: HashMap::new(),
            env_vars: std::env::vars().collect(),
        }
    }

    fn load_baml_sources(&mut self, path: PathBuf) -> Result<()> {
        let runtime = BamlRuntime::from_directory(&path, self.env_vars.clone()).context(
            format!("Failed to load BAML sources from {}", path.display()),
        )?;
        self.runtime = Some(runtime);
        println!("✓ Loaded BAML sources from {}", path.display());
        Ok(())
    }

    fn dump_thir(&self) -> Result<String> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| anyhow!("No BAML sources loaded. Use :load <path> to load sources."))?;

        #[cfg(feature = "internal")]
        {
            let internal = runtime.internal();

            // Convert AST to HIR
            let hir = Hir::from_ast(&internal.db.ast);

            // Typecheck HIR to get THIR
            let mut diagnostics = Diagnostics::default();
            let thir = typecheck(&hir, &mut diagnostics);

            // Format the THIR for display
            let mut output = String::new();
            output.push_str("=== TYPED HIGH-LEVEL INTERMEDIATE REPRESENTATION (THIR) ===\n\n");

            // Display global assignments
            if !thir.global_assignments.is_empty() {
                output.push_str("Global Assignments:\n");
                for (name, expr) in &thir.global_assignments {
                    output.push_str(&format!("  {} = {}\n", name, expr.dump_str()));
                }
                output.push_str("\n");
            }

            // Display expression functions
            if !thir.expr_functions.is_empty() {
                output.push_str("Expression Functions:\n");
                for func in &thir.expr_functions {
                    output.push_str(&format!("  fn {}(", func.name));
                    let params: Vec<String> = func
                        .parameters
                        .iter()
                        .map(|p| format!("{}: {:?}", p.name, p.r#type))
                        .collect();
                    output.push_str(&params.join(", "));
                    output.push_str(&format!(") -> {:?} {{\n", func.return_type));
                    output.push_str(&format!("    {}\n", func.body.dump_str()));
                    output.push_str("  }\n\n");
                }
            }

            // Display LLM functions
            if !thir.llm_functions.is_empty() {
                output.push_str("LLM Functions:\n");
                for func in &thir.llm_functions {
                    output.push_str(&format!("  function {}\n", func.name));
                }
                output.push_str("\n");
            }

            // Display classes
            if !thir.classes.is_empty() {
                output.push_str("Classes:\n");
                for (_name, class) in &thir.classes {
                    output.push_str(&format!("  class {}\n", class.name));
                }
                output.push_str("\n");
            }

            // Display enums
            if !thir.enums.is_empty() {
                output.push_str("Enums:\n");
                for (_name, enum_def) in &thir.enums {
                    output.push_str(&format!("  enum {}\n", enum_def.name));
                }
                output.push_str("\n");
            }

            // Show any type errors
            if diagnostics.has_errors() {
                output.push_str("Type Errors:\n");
                for error in diagnostics.errors() {
                    output.push_str(&format!("  {:?}\n", error));
                }
                output.push_str("\n");
            }

            Ok(output)
        }

        #[cfg(not(feature = "internal"))]
        {
            Err(anyhow!(
                "THIR dumping requires the 'internal' feature to be enabled"
            ))
        }
    }

    fn reset(&mut self) {
        self.variables.clear();
        println!("✓ Reset interpreter environment");
    }

    fn evaluate_expression(&mut self, input: &str) -> Result<String> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| anyhow!("No BAML sources loaded. Use :load <path> to load sources."))?;

        // For now, we'll implement a simple expression evaluator
        // This is a placeholder - we'd need to integrate with the BAML parser properly
        self.evaluate_simple_expression(input)
    }

    fn evaluate_simple_expression(&mut self, input: &str) -> Result<String> {
        // Check if this is a variable assignment
        if let Some((var_name, expr_str)) = input.split_once('=') {
            let var_name = var_name.trim().to_string();
            let expr_str = expr_str.trim();

            // For now, just handle simple literal values
            let value = self.parse_simple_value(expr_str)?;
            self.variables.insert(var_name.clone(), value);
            Ok(format!("✓ {} = {}", var_name, expr_str))
        } else if let Some(value) = self.variables.get(input.trim()) {
            // Return variable value
            Ok(format!("{}", self.format_value(value)))
        } else {
            // Try to parse as a simple value
            let value = self.parse_simple_value(input)?;
            Ok(self.format_value(&value))
        }
    }

    fn parse_simple_value(&self, input: &str) -> Result<BamlValueWithMeta<ExprMetadata>> {
        let input = input.trim();

        // Try to parse different types
        if input == "true" {
            Ok(BamlValueWithMeta::Bool(true, (Span::fake(), None)))
        } else if input == "false" {
            Ok(BamlValueWithMeta::Bool(false, (Span::fake(), None)))
        } else if input == "null" {
            Ok(BamlValueWithMeta::Null((Span::fake(), None)))
        } else if let Ok(int_val) = input.parse::<i64>() {
            Ok(BamlValueWithMeta::Int(int_val, (Span::fake(), None)))
        } else if let Ok(float_val) = input.parse::<f64>() {
            Ok(BamlValueWithMeta::Float(float_val, (Span::fake(), None)))
        } else if input.starts_with('"') && input.ends_with('"') {
            let str_val = input[1..input.len() - 1].to_string();
            Ok(BamlValueWithMeta::String(str_val, (Span::fake(), None)))
        } else if input.starts_with('[') && input.ends_with(']') {
            // Simple array parsing (comma separated)
            let inner = &input[1..input.len() - 1];
            if inner.trim().is_empty() {
                Ok(BamlValueWithMeta::List(Vec::new(), (Span::fake(), None)))
            } else {
                let items: Result<Vec<_>> = inner
                    .split(',')
                    .map(|item| self.parse_simple_value(item.trim()))
                    .collect();
                Ok(BamlValueWithMeta::List(items?, (Span::fake(), None)))
            }
        } else {
            // Treat as string literal if no quotes
            Ok(BamlValueWithMeta::String(
                input.to_string(),
                (Span::fake(), None),
            ))
        }
    }

    fn format_value(&self, value: &BamlValueWithMeta<ExprMetadata>) -> String {
        match value {
            BamlValueWithMeta::Null(_) => "null".to_string(),
            BamlValueWithMeta::Bool(b, _) => b.to_string(),
            BamlValueWithMeta::Int(i, _) => i.to_string(),
            BamlValueWithMeta::Float(f, _) => f.to_string(),
            BamlValueWithMeta::String(s, _) => format!("\"{}\"", s),
            BamlValueWithMeta::List(items, _) => {
                let formatted: Vec<String> =
                    items.iter().map(|item| self.format_value(item)).collect();
                format!("[{}]", formatted.join(", "))
            }
            BamlValueWithMeta::Map(map, _) => {
                let formatted: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, self.format_value(v)))
                    .collect();
                format!("{{{}}}", formatted.join(", "))
            }
            _ => format!("{:?}", value), // Fallback for other types
        }
    }

    fn list_variables(&self) -> String {
        if self.variables.is_empty() {
            "No variables defined".to_string()
        } else {
            let mut vars: Vec<String> = self
                .variables
                .iter()
                .map(|(name, value)| format!("{} = {}", name, self.format_value(value)))
                .collect();
            vars.sort();
            vars.join("\n")
        }
    }
}

impl ReplArgs {
    pub fn run(&self) -> Result<()> {
        let mut state = ReplState::new();

        // Try to load initial BAML sources if the directory exists
        if self.from.exists() {
            if let Err(e) = state.load_baml_sources(self.from.clone()) {
                eprintln!("Warning: Could not load initial BAML sources: {}", e);
                eprintln!("Use :load <path> to load BAML sources");
            }
        } else {
            println!(
                "No BAML sources found at {}. Use :load <path> to load sources.",
                self.from.display()
            );
        }

        // Set up readline with history
        let history = Box::new(
            FileBackedHistory::with_file(100, "baml_repl_history.txt".into())
                .map_err(|_| anyhow!("Failed to set up history"))?,
        );
        let mut line_editor = Reedline::create().with_history(history);
        let prompt = DefaultPrompt::default();

        println!("BAML REPL - Interactive BAML Expression Evaluator");
        println!("Type expressions to evaluate them, or use commands:");
        println!("  :load <path>   - Load BAML sources from directory");
        println!("  :reset         - Clear all variables");
        println!("  :vars          - List all variables");
        println!("  :thir          - Show THIR (Typed HIR) of loaded BAML sources");
        println!("  :help          - Show this help");
        println!("  :quit or Ctrl+C - Exit");
        println!("  x = expr       - Assign expression result to variable x");
        println!();

        loop {
            let sig = line_editor.read_line(&prompt);
            match sig {
                Ok(Signal::Success(buffer)) => {
                    let input = buffer.trim();

                    if input.is_empty() {
                        continue;
                    }

                    // Handle commands starting with ':'
                    if input.starts_with(':') {
                        match self.handle_command(&mut state, input) {
                            Ok(Some(msg)) => println!("{}", msg),
                            Ok(None) => break, // :quit
                            Err(e) => eprintln!("Error: {}", e),
                        }
                    } else {
                        // Handle expression evaluation
                        match state.evaluate_expression(input) {
                            Ok(result) => println!("{}", result),
                            Err(e) => eprintln!("Error: {}", e),
                        }
                    }
                }
                Ok(Signal::CtrlD) | Ok(Signal::CtrlC) => {
                    println!("Goodbye!");
                    break;
                }
                x => {
                    println!("Event: {:?}", x);
                }
            }
        }

        Ok(())
    }

    fn handle_command(&self, state: &mut ReplState, input: &str) -> Result<Option<String>> {
        let parts: Vec<&str> = input[1..].split_whitespace().collect();
        if parts.is_empty() {
            return Err(anyhow!("Empty command"));
        }

        match parts[0] {
            "load" => {
                if parts.len() != 2 {
                    return Err(anyhow!("Usage: :load <path>"));
                }
                let path = PathBuf::from(parts[1]);
                state.load_baml_sources(path)?;
                Ok(Some("".to_string())) // Success message already printed
            }
            "reset" => {
                state.reset();
                Ok(Some("".to_string())) // Success message already printed
            }
            "vars" => Ok(Some(state.list_variables())),
            "thir" => match state.dump_thir() {
                Ok(output) => Ok(Some(output)),
                Err(e) => Err(e),
            },
            "help" => Ok(Some(
                r#"BAML REPL Commands:
  :load <path>   - Load BAML sources from directory
  :reset         - Clear all variables
  :vars          - List all variables
  :thir          - Show THIR (Typed HIR) of loaded BAML sources
  :help          - Show this help
  :quit          - Exit the REPL
  
Expression syntax:
  x = expr       - Assign expression result to variable x
  variable_name  - Show value of variable
  
Supported literals:
  Numbers: 42, 3.14
  Strings: "hello world"
  Booleans: true, false
  Null: null
  Arrays: [1, 2, 3]"#
                    .to_string(),
            )),
            "quit" | "exit" => {
                println!("Goodbye!");
                Ok(None)
            }
            _ => Err(anyhow!(
                "Unknown command: {}. Type :help for available commands.",
                parts[0]
            )),
        }
    }
}
