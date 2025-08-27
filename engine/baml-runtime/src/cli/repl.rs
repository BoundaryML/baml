use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Context, Result};
use baml_compiler::{
    hir::{self, Hir},
    thir::{
        interpret::interpret_thir,
        typecheck::{typecheck_expression, typecheck_returning_context, VarInfo},
    },
};
use baml_types::{
    expr::ExprMetadata, ir_type::TypeGeneric, BamlValue, BamlValueWithMeta, Completion, TypeIR,
};
use clap::Args;
use indexmap::IndexMap;
use internal_baml_ast::{ast::WithName, parse, parse_standalone_expression};
use internal_baml_core::{
    ast as baml_ast,
    ast::Span,
    internal_baml_diagnostics::{Diagnostics, SourceFile},
};
use jsonish::{ResponseBamlValue, ResponseValueMeta};
use pretty::RcDoc;
use reedline::{
    default_emacs_keybindings, ColumnarMenu, DefaultCompleter, DefaultPrompt, Emacs,
    FileBackedHistory, KeyCode, KeyModifiers, MenuBuilder, Reedline, ReedlineEvent, ReedlineMenu,
    Signal,
};

use crate::{BamlRuntime, TripWire};

#[derive(Args, Clone, Debug)]
pub struct ReplArgs {
    #[arg(
        long,
        help = "Initial BAML source directory to load",
        default_value = "./baml_src"
    )]
    pub from: PathBuf,

    #[arg(
        short = 'e',
        long = "eval",
        help = "Evaluate a single expression and exit"
    )]
    pub expression: Option<String>,
}

struct ReplState {
    runtime: Option<BamlRuntime>,
    variables: IndexMap<String, BamlValueWithMeta<ExprMetadata>>,
    env_vars: HashMap<String, String>,
}

impl ReplState {
    fn new() -> Self {
        Self {
            runtime: None,
            variables: IndexMap::new(),
            env_vars: std::env::vars().collect(),
        }
    }

    fn load_baml_sources(&mut self, path: PathBuf) -> Result<()> {
        let runtime =
            BamlRuntime::from_directory(&path, self.env_vars.clone(), Default::default()).context(
                format!("Failed to load BAML sources from {}", path.display()),
            )?;
        self.runtime = Some(runtime);
        println!("✓ Loaded BAML sources from {}", path.display());
        Ok(())
    }

    fn function_parameters(&self) -> Result<HashMap<String, Vec<String>>> {
        let hir = match self.runtime.as_ref() {
            Some(runtime) => {
                let internal = &runtime.inner;
                Hir::from_ast(&internal.db.ast)
            }
            None => Hir::empty(),
        };
        let mut diagnostics = Diagnostics::default();
        diagnostics.set_source(&(PathBuf::from("repl"), "function_parameters").into());
        let (thir, _) = typecheck_returning_context(&hir, &mut diagnostics);
        Ok(thir
            .llm_functions
            .iter()
            .map(|f| {
                (
                    f.name.clone(),
                    f.parameters.iter().map(|p| p.name.clone()).collect(),
                )
            })
            .collect())
    }

