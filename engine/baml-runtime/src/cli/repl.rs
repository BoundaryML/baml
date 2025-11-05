use std::{
    collections::HashMap,
    fs::{create_dir_all, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context, Result};
use baml_compiler::{
    hir::{self, Hir},
    thir::{
        interpret::interpret_thir,
        typecheck::{typecheck_expression, typecheck_returning_context, VarInfo},
    },
    watch::{shared_handler, SharedWatchHandler},
};
use baml_types::{
    expr::ExprMetadata, ir_type::TypeGeneric, BamlValue, BamlValueWithMeta, Completion, TypeIR,
};
use clap::Args;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode, KeyEvent,
        KeyModifiers, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use dirs;
use indexmap::IndexMap;
use internal_baml_ast::{ast::WithName, parse, parse_standalone_expression};
use internal_baml_core::{
    ast as baml_ast,
    ast::Span,
    internal_baml_diagnostics::{Diagnostics, SourceFile},
};
use jsonish::{ResponseBamlValue, ResponseValueMeta};
use log::LevelFilter;
use pretty::RcDoc;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span as TuiSpan},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
    Terminal,
};
use supports_color::{self, Stream};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{BamlRuntime, TripWire};

// Events for UI status during LLM calls
struct TokenUsage {
    prompt_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
}

enum LlmStatusEvent {
    Started(u64, String),
    Finished(u64, Option<TokenUsage>),
}

#[derive(Default, Clone, Copy)]
struct TokenUsageAcc {
    prompt: u64,
    output: u64,
    total: u64,
    cached_input: u64,
}

impl TokenUsageAcc {
    fn add(&mut self, u: &TokenUsage) {
        self.prompt = self.prompt.saturating_add(u.prompt_tokens.unwrap_or(0));
        self.output = self.output.saturating_add(u.output_tokens.unwrap_or(0));
        self.total = self.total.saturating_add(u.total_tokens.unwrap_or(0));
        self.cached_input = self
            .cached_input
            .saturating_add(u.cached_input_tokens.unwrap_or(0));
    }
}

fn history_path() -> PathBuf {
    if let Some(mut dir) = dirs::data_dir() {
        dir.push("baml");
        let _ = create_dir_all(&dir);
        return dir.join("repl_history.txt");
    }
    if let Some(mut home) = dirs::home_dir() {
        home.push(".baml");
        let _ = create_dir_all(&home);
        return home.join("repl_history.txt");
    }
    PathBuf::from(".repl_history.txt")
}
const HISTORY_LIMIT: usize = 100;

