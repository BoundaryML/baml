use anyhow::Result;
use baml_db::*;
use regex::Regex;
use salsa::{Event, EventKind, Setter};
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompilerPhase {
    Lexer,
    Parser,
    Hir,
    Thir,
    Diagnostics,
    Codegen,
    Metrics,
}

impl CompilerPhase {
    pub const ALL: &'static [CompilerPhase] = &[
        CompilerPhase::Lexer,
        CompilerPhase::Parser,
        CompilerPhase::Hir,
        CompilerPhase::Thir,
        CompilerPhase::Diagnostics,
        CompilerPhase::Codegen,
        CompilerPhase::Metrics,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            CompilerPhase::Lexer => "Lexer (Tokens)",
            CompilerPhase::Parser => "Parser (CST/AST)",
            CompilerPhase::Hir => "HIR (High-level IR)",
            CompilerPhase::Thir => "THIR (Typed IR)",
            CompilerPhase::Diagnostics => "Diagnostics",
            CompilerPhase::Codegen => "Codegen (Bytecode)",
            CompilerPhase::Metrics => "Metrics (Incremental)",
        }
    }

    pub fn next(&self) -> CompilerPhase {
        match self {
            CompilerPhase::Lexer => CompilerPhase::Parser,
            CompilerPhase::Parser => CompilerPhase::Hir,
            CompilerPhase::Hir => CompilerPhase::Thir,
            CompilerPhase::Thir => CompilerPhase::Diagnostics,
            CompilerPhase::Diagnostics => CompilerPhase::Codegen,
            CompilerPhase::Codegen => CompilerPhase::Metrics,
            CompilerPhase::Metrics => CompilerPhase::Lexer,
        }
    }

    pub fn prev(&self) -> CompilerPhase {
        match self {
            CompilerPhase::Lexer => CompilerPhase::Metrics,
            CompilerPhase::Parser => CompilerPhase::Lexer,
            CompilerPhase::Hir => CompilerPhase::Parser,
            CompilerPhase::Thir => CompilerPhase::Hir,
            CompilerPhase::Diagnostics => CompilerPhase::Thir,
            CompilerPhase::Codegen => CompilerPhase::Diagnostics,
            CompilerPhase::Metrics => CompilerPhase::Codegen,
        }
    }
}