    fn dump_thir(&self) -> Result<String> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| anyhow!("No BAML sources loaded. Use :load <path> to load sources."))?;

        let internal = &runtime.inner;

        // Convert AST to HIR
        let hir = Hir::from_ast(&internal.db.ast);

        // Typecheck HIR to get THIR
        let mut diagnostics = Diagnostics::default();
        let (thir, _) = typecheck_returning_context(&hir, &mut diagnostics);

        // Format the THIR for display
        let mut output = String::new();
        output.push_str("=== TYPED HIGH-LEVEL INTERMEDIATE REPRESENTATION (THIR) ===\n\n");

        // Display global assignments
        if !thir.global_assignments.is_empty() {
            output.push_str("Global Assignments:\n");
            for (name, expr) in &thir.global_assignments {
                output.push_str(&format!("  {} = {}\n", name, expr.dump_str()));
            }
            output.push('\n');
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
            output.push('\n');
        }

        // Display classes
        if !thir.classes.is_empty() {
            output.push_str("Classes:\n");
            for (_name, class) in &thir.classes {
                output.push_str(&format!("  class {}\n", class.name));
            }
            output.push('\n');
        }

        // Display enums
        if !thir.enums.is_empty() {
            output.push_str("Enums:\n");
            for (_name, enum_def) in &thir.enums {
                output.push_str(&format!("  enum {}\n", enum_def.name));
            }
            output.push('\n');
        }

        // Show any type errors
        if diagnostics.has_errors() {
            output.push_str("Type Errors:\n");
            for error in diagnostics.errors() {
                output.push_str(&format!("  {error:?}\n"));
            }
            output.push('\n');
        }

        Ok(output)
    }

    fn reset(&mut self) {
        self.variables.clear();
        println!("✓ Reset interpreter environment");
    }

    async fn evaluate_expression(&mut self, input: &str) -> Result<String> {
        // let runtime = self
        //     .runtime
        //     .as_ref()
        //     .ok_or_else(|| anyhow!("No BAML sources loaded. Use :load <path> to load sources."))?;

        // For now, we'll implement a simple expression evaluator
        // This is a placeholder - we'd need to integrate with the BAML parser properly
        self.evaluate_simple_expression(input).await
    }

    async fn evaluate_simple_expression(&mut self, input: &str) -> Result<String> {
        // Check if this is a variable assignment
        if let Some((var_name, expr_str)) = input.split_once('=') {
            let var_name = var_name.trim().to_string();
            let expr_str = expr_str.trim();

            // Parse and evaluate the BAML expression
            let value = self.parse_and_evaluate_baml_expression(expr_str).await?;
            self.variables.insert(var_name.clone(), value.clone());
            Ok(format!("✓ {} = {}", var_name, self.format_value(&value)))
        } else if let Some(value) = self.variables.get(input.trim()) {
            // Return variable value
            Ok(self.format_value(value).to_string())
        } else {
            // Try to parse as a BAML expression
            let value = self.parse_and_evaluate_baml_expression(input).await?;
            Ok(self.format_value(&value))
        }
    }

    async fn parse_and_evaluate_baml_expression(
        &self,
        input: &str,
    ) -> Result<BamlValueWithMeta<ExprMetadata>> {
        let hir = match self.runtime.as_ref() {
            Some(runtime) => {
                // Get the internal runtime to access the existing context
                let internal = &runtime.inner;

                // Convert AST to HIR from existing loaded sources
                Hir::from_ast(&internal.db.ast)
            }
            None => Hir::empty(),
        };

        // Typecheck to get THIR
        let mut type_diagnostics = Diagnostics::default();
        type_diagnostics.set_source(&(PathBuf::from("repl"), input).into());
        let (thir, type_context) = typecheck_returning_context(&hir, &mut type_diagnostics);

        if type_diagnostics.has_errors() {
            eprintln!("Warning: Type errors in loaded BAML sources");
        }

        let input_expr_ast = parse_standalone_expression(input, &mut type_diagnostics)?;
        let input_expr_hir = hir::Expression::from_ast(&input_expr_ast);

        let input_expr_thir =
            typecheck_expression(&input_expr_hir, &type_context, &mut type_diagnostics);

        // let variables: IndexMap<String, BamlValueWithMeta<TypeGeneric<TypeIR>>> = self

        let variables: IndexMap<String, BamlValueWithMeta<ExprMetadata>> = self
            .variables
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let fn_params = self.function_parameters()?.clone();

        let runtime_clone = self.runtime.clone();
        let env_vars = self.env_vars.clone();
        let handle_llm_function = move |function_name: String, args: Vec<BamlValue>| {
            let fn_params = fn_params.clone();
            let runtime_clone = runtime_clone.clone();
            let env_vars = env_vars.clone();
            async move {
                match runtime_clone.as_ref() {
                    Some(runtime) => {
                        let param_names = fn_params
                            .get(&function_name)
                            .ok_or_else(|| anyhow!("LLM Function {} not found", function_name))?;
                        let args = args
                            .clone()
                            .into_iter()
                            .zip(param_names.iter())
                            .map(|(arg, name)| (name.clone(), arg.clone()))
                            .collect();
                        let cxt =
                            runtime.create_ctx_manager(BamlValue::String("none".to_string()), None);
                        let res = runtime
                            .call_function(
                                function_name,
                                &args,
                                &cxt,
                                None,
                                None,
                                None,
                                env_vars,
                                TripWire::new(None),
                            )
                            .await;
                        let function_result = res.0?;
                        match function_result.parsed() {
                            Some(Ok(response_baml_value)) => Ok(response_baml_value
                                .clone()
                                .0
                                .map_meta_owned(|_| (Span::fake(), None))),
                            Some(Err(e)) => Err(anyhow!("Failed to parse function result: {}", e)),
                            None => Err(anyhow!("No parsed result available from function call")),
                        }
                    }
                    None => Err(anyhow!(
                        "No runtime loaded, it should be impossible to call an LLM function"
                    )),
                }
            }
        };
        let eval_result = interpret_thir(
            thir.clone(),
            input_expr_thir,
            handle_llm_function,
            variables,
        )
        .await?;

        Ok(eval_result)
    }

    fn infer_expression_type(&self, input: &str) -> Result<String> {
        let input = input.trim();

        let hir = match self.runtime.as_ref() {
            Some(runtime) => {
                // Get the internal runtime to access the existing context
                let internal = &runtime.inner;

                // Convert AST to HIR from existing loaded sources
                Hir::from_ast(&internal.db.ast)
            }
            None => Hir::empty(),
        };

        // Typecheck to get THIR and type context
        let mut type_diagnostics = Diagnostics::default();
        type_diagnostics.set_source(&(PathBuf::from("repl"), input).into());
        let (_thir, mut type_context) = typecheck_returning_context(&hir, &mut type_diagnostics);

        for (k, v) in &self.variables {
            if let Some(ty) = v.meta().1.as_ref() {
                type_context.vars.insert(
                    k.clone(),
                    VarInfo {
                        ty: ty.clone(),
                        mut_var_info: None,
                    },
                );
            }
        }

        if type_diagnostics.has_errors() {
            eprintln!("Warning: Type errors in loaded BAML sources");
        }

        // Parse the expression
        let input_expr_ast = parse_standalone_expression(input, &mut type_diagnostics)?;
        let input_expr_hir = hir::Expression::from_ast(&input_expr_ast);

        // Typecheck the expression to infer its type
        let input_expr_thir =
            typecheck_expression(&input_expr_hir, &type_context, &mut type_diagnostics);

        // Check for type errors in the expression
        if type_diagnostics.has_errors() {
            let error_messages: Vec<String> = type_diagnostics
                .errors()
                .iter()
                .map(|e| e.message().to_string())
                .collect();
            return Err(anyhow!("Type error: {}", error_messages.join("; ")));
        }

        // Extract the inferred type
        if let Some(inferred_type) = input_expr_thir.meta().1.as_ref() {
            Ok(ReplState::format_type(inferred_type))
        } else {
            Ok("unknown".to_string())
        }
    }

    fn get_completion_candidates(&self) -> Vec<String> {
        let mut candidates = Vec::new();

        // Add all variable names
        for var_name in self.variables.keys() {
            candidates.push(var_name.clone());
        }

        // Add REPL commands
        candidates.extend([
            ":load".to_string(),
            ":l".to_string(),
            ":reset".to_string(),
            ":r".to_string(),
            ":vars".to_string(),
            ":v".to_string(),
            ":thir".to_string(),
            ":type".to_string(),
            ":t".to_string(),
            ":help".to_string(),
            ":h".to_string(),
            ":?".to_string(),
            ":quit".to_string(),
            ":q".to_string(),
        ]);

        // Add functions and declarations from THIR if available
        if let Some(runtime) = &self.runtime {
            let internal = &runtime.inner;

            // Add function names
            for function in internal.db.walk_functions() {
                candidates.push(function.name().to_string());
            }

            // Add class names
            for class in internal.db.walk_classes() {
                candidates.push(class.name().to_string());
            }

            // Add enum names
            for enum_walker in internal.db.walk_enums() {
                candidates.push(enum_walker.name().to_string());
            }

            // Add client names
            for client in internal.db.walk_clients() {
                candidates.push(client.name().to_string());
            }
        }

        candidates.sort();
        candidates.dedup();
        candidates
    }

    fn format_type(ty: &TypeIR) -> String {
        match ty {
            TypeIR::Primitive(type_val, _) => match type_val {
                baml_types::ir_type::TypeValue::String => "string".to_string(),
                baml_types::ir_type::TypeValue::Int => "int".to_string(),
                baml_types::ir_type::TypeValue::Float => "float".to_string(),
                baml_types::ir_type::TypeValue::Bool => "bool".to_string(),
                baml_types::ir_type::TypeValue::Null => "null".to_string(),
                baml_types::ir_type::TypeValue::Media(media_type) => {
                    format!("media({media_type:?})")
                }
            },
            TypeIR::Enum { name, .. } => name.clone(),
            TypeIR::Class { name, .. } => name.clone(),
            TypeIR::List(inner, _) => format!("{}[]", ReplState::format_type(inner)),
            TypeIR::Map(key, value, _) => {
                format!(
                    "map<{}, {}>",
                    ReplState::format_type(key),
                    ReplState::format_type(value)
                )
            }
            TypeIR::Union(union_type, _) => {
                use baml_types::ir_type::UnionTypeViewGeneric;
                match union_type.view() {
                    UnionTypeViewGeneric::Null => "null".to_string(),
                    UnionTypeViewGeneric::Optional(inner) => {
                        format!("{}?", ReplState::format_type(inner))
                    }
                    UnionTypeViewGeneric::OneOf(types) => {
                        let type_names: Vec<String> =
                            types.iter().map(|t| ReplState::format_type(t)).collect();
                        format!("({})", type_names.join(" | "))
                    }
                    UnionTypeViewGeneric::OneOfOptional(types) => {
                        let type_names: Vec<String> =
                            types.iter().map(|t| ReplState::format_type(t)).collect();
                        format!("({})?", type_names.join(" | "))
                    }
                }
            }
            TypeIR::RecursiveTypeAlias { name, .. } => name.clone(),
            TypeIR::Literal(literal, _) => format!("literal({literal:?})"),
            TypeIR::Tuple(types, _) => {
                let type_names: Vec<String> = types.iter().map(ReplState::format_type).collect();
                format!("({})", type_names.join(", "))
            }
            TypeIR::Arrow(arrow, _) => {
                let input_types: Vec<String> = arrow
                    .param_types
                    .iter()
                    .map(ReplState::format_type)
                    .collect();
                format!(
                    "({}) -> {}",
                    input_types.join(", "),
                    ReplState::format_type(&arrow.return_type)
                )
            }
            TypeIR::Top(_) => panic!(
                "TypeIR::Top should have been resolved by the compiler before code generation. \
                 This indicates a bug in the type resolution phase."
            ),
        }
    }

    fn format_value(&self, value: &BamlValueWithMeta<ExprMetadata>) -> String {
        let doc = value.to_doc();
        let mut output = Vec::new();
        doc.render(80, &mut output).unwrap();
        String::from_utf8(output).unwrap()
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
    pub async fn run(&self) -> Result<()> {
        let mut state = ReplState::new();

        // Try to load initial BAML sources if the directory exists
        if self.from.exists() {
            if let Err(e) = state.load_baml_sources(self.from.clone()) {
                eprintln!("Warning: Could not load initial BAML sources: {e}");
                eprintln!("Use :load <path> to load BAML sources");
            }
        } else {
            println!(
                "No BAML sources found at {}. Use :load <path> to load sources.",
                self.from.display()
            );
        }

        // If -e argument is provided, evaluate the expression and exit
        if let Some(expression) = &self.expression {
            match state.evaluate_expression(expression).await {
                Ok(result) => {
                    println!("{result}");
                    return Ok(());
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
        }

        // Set up readline with history and completion
        let history = Box::new(
            FileBackedHistory::with_file(100, "baml_repl_history.txt".into())
                .map_err(|_| anyhow!("Failed to set up history"))?,
        );

        // Create completer with initial candidates
        let completion_candidates = state.get_completion_candidates();
        let completer = Box::new(DefaultCompleter::new_with_wordlen(completion_candidates, 1));

        // Set up completion menu
        let completion_menu = Box::new(ColumnarMenu::default().with_name("completion_menu"));

        // Set up keybindings for tab completion
        let mut keybindings = default_emacs_keybindings();
        keybindings.add_binding(
            KeyModifiers::NONE,
            KeyCode::Tab,
            ReedlineEvent::UntilFound(vec![
                ReedlineEvent::Menu("completion_menu".to_string()),
                ReedlineEvent::MenuNext,
            ]),
        );

        let edit_mode = Box::new(Emacs::new(keybindings));

        let mut line_editor = Reedline::create()
            .with_history(history)
            .with_completer(completer)
            .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
            .with_edit_mode(edit_mode);
        let prompt = DefaultPrompt::default();

        println!("BAML REPL - Interactive BAML Expression Evaluator");
        println!("Type expressions to evaluate them, or use commands:");
        println!("  :load <path> (:l)  - Load BAML sources from directory");
        println!("  :reset (:r)        - Clear all variables");
        println!("  :vars (:v)         - List all variables");
        println!("  :thir              - Show THIR (Typed HIR) of loaded BAML sources");
        println!("  :type <expr> (:t)  - Show the inferred type of an expression");
        println!("  :help (:h, :?)     - Show this help");
        println!("  :quit (:q)         - Exit");
        println!("  x = expr           - Assign expression result to variable x");
        println!();
        println!("💡 Use Tab for autocompletion of commands, variables, and functions");
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
                            Ok(Some(msg)) => println!("{msg}"),
                            Ok(None) => break, // :quit
                            Err(e) => eprintln!("Error: {e}"),
                        }
                    } else {
                        // Handle expression evaluation
                        match state.evaluate_expression(input).await {
                            Ok(result) => println!("{result}"),
                            Err(e) => eprintln!("Error: {e}"),
                        }
                    }
                }
                Ok(Signal::CtrlD) | Ok(Signal::CtrlC) => {
                    println!("Goodbye!");
                    break;
                }
                x => {
                    println!("Event: {x:?}");
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
            "load" | "l" => {
                if parts.len() != 2 {
                    return Err(anyhow!("Usage: :load <path> (or :l <path>)"));
                }
                let path = PathBuf::from(parts[1]);
                state.load_baml_sources(path)?;
                Ok(Some("".to_string())) // Success message already printed
            }
            "reset" | "r" => {
                state.reset();
                Ok(Some("".to_string())) // Success message already printed
            }
            "vars" | "v" => Ok(Some(state.list_variables())),
            "thir" => match state.dump_thir() {
                Ok(output) => Ok(Some(output)),
                Err(e) => Err(e),
            },
            "type" | "t" => {
                if parts.len() < 2 {
                    return Err(anyhow!("Usage: :type <expression> (or :t <expression>)"));
                }
                let command_prefix = if parts[0] == "type" { ":type" } else { ":t" };
                let expr_str = input[command_prefix.len()..].trim();
                match state.infer_expression_type(expr_str) {
                    Ok(type_info) => Ok(Some(type_info)),
                    Err(e) => Err(e),
                }
            }
            "help" | "h" | "?" => Ok(Some(
                r#"BAML REPL Commands:
  :load <path> (:l)  - Load BAML sources from directory
  :reset (:r)        - Clear all variables
  :vars (:v)         - List all variables
  :thir              - Show THIR (Typed HIR) of loaded BAML sources
  :type <expr> (:t)  - Show the inferred type of an expression
  :help (:h, :?)     - Show this help
  :quit (:q)         - Exit the REPL
  
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
            "quit" | "exit" | "q" => {
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