fn load_history_from_file(path: &Path, limit: usize) -> Vec<String> {
    let mut buf = String::new();
    if let Ok(mut f) = std::fs::File::open(path) {
        let _ = f.read_to_string(&mut buf);
    }
    let mut items: Vec<String> = buf
        .lines()
        .map(|s| s.trim_end().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if items.len() > limit {
        let start = items.len() - limit;
        items = items.split_off(start);
    }
    items
}

fn append_history_line(path: &Path, line: &str) {
    if line.trim().is_empty() {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = create_dir_all(parent);
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

// Shimmer utilities
static PROCESS_START: OnceLock<Instant> = OnceLock::new();

fn elapsed_since_start() -> Duration {
    let start = PROCESS_START.get_or_init(Instant::now);
    start.elapsed()
}

fn shimmer_spans(text: &str) -> Vec<TuiSpan<'static>> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }

    let padding = 10usize;
    let period = chars.len() + padding * 2;
    let sweep_seconds = 2.0f32;
    let pos_f =
        (elapsed_since_start().as_secs_f32() % sweep_seconds) / sweep_seconds * period as f32;
    let pos = pos_f as usize;

    let has_true_color = supports_color::on_cached(Stream::Stdout)
        .map(|level| level.has_16m)
        .unwrap_or(false);
    let band_half_width = 3.0f32;

    let mut spans = Vec::with_capacity(chars.len());
    for (i, ch) in chars.iter().enumerate() {
        let i_pos = i as isize + padding as isize;
        let pos = pos as isize;
        let dist = (i_pos - pos).abs() as f32;

        let t = if dist <= band_half_width {
            let x = std::f32::consts::PI * (dist / band_half_width);
            0.5 * (1.0 + x.cos())
        } else {
            0.0
        };
        let brightness = 0.4 + 0.6 * t;
        let level = (brightness * 255.0).clamp(0.0, 255.0) as u8;
        let style = if has_true_color {
            Style::default()
                .fg(Color::Rgb(level, level, level))
                .add_modifier(Modifier::BOLD)
        } else {
            color_for_level(level)
        };
        spans.push(TuiSpan::styled(ch.to_string(), style));
    }
    spans
}

fn color_for_level(level: u8) -> Style {
    if level < 160 {
        Style::default().add_modifier(Modifier::DIM)
    } else if level < 224 {
        Style::default()
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    }
}

// Ghostty Graphical Progress Bar (OSC 9;4)
// See Ghostty 1.2.0 release notes: supports ConEmu/CorEMU OSC 9;4 sequences
// Command format: ESC ] 9 ; 4 ; <percent> BEL
// We treat 0 as hidden and 1..99 as active. 100 is treated as complete then hidden.
fn terminal_supports_graphical_progress() -> bool {
    // Allow override via env var for testing or disabling.
    if let Ok(v) = std::env::var("BAML_REPL_PROGRESS") {
        if v == "0" || v.eq_ignore_ascii_case("false") {
            return false;
        }
        if v == "1" || v.eq_ignore_ascii_case("true") {
            return true;
        }
    }
    if let Ok(tp) = std::env::var("TERM_PROGRAM") {
        if tp.to_ascii_lowercase().contains("ghostty") {
            return true;
        }
    }
    // Default off to avoid spamming BEL on unknown terminals.
    false
}

fn send_graphical_progress(percent: i32) {
    // Clamp to 0..100 range; 0 hides in Ghostty's implementation.
    let p = percent.clamp(0, 100);
    // OSC 9;4;P BEL (ConEmu/CorEMU percentage mode)
    let seq = format!("\x1b]9;4;{p}\x07");
    let _ = std::io::stdout().write_all(seq.as_bytes());
    let _ = std::io::stdout().flush();
}

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
    current_run_id: Option<u64>,
}

impl ReplState {
    fn new() -> Self {
        Self {
            runtime: None,
            variables: IndexMap::new(),
            env_vars: std::env::vars().collect(),
            current_run_id: None,
        }
    }

    #[allow(clippy::print_stdout)]
    fn load_baml_sources(&mut self, path: PathBuf) -> Result<()> {
        let runtime =
            BamlRuntime::from_directory(&path, self.env_vars.clone(), Default::default()).context(
                format!("Failed to load BAML sources from {}", path.display()),
            )?;
        self.runtime = Some(runtime);
        println!("✓ Loaded BAML sources from {}", path.display());
        Ok(())
    }

    fn function_names(&self) -> Vec<String> {
        // Prefer llm_functions from THIR, falling back to walker
        if let Ok(params) = self.function_parameters() {
            return params.keys().cloned().collect();
        }
        let mut names = Vec::new();
        if let Some(runtime) = &self.runtime {
            let internal = &runtime;
            for function in internal.db.walk_functions() {
                names.push(function.name().to_string());
            }
        }
        names.sort();
        names.dedup();
        names
    }

    fn function_parameters(&self) -> Result<HashMap<String, Vec<String>>> {
        let hir = match self.runtime.as_ref() {
            Some(runtime) => {
                let internal = &runtime;
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

        let internal = &runtime;

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
            for (name, ga) in &thir.global_assignments {
                let ann = ga
                    .annotated_type
                    .as_ref()
                    .map(|t| format!(": {}", t.diagnostic_repr()))
                    .unwrap_or_default();
                output.push_str(&format!("  {}{} = {}\n", name, ann, ga.expr.dump_str()));
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

    #[allow(clippy::print_stdout)]
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

    async fn evaluate_expression_with_status(
        &mut self,
        input: &str,
        status_tx: Option<std::sync::mpsc::Sender<LlmStatusEvent>>,
    ) -> Result<String> {
        self.evaluate_simple_expression_with_status(input, status_tx)
            .await
    }

    async fn evaluate_simple_expression(&mut self, input: &str) -> Result<String> {
        self.evaluate_simple_expression_with_status(input, None)
            .await
    }

    async fn evaluate_simple_expression_with_status(
        &mut self,
        input: &str,
        status_tx: Option<std::sync::mpsc::Sender<LlmStatusEvent>>,
    ) -> Result<String> {
        // Check if this is a variable assignment
        if let Some((var_name, expr_str)) = input.split_once('=') {
            let var_name = var_name.trim().to_string();
            let expr_str = expr_str.trim();

            // Parse and evaluate the BAML expression
            let (value, watch_notifications) = self
                .parse_and_evaluate_baml_expression_with_status(expr_str, status_tx)
                .await?;
            self.variables.insert(var_name.clone(), value.clone());

            let mut output = String::new();
            if !watch_notifications.is_empty() {
                for event in &watch_notifications {
                    output.push_str(event);
                    output.push('\n');
                }
            }
            output.push_str(&format!("✓ {} = {}", var_name, self.format_value(&value)));
            Ok(output)
        } else if let Some(value) = self.variables.get(input.trim()) {
            // Return variable value
            Ok(self.format_value(value).to_string())
        } else {
            // Try to parse as a BAML expression
            let (value, watch_notifications) = self
                .parse_and_evaluate_baml_expression_with_status(input, status_tx)
                .await?;

            let mut output = String::new();
            if !watch_notifications.is_empty() {
                for event in &watch_notifications {
                    output.push_str(event);
                    output.push('\n');
                }
            }
            output.push_str(&self.format_value(&value));
            Ok(output)
        }
    }

    async fn parse_and_evaluate_baml_expression_with_status(
        &self,
        input: &str,
        status_tx: Option<std::sync::mpsc::Sender<LlmStatusEvent>>,
    ) -> Result<(BamlValueWithMeta<ExprMetadata>, Vec<String>)> {
        let hir = match self.runtime.as_ref() {
            Some(runtime) => {
                // Get the internal runtime to access the existing context
                let internal = &runtime;

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

        // Check for type errors in the user's expression
        if type_diagnostics.has_errors() {
            let error_messages: Vec<String> = type_diagnostics
                .errors()
                .iter()
                .map(|e| e.message().to_string())
                .collect();
            return Err(anyhow!("Type error: {}", error_messages.join("; ")));
        }

        // let variables: IndexMap<String, BamlValueWithMeta<TypeGeneric<TypeIR>>> = self

        let variables: IndexMap<String, BamlValueWithMeta<ExprMetadata>> = self
            .variables
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let fn_params = self.function_parameters()?.clone();

        let runtime_clone = self.runtime.clone();
        let env_vars = self.env_vars.clone();
        let run_id = self.current_run_id.unwrap_or(0);
        let handle_llm_function = move |function_name: String,
                                        args: Vec<BamlValue>,
                                        _watch_context: Option<
            baml_compiler::thir::interpret::WatchStreamContext,
        >| {
            let fn_params = fn_params.clone();
            let runtime_clone = runtime_clone.clone();
            let env_vars = env_vars.clone();
            let status_tx = status_tx.clone();
            async move {
                match runtime_clone.as_ref() {
                    Some(runtime) => {
                        if let Some(tx) = &status_tx {
                            let _ = tx.send(LlmStatusEvent::Started(run_id, function_name.clone()));
                        }
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
                        let status_tx2 = status_tx.clone();
                        let res = runtime
                            .call_function(
                                function_name,
                                &args,
                                &cxt,
                                None,
                                None,
                                None,
                                env_vars,
                                None, // tags
                                TripWire::new_with_on_drop(
                                    None,
                                    Box::new(move || {
                                        if let Some(tx) = &status_tx2 {
                                            let _ = tx.send(LlmStatusEvent::Finished(run_id, None));
                                        }
                                    }),
                                ),
                            )
                            .await;
                        let function_result = res.0?;
                        // Notify final usage if available
                        if let Some(tx) = &status_tx {
                            use crate::LLMResponse;
                            if let LLMResponse::Success(resp) = function_result.llm_response() {
                                let usage = TokenUsage {
                                    prompt_tokens: resp.metadata.prompt_tokens,
                                    output_tokens: resp.metadata.output_tokens,
                                    total_tokens: resp.metadata.total_tokens,
                                    cached_input_tokens: resp.metadata.cached_input_tokens,
                                };
                                let _ = tx.send(LlmStatusEvent::Finished(run_id, Some(usage)));
                            } else {
                                let _ = tx.send(LlmStatusEvent::Finished(run_id, None));
                            }
                        }
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
        // REPL watch handler: collect notifications
        let watch_notifications = Arc::new(Mutex::new(Vec::new()));
        let watch_notifications_clone = watch_notifications.clone();
        let watch_handler = shared_handler(move |notification| {
            watch_notifications_clone
                .lock()
                .unwrap()
                .push(format!("{notification}"));
        });

        let eval_result = interpret_thir(
            "repl".to_string(),
            thir.clone(),
            input_expr_thir,
            handle_llm_function,
            watch_handler,
            variables,
            self.env_vars.clone(),
        )
        .await?;

        let notifications = watch_notifications.lock().unwrap().clone();
        Ok((eval_result, notifications))
    }

    async fn parse_and_evaluate_baml_expression(
        &self,
        input: &str,
    ) -> Result<(BamlValueWithMeta<ExprMetadata>, Vec<String>)> {
        self.parse_and_evaluate_baml_expression_with_status(input, None)
            .await
    }

    fn infer_expression_type(&self, input: &str) -> Result<String> {
        let input = input.trim();

        let hir = match self.runtime.as_ref() {
            Some(runtime) => {
                // Get the internal runtime to access the existing context
                let internal = &runtime;

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
            let internal = &runtime;

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
    #[allow(clippy::print_stdout)]
    pub async fn run(&self) -> Result<()> {
        // Suppress logging while TUI is active so logs don't corrupt the UI
        let _ = baml_log::set_log_level(baml_log::Level::Off);

        // TUI-based REPL using ratatui
        let mut state = ReplState::new();

        if self.from.exists() {
            if let Err(e) = state.load_baml_sources(self.from.clone()) {
                eprintln!("Warning: Could not load initial BAML sources: {e}");
            }
        }

        // One-off evaluation path
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

        // Terminal setup
        enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // UI state
        let mut input = String::new();
        let mut cursor_pos: usize = 0; // cursor position in chars
        #[derive(Clone)]
        enum Msg {
            User(String),
            Output(String),
            Error(String),
        }
        let mut messages: Vec<Msg> = vec![
            Msg::Output("BAML REPL - Interactive Evaluator".into()),
            Msg::Output("Type expressions or commands (e.g., :help)".into()),
        ];
        let mut status: Option<String> = None;
        let mut session_usage: TokenUsageAcc = TokenUsageAcc::default();
        let mut busy = false;
        let mut spinner_idx = 0usize;
        let mut shimmer_pos: usize = 0;
        let spinner_frames: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let tick_rate = Duration::from_millis(80);
        let mut last_tick = Instant::now();

        // Graphical progress (Ghostty OSC 9;4)
        let supports_progress = terminal_supports_graphical_progress();
        let mut progress_active = false;
        let mut progress_percent: i32 = 0;

        // Channel for LLM status updates
        let (status_tx, status_rx) = std::sync::mpsc::channel::<LlmStatusEvent>();

        // Eval result channel (avoid polling JoinHandle futures outside a runtime)
        let (eval_tx, eval_rx) = std::sync::mpsc::channel::<(ReplState, Result<String>)>();

        // Autocomplete data
        let mut fn_candidates = state.function_names();
        let commands: Vec<&'static str> = vec![
            ":load", ":l", ":reset", ":r", ":vars", ":v", ":thir", ":type", ":t", ":help", ":h",
            ":?", ":quit", ":q",
        ];
        let mut suggestions: Vec<String> = Vec::new();
        let mut selected_suggestion: usize = 0;
        let mut show_suggestions: bool = false;
        let mut show_breakdown: bool = false;
        // Where to render the shimmering status line (after which message)

        // History
        let hist_path = history_path();
        let mut history: Vec<String> = load_history_from_file(&hist_path, HISTORY_LIMIT);
        let mut history_cursor: usize = history.len(); // position in history; len() means new entry
        let mut draft_buffer: String = String::new(); // what was in input before first Up

        // Reverse-i-search (Ctrl-R)
        let mut search_mode: bool = false;
        let mut search_query: String = String::new();
        let mut search_from: Option<usize> = None; // starting index for next match
        let mut saved_input_before_search: String = String::new();

        // Run tracking
        let mut run_counter: u64 = 0;
        let mut history_scroll: u16 = 0; // 0 means bottom; grows as user scrolls up
        let mut last_height: u16 = 0; // last frame height for page up/down

        // Main event loop
        'outer: loop {
            // Drain LLM status events
            while let Ok(ev) = status_rx.try_recv() {
                match ev {
                    LlmStatusEvent::Started(_run_id, name) => {
                        busy = true;
                        status = Some(format!("Calling {name}"));
                        if supports_progress {
                            progress_active = true;
                            progress_percent = 1;
                            send_graphical_progress(progress_percent);
                        }
                    }
                    LlmStatusEvent::Finished(_run_id, usage) => {
                        if let Some(u) = usage {
                            session_usage.add(&u);
                        }
                        // Go back to general evaluation state if still busy
                        if busy {
                            status = Some("Evaluating expression".into());
                        }
                        // No explicit anchor; shimmer inserted relative to last user prompt
                        if supports_progress && progress_active {
                            send_graphical_progress(0);
                            progress_active = false;
                        }
                    }
                }
            }

            // Draw
            terminal.draw(|f| {
                let size = f.area();
                last_height = size.height;

                // Main log with interleaved messages
                let mut lines: Vec<Line> = Vec::new();
                let mut last_user_insert_idx: Option<usize> = None;
                for m in &messages {
                    match m {
                        Msg::User(t) => {
                            // Historical prompt: dimmed leader to look more "historical"
                            lines.push(Line::from(vec![
                                TuiSpan::styled(
                                    "🐑❯ ",
                                    Style::default()
                                        .fg(Color::Blue)
                                        .add_modifier(Modifier::DIM)
                                        .add_modifier(Modifier::BOLD),
                                ),
                                TuiSpan::styled(t.clone(), Style::default().fg(Color::Blue)),
                            ]));
                            last_user_insert_idx = Some(lines.len());
                        }
                        Msg::Output(t) => {
                            // Split multi-line outputs into separate lines so layout/scroll is accurate
                            for (i, seg) in t.split('\n').enumerate() {
                                lines.push(Line::from(TuiSpan::raw(seg.to_string())));
                            }
                            // Add an extra gap after each response for readability
                            lines.push(Line::from(""));
                        }
                        Msg::Error(t) => {
                            for (i, seg) in t.split('\n').enumerate() {
                                let styled = TuiSpan::styled(
                                    seg.to_string(),
                                    Style::default().fg(Color::Red),
                                );
                                lines.push(Line::from(styled));
                            }
                            // Add an extra gap after each error for readability
                            lines.push(Line::from(""));
                        }
                    }
                }
                // If there's an active status, render it as part of the history (shimmering)
                if busy {
                    let s = status.clone().unwrap_or_else(|| "Working...".into());
                    let status_icon = spinner_frames[spinner_idx % spinner_frames.len()];
                    let spans = shimmer_spans(&format!(" {status_icon} {s}"));
                    let idx = last_user_insert_idx.unwrap_or(lines.len());
                    let idx = idx.min(lines.len());
                    lines.insert(idx, Line::from(spans));
                }

                // Compute dynamic layout so prompt follows history immediately
                let reserved_rows = 2u16; // prompt + hints rows
                let max_history_rows = size.height.saturating_sub(reserved_rows);
                let history_rows = (lines.len() as u16).min(max_history_rows);

                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(history_rows), // history/messages inline
                        Constraint::Length(1),            // prompt row
                        Constraint::Min(0),               // flexible spacer (push hints to bottom)
                        Constraint::Length(1),            // hints/tokens row (at bottom)
                    ])
                    .split(size);
                let log_area = chunks[0];
                let prompt_area = chunks[1];
                let hints_rect: Rect = chunks[3];

                let lines_len = lines.len() as u16;
                // Preserve leading spaces in outputs (indentation), so don't trim
                let mut output = Paragraph::new(lines).wrap(Wrap { trim: false });
                let needed = lines_len; // number of history lines just computed
                let height = log_area.height;
                let base_scroll = needed.saturating_sub(height);
                // Clamp scroll offset to available history range
                if history_scroll > base_scroll {
                    history_scroll = base_scroll;
                }
                let top_line = base_scroll.saturating_sub(history_scroll);
                if needed > height {
                    output = output.scroll((top_line, 0));
                }
                f.render_widget(output, log_area);

                // Render prompt as its own single line anchored at the bottom
                let prompt_str = "🐑❯ ";
                let prompt_width = UnicodeWidthStr::width(prompt_str) as u16;

                // Compute display widths for input and window around cursor
                let input_chars: Vec<char> = input.chars().collect();
                let total_chars = input_chars.len();
                let cur = cursor_pos.min(total_chars);

                // Per-char column widths (treat width 0 as 1 to keep cursor progress sane)
                let char_widths: Vec<usize> = input_chars
                    .iter()
                    .map(|c| UnicodeWidthChar::width(*c).unwrap_or(1).max(1))
                    .collect();
                let total_cols: usize = char_widths.iter().sum();
                let cursor_cols: usize = char_widths.iter().take(cur).sum();

                let max_display_cols = prompt_area.width.saturating_sub(prompt_width) as usize;

                // Decide start index so that the cursor stays within the viewport
                let mut start = 0usize;
                if total_cols > max_display_cols && cursor_cols > max_display_cols {
                    // Move start forward until width from start..cur fits in the viewport
                    let mut acc = 0usize;
                    for i in (0..cur).rev() {
                        acc += char_widths[i];
                        if acc > max_display_cols {
                            start = i + 1; // exclude the char that overflowed
                            break;
                        }
                    }
                }

                // Determine end index given start and available columns
                let mut end = start;
                let mut used = 0usize;
                while end < total_chars {
                    let w = char_widths[end];
                    if used + w > max_display_cols {
                        break;
                    }
                    used += w;
                    end += 1;
                }
                let visible: String = input_chars[start..end].iter().collect();

                let prompt_line = Paragraph::new(Line::from(vec![
                    TuiSpan::styled(
                        prompt_str,
                        Style::default()
                            .fg(Color::Blue)
                            .add_modifier(Modifier::BOLD),
                    ),
                    // Do not style input text; let the terminal's theme decide
                    TuiSpan::raw(visible.clone()),
                ]));
                f.render_widget(prompt_line, prompt_area);

                // Compute cursor position (set after all rendering at end)
                let cols_in_view_before_cursor: usize = char_widths
                    .iter()
                    .skip(start)
                    .take(cur.saturating_sub(start))
                    .sum();
                let mut cursor_abs = (
                    prompt_area.x + prompt_width + (cols_in_view_before_cursor as u16),
                    prompt_area.y,
                );
                cursor_abs.0 = cursor_abs
                    .0
                    .min(prompt_area.x + prompt_area.width.saturating_sub(1));

                // Suggestions popup above the prompt
                if show_suggestions && !suggestions.is_empty() {
                    let max_items = suggestions.len().min(6) as u16;
                    let width = suggestions.iter().map(|s| s.len()).max().unwrap_or(0) as u16 + 4;
                    let popup_area = Rect {
                        x: prompt_area.x.saturating_add(prompt_width),
                        y: prompt_area.y.saturating_sub(max_items + 2).max(1),
                        width: width.min(size.width.saturating_sub(4)),
                        height: max_items + 2,
                    };
                    let block = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .title(" Suggestions ");
                    f.render_widget(block.clone(), popup_area);
                    let inner = block.inner(popup_area);
                    let mut sug_lines: Vec<Line> = Vec::new();
                    for (i, item) in suggestions.iter().take(max_items as usize).enumerate() {
                        let is_sel =
                            i == selected_suggestion.min(suggestions.len().saturating_sub(1));
                        let style = if is_sel {
                            Style::default()
                                .fg(Color::Black)
                                .bg(Color::Cyan)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::Gray)
                        };
                        sug_lines.push(Line::from(TuiSpan::styled(item.clone(), style)));
                    }
                    let para = Paragraph::new(sug_lines).wrap(Wrap { trim: true });
                    f.render_widget(para, inner);
                }

                // Split bottom hints row into left/right
                let gutter = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                    .split(hints_rect);

                if search_mode {
                    let preview = input.clone();
                    let left_text = format!(" (reverse-i-search)`{search_query}`: {preview}");
                    let left_para = Paragraph::new(Line::from(vec![TuiSpan::styled(
                        left_text,
                        Style::default()
                            .fg(Color::Gray)
                            .add_modifier(Modifier::BOLD),
                    )]));
                    f.render_widget(left_para, gutter[0]);
                } else {
                    // Simple hints; status message itself is rendered inline in the history above
                    let left_text = " ✓ Ready    ↩ send    ^T tokens    ^C quit";
                    let left_para = Paragraph::new(Line::from(vec![TuiSpan::styled(
                        left_text,
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    )]));
                    f.render_widget(left_para, gutter[0]);
                }

                let right_text = if show_breakdown {
                    format!(
                        "tok in:{} out:{} cache:{} total:{}",
                        session_usage.prompt,
                        session_usage.output,
                        session_usage.cached_input,
                        session_usage.total
                    )
                } else {
                    format!("tokens used: {}   context: —", session_usage.total)
                };
                let right_para = Paragraph::new(Line::from(TuiSpan::styled(
                    right_text,
                    Style::default().fg(Color::Gray),
                )));
                f.render_widget(right_para, gutter[1]);

                // Set cursor last so it remains after all rendering
                f.set_cursor_position(cursor_abs);
            })?;

            // Tick spinner
            if last_tick.elapsed() >= tick_rate {
                spinner_idx = (spinner_idx + 1) % spinner_frames.len();
                if busy {
                    shimmer_pos = shimmer_pos.wrapping_add(1);
                    if supports_progress && progress_active {
                        // Animate an indeterminate progress by sweeping 5..95
                        let step = 7;
                        let mut next = progress_percent + step;
                        if next >= 95 {
                            next = 5;
                        }
                        progress_percent = next.clamp(1, 99);
                        send_graphical_progress(progress_percent);
                    }
                }
                last_tick = Instant::now();
            }

            // If evaluation finished, harvest results from channel (non-blocking)
            while let Ok((new_state, res)) = eval_rx.try_recv() {
                state = new_state;
                match res {
                    Ok(r) => messages.push(Msg::Output(r)),
                    Err(e) => messages.push(Msg::Error(format!("Error: {e}"))),
                }
                busy = false;
                status = None;
                if supports_progress && progress_active {
                    send_graphical_progress(0);
                    progress_active = false;
                }
            }

            // Input events with short poll to keep spinner smooth
            if event::poll(Duration::from_millis(30))? {
                let ev = event::read()?;
                if let CEvent::Key(KeyEvent {
                    code, modifiers, ..
                }) = ev
                {
                    // Normalize control combos for macOS terminals that may not set modifiers
                    let ctrl = modifiers.contains(KeyModifiers::CONTROL);
                    // ASCII control chars as fallback (Ctrl-R = 0x12, Ctrl-T = 0x14)
                    let is_ctrl_r = (matches!(code, KeyCode::Char('r')) && ctrl)
                        || matches!(code, KeyCode::Char('\u{12}'));
                    let is_ctrl_t = (matches!(code, KeyCode::Char('t')) && ctrl)
                        || matches!(code, KeyCode::Char('\u{14}'));

                    if is_ctrl_t {
                        show_breakdown = !show_breakdown;
                        continue;
                    }
                    if is_ctrl_r && !search_mode {
                        // enter reverse search
                        search_mode = true;
                        search_query.clear();
                        search_from = None;
                        saved_input_before_search = input.clone();
                        cursor_pos = input.chars().count();
                        continue;
                    }
                    if is_ctrl_r && search_mode {
                        // find previous match from current anchor
                        let start = search_from.unwrap_or(history.len());
                        let mut found: Option<usize> = None;
                        for idx in (0..start).rev() {
                            if history[idx]
                                .to_lowercase()
                                .contains(&search_query.to_lowercase())
                            {
                                found = Some(idx);
                                break;
                            }
                        }
                        if let Some(idx) = found {
                            input = history[idx].clone();
                            search_from = Some(idx);
                            cursor_pos = input.chars().count();
                        }
                        continue;
                    }

                    match (code, modifiers) {
                        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                            break 'outer;
                        }
                        (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                            break 'outer;
                        }
                        (KeyCode::PageUp, _) => {
                            let step = (last_height.saturating_sub(3)).max(1);
                            history_scroll = history_scroll.saturating_add(step);
                        }
                        (KeyCode::PageDown, _) => {
                            let step = (last_height.saturating_sub(3)).max(1);
                            history_scroll = history_scroll.saturating_sub(step);
                        }
                        // Small scrolling with Shift+Up/Down (useful on macOS terminals)
                        (KeyCode::Up, m) if m.contains(KeyModifiers::SHIFT) => {
                            history_scroll = history_scroll.saturating_add(3);
                        }
                        (KeyCode::Down, m) if m.contains(KeyModifiers::SHIFT) => {
                            history_scroll = history_scroll.saturating_sub(3);
                        }
                        (KeyCode::Enter, _) if !busy && !search_mode => {
                            let submitted = input.trim().to_string();
                            if submitted.is_empty() {
                                continue;
                            }
                            if show_suggestions && !suggestions.is_empty() {
                                // accept suggestion
                                let byte_idx = input
                                    .char_indices()
                                    .nth(cursor_pos)
                                    .map(|(i, _)| i)
                                    .unwrap_or(input.len());
                                let before = &input[..byte_idx];
                                let last_ws = before
                                    .char_indices()
                                    .rev()
                                    .find(|&(_, c)| c.is_whitespace());
                                let token_start_byte =
                                    last_ws.map(|(i, c)| i + c.len_utf8()).unwrap_or(0);
                                let choice = suggestions
                                    [selected_suggestion.min(suggestions.len() - 1)]
                                .clone();
                                input.replace_range(token_start_byte..byte_idx, &choice);
                                cursor_pos = input[..token_start_byte].chars().count()
                                    + choice.chars().count();
                                show_suggestions = false;
                                suggestions.clear();
                                continue;
                            }
                            // Commands prefixed with ':' are handled immediately
                            if submitted.starts_with(':') {
                                messages.push(Msg::User(submitted.clone()));
                                match self.handle_command(&mut state, &submitted) {
                                    Ok(Some(msg)) => {
                                        if !msg.is_empty() {
                                            messages.push(Msg::Output(msg));
                                        }
                                        // If load happened, refresh candidates
                                        fn_candidates = state.function_names();
                                    }
                                    Ok(None) => break 'outer, // :quit
                                    Err(e) => messages.push(Msg::Error(format!("Error: {e}"))),
                                }
                            } else {
                                // Kick off async evaluation by moving state into task
                                let moved_state = ReplState {
                                    runtime: state.runtime.take(),
                                    variables: state.variables.clone(),
                                    env_vars: state.env_vars.clone(),
                                    current_run_id: None,
                                };
                                // Track busy/status
                                busy = true;
                                status = Some("Evaluating expression".into());
                                messages.push(Msg::User(submitted.clone()));
                                // Track last user prompt implicitly in render
                                run_counter = run_counter.saturating_add(1);
                                let expr = submitted.clone();
                                let tx = status_tx.clone();
                                // Prefer spawning on the runtime inside moved_state (ensures the task runs even when no global runtime is driving)
                                let tx_result = eval_tx.clone();
                                if let Some(rt_holder) = moved_state
                                    .runtime
                                    .as_ref()
                                    .map(|r| r.async_runtime.clone())
                                {
                                    rt_holder.spawn(async move {
                                        let mut ms = moved_state;
                                        ms.current_run_id = Some(run_counter);
                                        let res = ms
                                            .evaluate_expression_with_status(&expr, Some(tx))
                                            .await;
                                        let _ = tx_result.send((ms, res));
                                    });
                                } else {
                                    tokio::spawn(async move {
                                        let mut ms = moved_state;
                                        ms.current_run_id = Some(run_counter);
                                        let res = ms
                                            .evaluate_expression_with_status(&expr, Some(tx))
                                            .await;
                                        let _ = tx_result.send((ms, res));
                                    });
                                }
                            }
                            // Save to history
                            if history.last().map(|s| s != &submitted).unwrap_or(true) {
                                history.push(submitted.clone());
                                if history.len() > HISTORY_LIMIT {
                                    history.remove(0);
                                }
                                append_history_line(&hist_path, &submitted);
                            }
                            history_cursor = history.len();
                            draft_buffer.clear();
                            input.clear();
                            cursor_pos = 0;
                        }
                        (KeyCode::Enter, _) if search_mode => {
                            // accept current preview
                            search_mode = false;
                            search_query.clear();
                            search_from = None;
                            cursor_pos = input.chars().count();
                        }
                        (KeyCode::Char(ch), _) if !busy && !search_mode => {
                            let byte_idx = input
                                .char_indices()
                                .nth(cursor_pos)
                                .map(|(i, _)| i)
                                .unwrap_or(input.len());
                            input.insert(byte_idx, ch);
                            cursor_pos += 1;
                        }
                        (KeyCode::Backspace, _) if !busy && !search_mode => {
                            if cursor_pos > 0 {
                                let byte_idx = input
                                    .char_indices()
                                    .nth(cursor_pos)
                                    .map(|(i, _)| i)
                                    .unwrap_or(input.len());
                                let prev_byte_idx = input
                                    .char_indices()
                                    .nth(cursor_pos - 1)
                                    .map(|(i, _)| i)
                                    .unwrap_or(0);
                                input.replace_range(prev_byte_idx..byte_idx, "");
                                cursor_pos -= 1;
                            }
                        }
                        (KeyCode::Delete, _) if !busy && !search_mode => {
                            if cursor_pos < input.chars().count() {
                                let start = input
                                    .char_indices()
                                    .nth(cursor_pos)
                                    .map(|(i, _)| i)
                                    .unwrap_or(input.len());
                                let end = input
                                    .char_indices()
                                    .nth(cursor_pos + 1)
                                    .map(|(i, _)| i)
                                    .unwrap_or(input.len());
                                input.replace_range(start..end, "");
                            }
                        }
                        (KeyCode::Tab, _) if !busy && !search_mode => {
                            let byte_idx = input
                                .char_indices()
                                .nth(cursor_pos)
                                .map(|(i, _)| i)
                                .unwrap_or(input.len());
                            let before = &input[..byte_idx];
                            let last_ws = before
                                .char_indices()
                                .rev()
                                .find(|&(_, c)| c.is_whitespace());
                            let token_start = last_ws.map(|(i, c)| i + c.len_utf8()).unwrap_or(0);
                            let prefix = &input[token_start..byte_idx];
                            let pool: Vec<String> =
                                if prefix.starts_with(':') || input.starts_with(':') {
                                    commands.iter().map(|s| s.to_string()).collect()
                                } else {
                                    fn_candidates.clone()
                                };
                            let mut matches: Vec<String> =
                                pool.into_iter().filter(|s| s.starts_with(prefix)).collect();
                            matches.sort();
                            matches.dedup();
                            if matches.is_empty() {
                                show_suggestions = false;
                            } else if matches.len() == 1 {
                                input.replace_range(token_start..byte_idx, &matches[0]);
                                cursor_pos = input[..token_start].chars().count()
                                    + matches[0].chars().count();
                                show_suggestions = false;
                            } else {
                                suggestions = matches;
                                selected_suggestion = 0;
                                show_suggestions = true;
                            }
                        }
                        (KeyCode::Left, _) if !search_mode => {
                            cursor_pos = cursor_pos.saturating_sub(1);
                        }
                        (KeyCode::Right, _) if !search_mode => {
                            if cursor_pos < input.chars().count() {
                                cursor_pos += 1;
                            }
                        }
                        (KeyCode::Home, _) if !search_mode => {
                            cursor_pos = 0;
                        }
                        (KeyCode::End, _) if !search_mode => {
                            cursor_pos = input.chars().count();
                        }
                        // ctrl-r and ctrl-t handled above via normalized detection
                        (KeyCode::Backspace, _) if search_mode => {
                            let _ = search_query.pop();
                            // restart search from end
                            search_from = None;
                            if search_query.is_empty() {
                                input = saved_input_before_search.clone();
                                cursor_pos = input.chars().count();
                            }
                        }
                        (KeyCode::Char(ch), _) if search_mode => {
                            // extend query and search from end
                            search_query.push(ch);
                            let mut found: Option<usize> = None;
                            for idx in (0..history.len()).rev() {
                                if history[idx]
                                    .to_lowercase()
                                    .contains(&search_query.to_lowercase())
                                {
                                    found = Some(idx);
                                    break;
                                }
                            }
                            if let Some(idx) = found {
                                input = history[idx].clone();
                                search_from = Some(idx);
                                cursor_pos = input.chars().count();
                            }
                        }
                        (KeyCode::BackTab, _) if show_suggestions => {
                            if !suggestions.is_empty() {
                                selected_suggestion = if selected_suggestion == 0 {
                                    suggestions.len() - 1
                                } else {
                                    selected_suggestion - 1
                                };
                            }
                        }
                        (KeyCode::Down, _) if show_suggestions => {
                            if !suggestions.is_empty() {
                                selected_suggestion = (selected_suggestion + 1) % suggestions.len();
                            }
                        }
                        (KeyCode::Up, _) if show_suggestions => {
                            if !suggestions.is_empty() {
                                selected_suggestion = if selected_suggestion == 0 {
                                    suggestions.len() - 1
                                } else {
                                    selected_suggestion - 1
                                };
                            }
                        }
                        (KeyCode::Up, _) if !busy && !show_suggestions => {
                            if history_cursor == history.len() {
                                draft_buffer = input.clone();
                            }
                            if history_cursor > 0 {
                                history_cursor -= 1;
                                input = history[history_cursor].clone();
                                cursor_pos = input.chars().count();
                            }
                        }
                        (KeyCode::Down, _) if !busy && !show_suggestions => {
                            if history_cursor < history.len() {
                                history_cursor += 1;
                                if history_cursor == history.len() {
                                    input = draft_buffer.clone();
                                } else {
                                    input = history[history_cursor].clone();
                                }
                                cursor_pos = input.chars().count();
                            }
                        }
                        (KeyCode::Esc, _) if !busy && !search_mode => {
                            if show_suggestions {
                                show_suggestions = false;
                                suggestions.clear();
                            } else {
                                input.clear();
                                cursor_pos = 0;
                            }
                        }
                        (KeyCode::Esc, _) if search_mode => {
                            // cancel search
                            search_mode = false;
                            search_query.clear();
                            search_from = None;
                            input = saved_input_before_search.clone();
                        }
                        _ => {}
                    }
                } else if let CEvent::Mouse(MouseEvent { kind, .. }) = ev {
                    match kind {
                        MouseEventKind::ScrollUp => {
                            history_scroll = history_scroll.saturating_add(3);
                        }
                        MouseEventKind::ScrollDown => {
                            history_scroll = history_scroll.saturating_sub(3);
                        }
                        _ => {}
                    }
                }
            }
        }

        // Cleanup terminal
        disable_raw_mode()?;
        execute!(std::io::stdout(), DisableMouseCapture, LeaveAlternateScreen)?;
        // Ensure the progress bar is hidden on exit
        if supports_progress && progress_active {
            send_graphical_progress(0);
        }
        println!("Goodbye!");
        Ok(())
    }

    #[allow(clippy::print_stdout)]
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