pub struct CompilerRunner {
    db: RootDatabase,
    project_root: baml_workspace::ProjectRoot,
    is_directory: bool,
    /// Source files currently in the database (path -> SourceFile)
    source_files: HashMap<PathBuf, SourceFile>,
    phase_outputs: HashMap<CompilerPhase, String>,
    phase_outputs_annotated: HashMap<CompilerPhase, Vec<(String, LineStatus)>>,
    // Track Salsa events to determine what's recomputed vs cached
    recomputed_queries: Arc<Mutex<HashSet<String>>>,
    cached_queries: Arc<Mutex<HashSet<String>>>,
    // Track which files were modified in the last compilation
    modified_files: HashSet<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStatus {
    Recomputed,
    Cached,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualizationMode {
    /// Show which files changed (diff-based coloring)
    Diff,
    /// Show which Salsa queries were recomputed vs cached
    Salsa,
}

impl CompilerRunner {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let is_directory = path.is_dir();

        // Create event tracking
        let recomputed_queries = Arc::new(Mutex::new(HashSet::new()));
        let cached_queries = Arc::new(Mutex::new(HashSet::new()));

        let recomputed_clone = recomputed_queries.clone();
        let cached_clone = cached_queries.clone();

        // Create database with event callback
        let db =
            RootDatabase::new_with_event_callback(Box::new(move |event: Event| match event.kind {
                EventKind::WillExecute { database_key } => {
                    recomputed_clone
                        .lock()
                        .unwrap()
                        .insert(format!("{:?}", database_key));
                }
                EventKind::DidValidateMemoizedValue { database_key } => {
                    cached_clone
                        .lock()
                        .unwrap()
                        .insert(format!("{:?}", database_key));
                }
                _ => {}
            }));

        Self {
            project_root: baml_workspace::ProjectRoot::new(&db, PathBuf::new()),
            db,
            is_directory,
            source_files: HashMap::new(),
            phase_outputs: HashMap::new(),
            phase_outputs_annotated: HashMap::new(),
            recomputed_queries,
            cached_queries,
            modified_files: HashSet::new(),
        }
    }

    /// Compile files from a "fake filesystem" (HashMap of path -> content)
    /// If snapshot_files is provided, we:
    ///   1. Add snapshot files to DB first
    ///   2. Use .set_text() to update to current_files
    /// This allows Salsa to see what changed vs what's cached
    pub fn compile_from_filesystem(
        &mut self,
        current_files: &HashMap<PathBuf, String>,
        snapshot_files: Option<&HashMap<PathBuf, String>>,
    ) -> Result<()> {
        // Clear event tracking
        self.recomputed_queries.lock().unwrap().clear();
        self.cached_queries.lock().unwrap().clear();

        // Create new database with event callback
        let recomputed_clone = self.recomputed_queries.clone();
        let cached_clone = self.cached_queries.clone();

        self.db =
            RootDatabase::new_with_event_callback(Box::new(move |event: Event| match event.kind {
                EventKind::WillExecute { database_key } => {
                    recomputed_clone
                        .lock()
                        .unwrap()
                        .insert(format!("{:?}", database_key));
                }
                EventKind::DidValidateMemoizedValue { database_key } => {
                    cached_clone
                        .lock()
                        .unwrap()
                        .insert(format!("{:?}", database_key));
                }
                _ => {}
            }));

        // Set project root
        let project_path = if self.is_directory {
            current_files
                .keys()
                .next()
                .and_then(|p| p.parent())
                .unwrap_or_else(|| Path::new("."))
        } else {
            current_files
                .keys()
                .next()
                .and_then(|p| p.parent())
                .unwrap_or_else(|| Path::new("."))
        };
        self.project_root = self.db.set_project_root(project_path);

        // Clear the source files list and modified tracking
        self.source_files.clear();
        self.modified_files.clear();

        // If snapshot_files provided, use the "fake filesystem" approach
        if let Some(snapshot) = snapshot_files {
            // Step 1: Add snapshot files to DB
            let mut source_file_map = HashMap::new();
            for (path, content) in snapshot {
                let path_str = path.to_string_lossy().to_string();
                let source_file = self.db.add_file(&path_str, content);
                source_file_map.insert(path.clone(), source_file);
            }

            // Step 2: Use .set_text() to update to current files
            for (path, current_content) in current_files {
                if let Some(source_file) = source_file_map.get(path) {
                    // File exists in snapshot, check if it changed
                    let snapshot_content = snapshot.get(path).unwrap();
                    if snapshot_content != current_content {
                        // File changed - update it
                        source_file
                            .set_text(&mut self.db)
                            .to(current_content.clone());
                        self.modified_files.insert(path.clone());
                    }
                    self.source_files.insert(path.clone(), *source_file);
                } else {
                    // New file not in snapshot, add it
                    let path_str = path.to_string_lossy().to_string();
                    let source_file = self.db.add_file(&path_str, current_content);
                    self.source_files.insert(path.clone(), source_file);
                    self.modified_files.insert(path.clone());
                }
            }
        } else {
            // No snapshot, just add current files (all are "new")
            for (path, content) in current_files {
                let path_str = path.to_string_lossy().to_string();
                let source_file = self.db.add_file(&path_str, content);
                self.source_files.insert(path.clone(), source_file);
                self.modified_files.insert(path.clone());
            }
        }

        // Run all compiler phases
        self.run_all_phases()
    }

    fn run_all_phases(&mut self) -> Result<()> {
        self.phase_outputs.clear();
        self.phase_outputs_annotated.clear();

        for &phase in &[
            CompilerPhase::Lexer,
            CompilerPhase::Parser,
            CompilerPhase::Hir,
            CompilerPhase::Thir,
            CompilerPhase::Diagnostics,
            CompilerPhase::Codegen,
        ] {
            self.run_single_phase(phase)?;
        }

        self.run_single_phase(CompilerPhase::Metrics)?;

        Ok(())
    }

    fn run_single_phase(&mut self, phase: CompilerPhase) -> Result<()> {
        match phase {
            CompilerPhase::Lexer => self.run_lexer(),
            CompilerPhase::Parser => self.run_parser(),
            CompilerPhase::Hir => self.run_hir(),
            CompilerPhase::Thir => self.run_thir(),
            CompilerPhase::Diagnostics => self.run_diagnostics(),
            CompilerPhase::Codegen => self.run_codegen(),
            CompilerPhase::Metrics => self.run_metrics(),
        }
    }

    fn run_lexer(&mut self) -> Result<()> {
        let mut output = String::new();
        let mut output_annotated = Vec::new();

        // Sort files alphabetically by path
        let mut sorted_files: Vec<_> = self.source_files.iter().collect();
        sorted_files.sort_by_key(|(path, _)| path.as_path());

        for (path, source_file) in sorted_files {
            let file_path = path.display().to_string();
            // Check if THIS specific file was modified
            let file_recomputed = self.modified_files.contains(path);

            writeln!(output, "File: {}", file_path).ok();
            output_annotated.push((
                format!("File: {}", file_path),
                if file_recomputed {
                    LineStatus::Recomputed
                } else {
                    LineStatus::Unknown
                },
            ));

            let tokens = baml_lexer::lex_file(&self.db, *source_file);
            for token in tokens {
                let line = format!("{:?} {:?}", token.kind, token.text);
                writeln!(output, "{}", line).ok();
                output_annotated.push((
                    line,
                    if file_recomputed {
                        LineStatus::Recomputed
                    } else {
                        LineStatus::Cached
                    },
                ));
            }
            writeln!(output).ok();
            output_annotated.push((String::new(), LineStatus::Unknown));
        }

        self.phase_outputs.insert(CompilerPhase::Lexer, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Lexer, output_annotated);
        Ok(())
    }

    fn run_parser(&mut self) -> Result<()> {
        let mut output = String::new();
        let mut output_annotated = Vec::new();

        // Sort files alphabetically by path
        let mut sorted_files: Vec<_> = self.source_files.iter().collect();
        sorted_files.sort_by_key(|(path, _)| path.as_path());

        for (path, source_file) in sorted_files {
            let file_path = path.display().to_string();
            // Check if THIS specific file was modified
            let file_recomputed = self.modified_files.contains(path);

            writeln!(output, "File: {}", file_path).ok();
            output_annotated.push((
                format!("File: {}", file_path),
                if file_recomputed {
                    LineStatus::Recomputed
                } else {
                    LineStatus::Unknown
                },
            ));

            let green = baml_parser::parse_green(&self.db, *source_file);
            // Build a red tree (SyntaxNode) from the green tree
            let syntax_tree = baml_syntax::SyntaxNode::new_root(green);
            let tree_text = format!("{:#?}", syntax_tree);
            // Remove span ranges like @0..69 from the output
            let tree_text = remove_span_ranges(&tree_text);
            for line in tree_text.lines() {
                writeln!(output, "{}", line).ok();
                output_annotated.push((
                    line.to_string(),
                    if file_recomputed {
                        LineStatus::Recomputed
                    } else {
                        LineStatus::Cached
                    },
                ));
            }
            writeln!(output).ok();
            output_annotated.push((String::new(), LineStatus::Unknown));
        }

        self.phase_outputs.insert(CompilerPhase::Parser, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Parser, output_annotated);
        Ok(())
    }

    fn run_hir(&mut self) -> Result<()> {
        let mut output = String::new();
        let mut output_annotated = Vec::new();

        // Sort files alphabetically
        let mut sorted_files: Vec<_> = self.source_files.iter().collect();
        sorted_files.sort_by_key(|(path, _)| path.as_path());

        for (path, source_file) in sorted_files {
            let file_path = path.display().to_string();

            // Use real baml_hir for item extraction
            let items = baml_hir::file_items(&self.db, *source_file);

            // Check if THIS specific file was modified
            let file_recomputed = self.modified_files.contains(path);

            writeln!(output, "File: {}", file_path).ok();
            output_annotated.push((format!("File: {}", file_path), LineStatus::Unknown));

            // Show real HIR items
            if !items.is_empty() {
                for item in &items {
                    let item_line = format!("  {}", item);
                    writeln!(output, "{}", item_line).ok();
                    output_annotated.push((
                        item_line,
                        if file_recomputed {
                            LineStatus::Recomputed
                        } else {
                            LineStatus::Cached
                        },
                    ));
                }
            } else {
                let no_items = "  (no items)".to_string();
                writeln!(output, "{}", no_items).ok();
                output_annotated.push((
                    no_items,
                    if file_recomputed {
                        LineStatus::Recomputed
                    } else {
                        LineStatus::Cached
                    },
                ));
            }

            writeln!(output).ok();
            output_annotated.push((String::new(), LineStatus::Unknown));
        }

        self.phase_outputs.insert(CompilerPhase::Hir, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Hir, output_annotated);
        Ok(())
    }

    fn run_thir(&mut self) -> Result<()> {
        // THIR not yet implemented as a tracked function
        let output = "THIR not yet implemented".to_string();

        let output_annotated: Vec<_> = output
            .lines()
            .map(|line| (line.to_string(), LineStatus::Unknown))
            .collect();

        self.phase_outputs.insert(CompilerPhase::Thir, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Thir, output_annotated);
        Ok(())
    }

    fn run_diagnostics(&mut self) -> Result<()> {
        // Diagnostics not yet implemented as a tracked function
        let output = "Diagnostics not yet implemented".to_string();

        let output_annotated: Vec<_> = output
            .lines()
            .map(|line| (line.to_string(), LineStatus::Unknown))
            .collect();

        self.phase_outputs
            .insert(CompilerPhase::Diagnostics, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Diagnostics, output_annotated);
        Ok(())
    }

    fn run_codegen(&mut self) -> Result<()> {
        let bytecode = baml_codegen::generate_project_bytecode(&self.db, self.project_root);
        let output = format!("{:#?}", bytecode);

        let file_recomputed = self.was_query_recomputed(&format!("generate_project_bytecode("));
        let output_annotated: Vec<_> = output
            .lines()
            .map(|line| {
                (
                    line.to_string(),
                    if file_recomputed {
                        LineStatus::Recomputed
                    } else {
                        LineStatus::Cached
                    },
                )
            })
            .collect();

        self.phase_outputs.insert(CompilerPhase::Codegen, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Codegen, output_annotated);
        Ok(())
    }

    fn run_metrics(&mut self) -> Result<()> {
        let mut output = String::new();

        let recomputed = self.recomputed_queries.lock().unwrap();
        let cached = self.cached_queries.lock().unwrap();

        writeln!(output, "Recomputed Queries: {}", recomputed.len()).ok();
        writeln!(output, "Cached Queries: {}", cached.len()).ok();
        writeln!(output).ok();

        if !recomputed.is_empty() {
            writeln!(output, "Recomputed:").ok();
            for query in recomputed.iter() {
                writeln!(output, "  • {}", query).ok();
            }
            writeln!(output).ok();
        }

        if !cached.is_empty() {
            writeln!(output, "Cached:").ok();
            for query in cached.iter() {
                writeln!(output, "  • {}", query).ok();
            }
        }

        let output_annotated: Vec<_> = output
            .lines()
            .map(|line| (line.to_string(), LineStatus::Unknown))
            .collect();

        self.phase_outputs.insert(CompilerPhase::Metrics, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Metrics, output_annotated);
        Ok(())
    }

    fn was_query_recomputed(&self, query_pattern: &str) -> bool {
        self.recomputed_queries
            .lock()
            .unwrap()
            .iter()
            .any(|q| q.contains(query_pattern))
    }

    pub fn get_phase_output(&self, phase: CompilerPhase) -> Option<&str> {
        self.phase_outputs.get(&phase).map(|s| s.as_str())
    }

    pub fn get_phase_output_annotated(
        &self,
        phase: CompilerPhase,
    ) -> Option<&[(String, LineStatus)]> {
        self.phase_outputs_annotated
            .get(&phase)
            .map(|v| v.as_slice())
    }

    pub fn get_recomputation_status(&self, _phase: CompilerPhase) -> RecomputationStatus {
        let recomputed_count = self.recomputed_queries.lock().unwrap().len();
        let cached_count = self.cached_queries.lock().unwrap().len();
        RecomputationStatus::Summary {
            recomputed_count,
            cached_count,
        }
    }

    pub fn get_annotated_output(&self, phase: CompilerPhase) -> Vec<(String, LineStatus)> {
        self.phase_outputs_annotated
            .get(&phase)
            .cloned()
            .unwrap_or_default()
    }

    /// Get annotated output with mode-specific coloring
    pub fn get_annotated_output_with_mode(
        &self,
        phase: CompilerPhase,
        mode: VisualizationMode,
    ) -> Vec<(String, LineStatus)> {
        match mode {
            VisualizationMode::Salsa => {
                // In Salsa mode, use the original annotations (recomputed vs cached)
                self.get_annotated_output(phase)
            }
            VisualizationMode::Diff => {
                // In Diff mode, ALL lines from modified files are red, all from unmodified are green
                self.phase_outputs_annotated
                    .get(&phase)
                    .map(|lines| {
                        lines
                            .iter()
                            .map(|(text, status)| {
                                // Convert status: Recomputed->Red, Cached->Green, Unknown stays Unknown
                                let diff_status = match status {
                                    LineStatus::Recomputed => LineStatus::Recomputed, // File was modified
                                    LineStatus::Cached => LineStatus::Cached, // File unchanged
                                    LineStatus::Unknown => LineStatus::Unknown, // Headers, etc.
                                };
                                (text.clone(), diff_status)
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            }
        }
    }

    pub fn get_metrics_output(&mut self) -> Result<String> {
        if let Some(output) = self.phase_outputs.get(&CompilerPhase::Metrics) {
            Ok(output.clone())
        } else {
            self.run_metrics()?;
            Ok(self
                .phase_outputs
                .get(&CompilerPhase::Metrics)
                .cloned()
                .unwrap_or_default())
        }
    }
}

#[derive(Debug, Clone)]
pub enum RecomputationStatus {
    Summary {
        recomputed_count: usize,
        cached_count: usize,
    },
}

impl RecomputationStatus {
    pub fn recomputed_count(&self) -> usize {
        match self {
            RecomputationStatus::Summary {
                recomputed_count, ..
            } => *recomputed_count,
        }
    }

    pub fn cached_count(&self) -> usize {
        match self {
            RecomputationStatus::Summary { cached_count, .. } => *cached_count,
        }
    }
}

/// Helper to remove span ranges like @0..69 from CST output
fn remove_span_ranges(text: &str) -> String {
    let re = Regex::new(r"@\d+\.\.\d+").unwrap();
    re.replace_all(text, "").to_string()
}

/// Helper to read files from disk into a HashMap
pub fn read_files_from_disk(path: &Path) -> Result<HashMap<PathBuf, String>> {
    let mut files = HashMap::new();

    if path.is_dir() {
        let discovered = baml_workspace::discover_baml_files(path);
        for file_path in discovered {
            if let Ok(content) = std::fs::read_to_string(&file_path) {
                files.insert(file_path, content);
            }
        }
    } else {
        let content = std::fs::read_to_string(path)?;
        files.insert(path.to_path_buf(), content);
    }

    Ok(files)
}
