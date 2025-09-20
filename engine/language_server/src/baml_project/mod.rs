// use rustc_hash::FxHashSet;
// use std::sync::Arc;
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    io,
    path::{Path, PathBuf},
    str::FromStr,
    time::Instant,
};

use anyhow::Context;
use baml_lsp_types::{
    BamlFunction, BamlFunctionTestCasePair, BamlGeneratorConfig, BamlParam, BamlParentFunction,
    BamlSpan, SymbolLocation,
};
use baml_runtime::{
    // internal::llm_client::LLMResponse,
    BamlRuntime,
    DiagnosticsError,
    IRHelper,
    // RenderedPrompt,
    // runtime::InternalBamlRuntime
};
use baml_types::{BamlMediaType, BamlValue, GeneratorOutputType, TypeValue};
use file_utils::gather_files;
use generators_lib::{
    version_check::{check_version, GeneratorType, VersionCheckMode},
    GenerateOutput,
};
use internal_baml_diagnostics::Diagnostics;
use lsp_server::Notification;
use lsp_types::{
    Diagnostic, DiagnosticSeverity, Hover, HoverContents, Position, Range, TextDocumentItem,
};
use position_utils::get_word_at_position;
use semver::Version;

use crate::{server::client::Notifier, version, DocumentKey, TextDocument};

pub mod file_utils;
pub mod position_utils;

// --- Helper functions for working with text documents ---

/// Trims a given string by removing non-alphanumeric characters (besides underscores and periods).
pub fn trim_line(s: &str) -> String {
    let res = s
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .to_string();
    res
}

pub struct BamlProject {
    pub root_dir_name: PathBuf,
    // This is the version of the file on disk
    pub files: HashMap<DocumentKey, TextDocument>,
    // This is the version of the file that is currently being edited
    // (unsaved changes)
    pub unsaved_files: HashMap<DocumentKey, TextDocument>,
    pub cached_runtime: Option<(u64, Result<BamlRuntime, Diagnostics>)>,
}

impl Drop for BamlProject {
    fn drop(&mut self) {
        tracing::debug!("Dropping BamlProject");
    }
}

impl std::fmt::Debug for BamlProject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BamlProject")
            .field("root_dir_name", &self.root_dir_name)
            .field("files", &self.files.keys())
            .field("unsaved_files", &self.unsaved_files.keys())
            .field(
                "cached_runtime_hash",
                &self.cached_runtime.as_ref().map(|(hash, _)| hash),
            )
            .finish()
    }
}

impl BamlProject {
    pub fn new(root_dir: PathBuf) -> Self {
        tracing::debug!("Creating BamlProject for {}", root_dir.display());
        Self {
            root_dir_name: root_dir,
            files: HashMap::new(),
            unsaved_files: HashMap::new(),
            cached_runtime: None,
        }
    }

    pub fn list_functions(&mut self, feature_flags: &[String]) -> Vec<BamlFunction> {
        let runtime = self.runtime(HashMap::new(), feature_flags);
        if let Ok(runtime) = runtime {
            runtime.list_functions()
        } else {
            vec![]
        }
    }

    pub fn check_version(
        &self,
        generator_config: &BamlGeneratorConfig,
        is_diagnostic: bool,
    ) -> Option<String> {
        // Convert string parameters to enums
        // let generator_type = match generator_config.output_type.as_str() {
        //     "VSCodeCLI" => GeneratorType::VSCodeCLI,
        //     "VSCode" => GeneratorType::VSCode,
        //     "CLI" => GeneratorType::CLI,
        //     other => return Some(format!("Invalid generator type: {:?}", other)),
        // };
        let generator_type = GeneratorType::VSCode;

        // let version_check_mode = match version_check_mode {
        //     "Strict" => VersionCheckMode::Strict,
        //     "None" => VersionCheckMode::None,
        //     other => return Some(format!("Invalid version check mode: {:?}", other)),
        // };
        let version_check_mode = VersionCheckMode::Strict;

        let Ok(generator_language) =
            GeneratorOutputType::from_str(generator_config.output_type.as_str())
        else {
            return Some(format!(
                "Invalid generator language: {:?}",
                generator_config.output_type
            ));
        };

        check_version(
            &generator_config.version,
            version(),
            generator_type,
            version_check_mode,
            generator_language,
            is_diagnostic,
        )
        .map(|error| error.msg())
    }

    pub fn run_generators_native(
        &mut self,
        no_version_check: Option<bool>,
        feature_flags: &[String],
    ) -> Result<Vec<GenerateOutput>, anyhow::Error> {
        let env = std::env::vars().collect();
        let all_files = self
            .files
            .iter()
            .map(|(document_key, text_document)| {
                let path_buf = document_key.path();
                (PathBuf::from(path_buf), text_document.contents.clone())
            })
            .collect();
        let start_time = Instant::now();

        let runtime = self.runtime(env, feature_flags);
        if let Err(e) = runtime {
            if e.has_errors() {
                tracing::error!("Failed to run codegen: {:?}", e);
                return Err(anyhow::anyhow!("Project has errors."));
            } else {
                tracing::error!("Failed to run codegen: {:?}", e);
                return Err(e.into());
            }
        }
        let runtime = runtime.unwrap();

        let generated = match runtime.run_codegen(
            &all_files,
            no_version_check.unwrap_or(false),
            GeneratorType::VSCode,
        ) {
            Ok(gen) => {
                let elapsed = start_time.elapsed();
                tracing::debug!(
                    "Generated {:?} baml_clients in {:?}ms",
                    gen.len(),
                    elapsed.as_millis()
                );
                gen
            }
            Err(e) => {
                let elapsed = start_time.elapsed();
                tracing::debug!(
                    "Failed to run codegen in {:?}ms: {:?}",
                    elapsed.as_millis(),
                    e
                );
                tracing::error!("Failed to run codegen: {:?}", e);
                return Err(e);
            }
        };

        match generated.len() {
            1 => tracing::debug!(
                "Generated 1 baml_client: {}",
                generated[0].output_dir_full.display()
            ),
            n => tracing::debug!(
                "Generated {n} baml_clients: {}",
                generated
                    .iter()
                    .map(|g| g.output_dir_shorthand.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
        Ok(generated)
    }

    pub fn set_unsaved_file(&mut self, document_key: &DocumentKey, content: Option<String>) {
        tracing::debug!(
            "Setting unsaved file: {}, {}",
            document_key.path().display(),
            content.clone().unwrap_or("None".to_string())
        );
        if let Some(content) = content {
            let text_document = TextDocument::new(content, 0);
            self.unsaved_files
                .insert(document_key.clone(), text_document);
        } else {
            self.unsaved_files.remove(document_key);
        }
        self.cached_runtime = None;
    }
    pub fn remove_unsaved_file(&mut self, document_key: &DocumentKey) {
        self.unsaved_files.remove(document_key);
        self.cached_runtime = None;
    }
    pub fn save_file(&mut self, document_key: &DocumentKey, content: &str) {
        tracing::debug!(
            "Saving file: {}, {}",
            document_key.path().display(),
            content
        );
        let text_document = TextDocument::new(content.to_string(), 0);
        self.files.insert(document_key.clone(), text_document);
        self.unsaved_files.remove(document_key);
        self.cached_runtime = None;
    }

    pub fn update_file(&mut self, document_key: &DocumentKey, content: Option<String>) {
        tracing::debug!(
            "Updating file: {}, {}",
            document_key.path().display(),
            content.clone().unwrap_or("None".to_string())
        );
        if let Some(content) = content {
            let text_document = TextDocument::new(content, 0);
            self.files.insert(document_key.clone(), text_document);
        } else {
            self.files.remove(document_key);
        }
        self.cached_runtime = None;
    }

    /// Load files into the current state. Also return the newly loaded files.
    pub fn load_files(&mut self) -> anyhow::Result<HashMap<DocumentKey, TextDocument>> {
        let workspace_file_paths = gather_files(&self.root_dir_name, false).map_err(|e| {
            anyhow::anyhow!(
                "Failed to gather files from directory {}: {}",
                self.root_dir_name.display(),
                e
            )
        })?;
        let workspace_files = workspace_file_paths
            .into_iter()
            .map(|file_path| {
                let document_key = DocumentKey::from_path(&self.root_dir_name, &file_path)
                    .map_err(|e| {
                        anyhow::anyhow!(
                            "Failed to create document key for file {}: {}",
                            file_path.display(),
                            e
                        )
                    })?;
                let contents = std::fs::read_to_string(&file_path).map_err(|e| {
                    anyhow::anyhow!("Failed to read file {}: {}", file_path.display(), e)
                })?;
                let text_document = TextDocument::new(contents, 0);
                Ok((document_key, text_document))
            })
            .collect::<anyhow::Result<HashMap<_, _>>>()?;

        let project_files = workspace_files.clone();

        self.files = project_files;
        Ok(workspace_files)
    }

    pub fn list_generators(
        &mut self,
        feature_flags: &[String],
    ) -> Result<Vec<BamlGeneratorConfig>, &str> {
        let runtime = self.runtime(HashMap::new(), feature_flags);
        if let Ok(runtime) = runtime {
            Ok(runtime.list_generators())
        } else {
            Ok(vec![])
        }
    }

    pub fn runtime(
        &mut self,
        env_vars: HashMap<String, String>,
        feature_flags: &[String],
    ) -> Result<BamlRuntime, Diagnostics> {
        let mut all_files_for_hash = self.files.iter().collect::<Vec<_>>();

        log::debug!(
            "Baml Project saved files: {:#?}, Unsaved files: {:#?}",
            all_files_for_hash.len(),
            self.unsaved_files.len()
        );
        all_files_for_hash.extend(self.unsaved_files.iter());
        all_files_for_hash.sort_by_key(|(k, _)| k.path());

        let mut hasher = DefaultHasher::new();
        for (key, doc) in &all_files_for_hash {
            key.path().hash(&mut hasher);
            doc.contents.hash(&mut hasher);
        }
        let mut sorted_env_vars = env_vars.iter().collect::<Vec<_>>();
        sorted_env_vars.sort_by_key(|(k, _)| *k);
        for (k, v) in &sorted_env_vars {
            k.hash(&mut hasher);
            v.hash(&mut hasher);
        }
        // Include feature flags in the cache hash
        let mut sorted_flags = feature_flags.to_vec();
        sorted_flags.sort();
        for flag in &sorted_flags {
            flag.hash(&mut hasher);
        }
        let current_hash = hasher.finish();

        if let Some((cached_hash, cached_result)) = &self.cached_runtime {
            if *cached_hash == current_hash {
                tracing::debug!("Runtime cache hit ({})", current_hash);
                return cached_result.clone();
            }
            tracing::debug!(
                "Runtime cache miss (hash mismatch: {} != {})",
                *cached_hash,
                current_hash
            );
        } else {
            tracing::debug!("Runtime cache miss (no cache entry)");
        }

        let files_for_runtime = self
            .files
            .iter()
            .chain(self.unsaved_files.iter())
            .map(|(k, v)| (k.unchecked_to_string(), v.contents.clone()))
            .collect::<HashMap<_, _>>();

        // Convert feature flags to FeatureFlags struct
        tracing::info!(
            "BamlProject::runtime called with feature_flags: {:?}",
            feature_flags
        );
        let feature_flags_struct =
            match internal_baml_core::FeatureFlags::from_vec(feature_flags.to_vec()) {
                Ok(flags) => {
                    tracing::info!(
                        "Successfully converted feature flags to FeatureFlags struct: {:?}",
                        flags
                    );
                    flags
                }
                Err(errors) => {
                    tracing::warn!("Invalid feature flags: {:?}, using empty flags", errors);
                    internal_baml_core::FeatureFlags::new()
                }
            };

        let result = BamlRuntime::from_file_content(
            &self.root_dir_name.to_string_lossy(),
            &files_for_runtime,
            env_vars,
            feature_flags_struct,
        )
        .map_err(|e| match e.downcast::<DiagnosticsError>() {
            Ok(e) => e,
            Err(e) => {
                log::debug!("Error: {e:#?}");
                Diagnostics::new(self.root_dir_name.clone())
            }
        });

        // NOTE: consider using RefCell/RwLock/Mutex separately on this so we can have
        // &self & reduce critical sections as much as possible.
        self.cached_runtime = Some((current_hash, result.clone()));

        result
    }

    pub fn files(&self) -> Vec<String> {
        let mut all_files = self.files.clone();
        self.unsaved_files.iter().for_each(|(k, v)| {
            all_files.insert(k.clone(), v.clone());
        });
        let formatted_files = all_files
            .iter()
            .map(|(k, v)| format!("{}BAML_PATH_SPLTTER{}", k.unchecked_to_string(), v.contents))
            .collect::<Vec<String>>();
        formatted_files
    }
}

pub trait BamlRuntimeExt {
    fn list_function_test_pairs(&self) -> Vec<BamlFunctionTestCasePair>;

    fn search_for_symbol(&self, symbol: &str) -> Option<SymbolLocation>;
    fn search_for_class_locations(&self, symbol: &str) -> Vec<SymbolLocation>;
    fn search_for_enum_locations(&self, symbol: &str) -> Vec<SymbolLocation>;
    fn search_for_type_alias_locations(&self, symbol: &str) -> Vec<SymbolLocation>;
    fn list_functions(&self) -> Vec<BamlFunction>;
    fn list_generators(&self) -> Vec<BamlGeneratorConfig>;
    fn is_valid_class(&self, symbol: &str) -> bool;
    fn is_valid_enum(&self, symbol: &str) -> bool;
    fn is_valid_type_alias(&self, symbol: &str) -> bool;
    fn is_valid_function(&self, symbol: &str) -> bool;
    // fn check_version(
    //     generator_version: &str,
    //     current_version: &str,
    //     generator_type: &str,
    //     version_check_mode: &str,
    //     generator_language: &str,
    //     is_diagnostic: bool,
    // ) -> Option<String>;
}

impl BamlRuntimeExt for BamlRuntime {
    // fn check_version(
    //     generator_version: &str,
    //     current_version: &str,
    //     generator_type: &str,
    //     version_check_mode: &str,
    //     generator_language: &str,
    //     is_diagnostic: bool,
    // ) -> Option<String> {
    //     // Convert string parameters to enums
    //     let generator_type = match generator_type {
    //         "VSCodeCLI" => GeneratorType::VSCodeCLI,
    //         "VSCode" => GeneratorType::VSCode,
    //         "CLI" => GeneratorType::CLI,
    //         other => return Some(format!("Invalid generator type: {:?}", other)),
    //     };

    //     let version_check_mode = match version_check_mode {
    //         "Strict" => VersionCheckMode::Strict,
    //         "None" => VersionCheckMode::None,
    //         other => return Some(format!("Invalid version check mode: {:?}", other)),
    //     };

    //     let Ok(generator_language) = GeneratorOutputType::from_str(generator_language) else {
    //         return Some(format!(
    //             "Invalid generator language: {:?}",
    //             generator_language
    //         ));
    //     };

    //     check_version(
    //         generator_version,
    //         current_version,
    //         generator_type,
    //         version_check_mode,
    //         generator_language,
    //         is_diagnostic,
    //     )
    //     .map(|error| error.msg())
    // }

    fn list_generators(&self) -> Vec<BamlGeneratorConfig> {
        self.codegen_generators()
            .map(|generator| BamlGeneratorConfig {
                output_type: generator.output_type.clone().to_string(),
                version: generator.version.clone(),
                span: BamlSpan {
                    file_path: generator.span.file.path().to_string(),
                    start: generator.span.start,
                    end: generator.span.end,
                    start_line: generator.span.line_and_column().0 .0,
                    end_line: generator.span.line_and_column().1 .0,
                },
            })
            .collect()
    }

    fn is_valid_class(&self, symbol: &str) -> bool {
        self.inner.ir.find_class(symbol).is_ok()
    }

    fn is_valid_enum(&self, symbol: &str) -> bool {
        self.inner.ir.find_enum(symbol).is_ok()
    }

    fn is_valid_type_alias(&self, symbol: &str) -> bool {
        self.inner.ir.find_type_alias(symbol).is_ok()
    }

    fn is_valid_function(&self, symbol: &str) -> bool {
        self.inner.ir.find_function(symbol).is_ok()
    }

    fn search_for_class_locations(&self, symbol: &str) -> Vec<SymbolLocation> {
        self.inner
            .ir
            .find_class_locations(symbol)
            .into_iter()
            .map(|span| {
                let ((start_line, start_character), (end_line, end_character)) =
                    span.line_and_column();
                SymbolLocation {
                    uri: span.file.path().to_string(),
                    start_line,
                    start_character,
                    end_line,
                    end_character,
                }
            })
            .collect()
    }

    fn search_for_enum_locations(&self, symbol: &str) -> Vec<SymbolLocation> {
        self.inner
            .ir
            .find_enum_locations(symbol)
            .into_iter()
            .map(|span| {
                let ((start_line, start_character), (end_line, end_character)) =
                    span.line_and_column();
                SymbolLocation {
                    uri: span.file.path().to_string(),
                    start_line,
                    start_character,
                    end_line,
                    end_character,
                }
            })
            .collect()
    }

    fn search_for_type_alias_locations(&self, symbol: &str) -> Vec<SymbolLocation> {
        self.inner
            .ir
            .find_type_alias_locations(symbol)
            .into_iter()
            .map(|span| {
                let ((start_line, start_character), (end_line, end_character)) =
                    span.line_and_column();
                SymbolLocation {
                    uri: span.file.path().to_string(),
                    start_line,
                    start_character,
                    end_line,
                    end_character,
                }
            })
            .collect()
    }

    fn list_functions(&self) -> Vec<BamlFunction> {
        let ctx = &self.create_ctx_manager(BamlValue::String("wasm".to_string()), None);
        let ctx = ctx.create_ctx_with_default();
        let ctx = ctx.eval_ctx(false);

        self.inner
            .ir
            .walk_functions()
            .map(|f| {
                let snippet = format!(
                    r#"test TestName {{
  functions [{name}]
  args {{
{args}
  }}
}}
"#,
                    name = f.name(),
                    args = {
                        // Convert baml_runtime::TypeIR inputs to baml_types::TypeIR
                        let params = f
                            .inputs()
                            .iter()
                            .map(|(k, runtime_type)| {
                                // Convert runtime TypeIR to internal TypeIR using the walker's type method
                                (k.clone(), runtime_type.clone())
                            })
                            .collect::<indexmap::IndexMap<String, _>>();

                        // Use the IR's get_dummy_args method
                        self.inner.ir.get_dummy_args(2, true, &params)
                    }
                );

                let wasm_span = match f.span() {
                    Some(span) => span.into(),
                    None => BamlSpan::default(),
                };

                BamlFunction {
                    name: f.name().to_string(),
                    span: wasm_span,
                    signature: {
                        let inputs = {
                            let params = f
                                .inputs()
                                .iter()
                                .map(|(k, runtime_type)| (k.clone(), runtime_type.clone()))
                                .collect::<indexmap::IndexMap<String, _>>();

                            self.inner
                                .ir
                                .get_dummy_args(2, false, &params)
                                .split('\n')
                                .map(|line| line.trim().to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        };

                        format!("({}) -> {}", inputs, f.output())
                    },
                    test_snippet: snippet,
                    test_cases: f
                        .walk_tests()
                        .map(|tc| {
                            let params = match tc.test_case_params(&ctx) {
                                Ok(params) => Ok(params
                                    .iter()
                                    .map(|(k, v)| {
                                        let as_str = match v {
                                            Ok(v) => match serde_json::to_string(v) {
                                                Ok(s) => Ok(s),
                                                Err(e) => Err(e.to_string()),
                                            },
                                            Err(e) => Err(e.to_string()),
                                        };

                                        let (value, error) = match as_str {
                                            Ok(s) => (Some(s), None),
                                            Err(e) => (None, Some(e)),
                                        };

                                        BamlParam {
                                            name: k.to_string(),
                                            value,
                                            error,
                                        }
                                    })
                                    .collect()),
                                Err(e) => Err(e.to_string()),
                            };

                            let (mut params, error) = match params {
                                Ok(p) => (p, None),
                                Err(e) => (Vec::new(), Some(e)),
                            };

                            // Any missing params should be set to an error
                            f.inputs().iter().for_each(|(param_name, t)| {
                                if !params.iter().any(|p| p.name == *param_name) && !t.is_optional()
                                {
                                    params.insert(
                                        0,
                                        BamlParam {
                                            name: param_name.to_string(),
                                            value: None,
                                            error: Some("Missing parameter".to_string()),
                                        },
                                    );
                                }
                            });

                            let wasm_span = match tc.span() {
                                Some(span) => span.into(),
                                None => BamlSpan::default(),
                            };
                            let function_name_span = tc
                                .test_case()
                                .functions
                                .iter()
                                .find(|f| f.elem.name() == tc.function().name())
                                .and_then(|f| f.attributes.span.as_ref())
                                .map(|span| span.into());

                            BamlFunctionTestCasePair {
                                name: tc.test_case().name.clone(),
                                inputs: params,
                                error,
                                span: wasm_span,
                                function: {
                                    let f = tc.function();
                                    let (start, end) =
                                        f.span().map_or((0, 0), |f| (f.start, f.end));
                                    BamlParentFunction {
                                        start,
                                        end,
                                        name: f.name().to_string(),
                                    }
                                },
                                function_name_span,
                            }
                        })
                        .collect(),
                }
            })
            .collect()
    }
    fn search_for_symbol(&self, symbol: &str) -> Option<SymbolLocation> {
        let runtime = self.inner.ir.clone();

        if let Ok(walker) = runtime.find_enum(symbol) {
            let elem = walker.span().unwrap();

            let ((s_line, s_character), (e_line, e_character)) = elem.line_and_column();
            return Some(SymbolLocation {
                uri: elem.file.path().to_string(), // Use the variable here
                start_line: s_line,
                start_character: s_character,
                end_line: e_line,
                end_character: e_character,
            });
        }
        if let Ok(walker) = runtime.find_class(symbol) {
            let elem = walker.span().unwrap();

            let _uri_str = elem.file.path().to_string(); // Store the String in a variable
            let ((s_line, s_character), (e_line, e_character)) = elem.line_and_column();
            return Some(SymbolLocation {
                uri: elem.file.path().to_string(), // Use the variable here
                start_line: s_line,
                start_character: s_character,
                end_line: e_line,
                end_character: e_character,
            });
        }
        if let Ok(walker) = runtime.find_type_alias(symbol) {
            let elem = walker.span().unwrap();

            let _uri_str = elem.file.path().to_string(); // Store the String in a variable
            let ((s_line, s_character), (e_line, e_character)) = elem.line_and_column();
            return Some(SymbolLocation {
                uri: elem.file.path().to_string(), // Use the variable here
                start_line: s_line,
                start_character: s_character,
                end_line: e_line,
                end_character: e_character,
            });
        }

        if let Ok(walker) = runtime.find_function(symbol) {
            let elem = walker.span().unwrap();

            let _uri_str = elem.file.path().to_string(); // Store the String in a variable
            let ((s_line, s_character), (e_line, e_character)) = elem.line_and_column();
            return Some(SymbolLocation {
                uri: elem.file.path().to_string(), // Use the variable here
                start_line: s_line,
                start_character: s_character,
                end_line: e_line,
                end_character: e_character,
            });
        }

        if let Ok(walker) = runtime.find_client(symbol) {
            let elem = walker.span().unwrap();

            let _uri_str = elem.file.path().to_string(); // Store the String in a variable
            let ((s_line, s_character), (e_line, e_character)) = elem.line_and_column();

            return Some(SymbolLocation {
                uri: elem.file.path().to_string(), // Use the variable here
                start_line: s_line,
                start_character: s_character,
                end_line: e_line,
                end_character: e_character,
            });
        }

        if let Ok(walker) = runtime.find_retry_policy(symbol) {
            let elem = walker.span().unwrap();

            let _uri_str = elem.file.path().to_string(); // Store the String in a variable
            let ((s_line, s_character), (e_line, e_character)) = elem.line_and_column();
            return Some(SymbolLocation {
                uri: elem.file.path().to_string(), // Use the variable here
                start_line: s_line,
                start_character: s_character,
                end_line: e_line,
                end_character: e_character,
            });
        }

        if let Ok(walker) = runtime.find_template_string(symbol) {
            let elem = walker.span().unwrap();
            let _uri_str = elem.file.path().to_string(); // Store the String in a variable
            let ((s_line, s_character), (e_line, e_character)) = elem.line_and_column();
            return Some(SymbolLocation {
                uri: elem.file.path().to_string(), // Use the variable here
                start_line: s_line,
                start_character: s_character,
                end_line: e_line,
                end_character: e_character,
            });
        }

        None
    }
    fn list_function_test_pairs(&self) -> Vec<BamlFunctionTestCasePair> {
        let ctx = self.create_ctx_manager(BamlValue::String("wasm".to_string()), None);

        let ctx = ctx.create_ctx_with_default();
        let ctx = ctx.eval_ctx(true);

        self.inner
            .ir
            .walk_function_test_pairs()
            .map(|tc| {
                let params = match tc.test_case_params(&ctx) {
                    Ok(params) => Ok(params
                        .iter()
                        .map(|(k, v)| {
                            let as_str = match v {
                                Ok(v) => match serde_json::to_string(v) {
                                    Ok(s) => Ok(s),
                                    Err(e) => Err(e.to_string()),
                                },
                                Err(e) => Err(e.to_string()),
                            };

                            let (value, error) = match as_str {
                                Ok(s) => (Some(s), None),
                                Err(e) => (None, Some(e)),
                            };

                            BamlParam {
                                name: k.to_string(),
                                value,
                                error,
                            }
                        })
                        .collect()),
                    Err(e) => Err(e.to_string()),
                };

                let (mut params, error) = match params {
                    Ok(p) => (p, None),
                    Err(e) => (Vec::new(), Some(e)),
                };
                // Any missing params should be set to an error
                // Any missing params should be set to an error
                tc.function().inputs().iter().for_each(|func_params| {
                    let (param_name, t) = func_params;
                    if !params.iter().any(|p| p.name == *param_name) && !t.is_optional() {
                        params.push(BamlParam {
                            name: param_name.to_string(),
                            value: None,
                            error: Some("Missing parameter".to_string()),
                        });
                    }
                });
                let wasm_span = match tc.span() {
                    Some(span) => span.into(),
                    None => BamlSpan::default(),
                };

                let function_name_span = tc
                    .test_case()
                    .functions
                    .iter()
                    .find(|f| f.elem.name() == tc.function().name())
                    .and_then(|f| f.attributes.span.as_ref())
                    .map(|span| span.into());
                BamlFunctionTestCasePair {
                    name: tc.test_case().name.clone(),
                    inputs: params,
                    error,
                    span: wasm_span,
                    function: {
                        let f = tc.function();
                        let (start, end) = f.span().map_or((0, 0), |f| (f.start, f.end));
                        BamlParentFunction {
                            start,
                            end,
                            name: f.name().to_string(),
                        }
                    },
                    function_name_span,
                }
            })
            .collect()
    }
}

/// The Project struct wraps a WASM project, its runtime, and exposes methods for file updates,
/// diagnostics, symbol lookup, and code generation.
pub struct Project {
    pub baml_project: BamlProject,
    // A callback invoked when a runtime update succeeds (passing diagnostics and a file map).
    // on_success: Box<dyn Fn(WasmDiagnosticError, HashMap<String, String>)>,
    pub current_runtime: Option<BamlRuntime>,
    pub last_successful_runtime: Option<BamlRuntime>,
}

impl std::fmt::Debug for Project {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Project")
    }
}

impl Project {
    /// Creates a new `Project` instance.
    pub fn new(
        baml_project: BamlProject,
        // on_success: F
    ) -> Self
// where
    //     F: Fn(WasmDiagnosticError, HashMap<String, String>) + 'static,
    {
        Self {
            baml_project,
            // on_success: Box::new(on_success),
            current_runtime: None,
            last_successful_runtime: None,
        }
    }

    /// Checks the version of a given generator.
    /// (In this stub, we assume `WasmRuntime::check_version` is available as a static method.)
    pub fn check_version(
        &self,
        generator: &BamlGeneratorConfig,
        _is_diagnostic: bool,
    ) -> Option<String> {
        Some(generator.version.clone())
        // Call your actual WASM runtime version check here.
    }

    /// Iterates over all generators and prints error messages if version mismatches are found.
    pub fn check_version_on_save(&self, feature_flags: &[String]) -> Option<String> {
        let mut first_error_message = None;
        if let Ok(generators) = self.list_generators(&[]) {
            for gen in generators.iter() {
                if let Some(message) = self.check_version(gen, false) {
                    if first_error_message.is_none() {
                        first_error_message = Some(message.clone());
                    }
                    tracing::error!("{}", message);
                }
            }
        }
        first_error_message
    }

    /// Returns true if any generator produces TypeScript output.
    pub fn is_typescript_generator_present(&self, feature_flags: &[String]) -> bool {
        if let Ok(generators) = self.list_generators(&[]) {
            generators
                .iter()
                .any(|g| g.output_type.to_lowercase() == "typescript")
        } else {
            false
        }
    }

    /// Updates the runtime.
    /// Reads all files from the WASM project, builds a map from file URIs to file content,
    /// invokes diagnostics, and calls the success callback.
    /// TODO: Consider pushing diagnostics here.
    pub fn update_runtime(
        &mut self,
        runtime_notifier: Option<Notifier>,
        feature_flags: &[String],
    ) -> anyhow::Result<()> {
        let start_time = Instant::now();
        let fake_env_vars: HashMap<String, String> = HashMap::new();
        let _no_version_check = false;
        let files = self.baml_project.files();
        // dbg!(&files);
        let mut file_map = HashMap::new();
        for file in files {
            // Expecting files to be in the format: "pathBAML_PATH_SPLTTERcontent"
            let parts: Vec<&str> = file.splitn(2, "BAML_PATH_SPLTTER").collect();
            if parts.len() == 2 {
                file_map.insert(parts[0].to_string(), parts[1].to_string());
            }
        }

        if let Some(notifier) = runtime_notifier {
            notifier
                .0
                .send(lsp_server::Message::Notification(Notification::new(
                    "runtime_updated".to_string(),
                    serde_json::json!({
                        "root_path": self.root_path(),
                        "files": file_map,
                    }),
                )))?;
        }

        let runtime = self.baml_project.runtime(fake_env_vars, feature_flags);
        self.current_runtime = runtime.clone().ok();
        if runtime.is_ok() {
            self.last_successful_runtime = runtime.ok();
        }

        let elapsed = start_time.elapsed();
        tracing::debug!("update_runtime took {:?}ms", elapsed.as_millis());
        Ok(())
    }

    // /// Requests diagnostics for the current project.
    // pub fn request_diagnostics(&self) -> Result<(), Box<dyn std::error::Error>> {
    //     if let Some(ref runtime) = self.current_runtime {
    //         let files = self.baml_project.files();
    //         let mut file_map = HashMap::new();
    //         for file in files {
    //             let parts: Vec<&str> = file.splitn(2, "BAML_PATH_SPLTTER").collect();
    //             if parts.len() == 2 {
    //                 file_map.insert(parts[0].to_string(), parts[1].to_string());
    //             }
    //         }
    //         // let diagnostics = self.baml_project.diagnostics(runtime);
    //         // (self.on_success)(diagnostics, file_map);
    //         todo!()
    //     }
    //     Ok(())
    // }
    //

    /// Retrieves a reference to the current runtime or the last successful one.
    pub fn runtime(&self) -> anyhow::Result<&BamlRuntime> {
        if let Some(ref rt) = self.current_runtime {
            Ok(rt)
        } else if let Some(ref rt) = self.last_successful_runtime {
            Ok(rt)
        } else {
            Err(anyhow::anyhow!(
                "BAML Generate failed - Project has errors."
            ))
        }
    }

    /// Returns a map of file URIs to their content.
    pub fn files(&self) -> HashMap<String, String> {
        let files = self.baml_project.files();
        let mut file_map = HashMap::new();
        for file in files {
            let parts: Vec<&str> = file.splitn(2, "BAML_PATH_SPLTTER").collect();
            if parts.len() == 2 {
                file_map.insert(parts[0].to_string(), parts[1].to_string());
            }
        }
        file_map
    }

    /// Replaces the current WASM project with a new one.
    pub fn replace_all_files(&mut self, project: BamlProject) {
        self.baml_project = project;
        self.last_successful_runtime = self.current_runtime.take();
    }

    // /// Records an update to a file that has not yet been saved.
    // pub fn update_unsaved_file(&mut self, file_path: &str, content: String) {
    //     self.baml_project.set_unsaved_file(file_path, Some(content));
    //     // Force runtime update when file changes
    //     self.current_runtime = None;
    // }

    // /// Saves a file and marks the runtime as stale.
    // pub fn save_file<P: AsRef<Path>, S: AsRef<str>>(&mut self, file_path: P, content: S) {
    //     self.baml_project
    //         .save_file(file_path.as_ref().to_str().unwrap(), content.as_ref());
    //     // Force runtime update when file is saved
    //     self.current_runtime = None;
    // }

    /// Reads a file and converts it into a text document.
    pub fn get_file(&self, uri: &str) -> io::Result<TextDocumentItem> {
        // Here we treat the URI as a file path.
        let path = Path::new(uri);
        file_utils::convert_to_text_document(path)
    }

    // /// Updates (or inserts) the file content in the WASM project.
    // pub fn upsert_file(&mut self, file_path: &str, content: Option<String>) {
    //     self.baml_project.update_file(file_path, content);
    //     if self.current_runtime.is_some() {
    //         self.last_successful_runtime = self.current_runtime.take();
    //     }
    // }

    pub fn handle_hover_request(
        &mut self,
        doc: &TextDocumentItem,
        position: &Position,
        notifier: Notifier,
        feature_flags: &[String],
    ) -> anyhow::Result<Option<Hover>> {
        // Force runtime update before handling hover
        self.update_runtime(Some(notifier), feature_flags)
            .map_err(|e| anyhow::anyhow!("Failed to update runtime: {e}"))?;

        let word = get_word_at_position(&doc.text, position);
        let cleaned_word = trim_line(&word);
        if cleaned_word.is_empty() {
            return Ok(None);
        }
        let rt = self
            .runtime()
            .map_err(|e| anyhow::anyhow!("Failed to generate a runtime: {e}"))?;
        let maybe_symbol = rt.search_for_symbol(&cleaned_word);
        match maybe_symbol {
            None => Ok(None),
            Some(symbol_location) => {
                let range = Range {
                    start: Position {
                        line: symbol_location.start_line as u32,
                        character: symbol_location.start_character as u32,
                    },
                    end: Position {
                        line: symbol_location.end_line as u32,
                        character: symbol_location.end_character as u32,
                    },
                };

                let symbol_doc = self
                    .files()
                    .get(&symbol_location.uri)
                    .context("File not found")?
                    .clone();
                let symbol_text_document = TextDocument::new(symbol_doc, 0);
                let hover_lookup_text = symbol_text_document
                    .get_text_range(range)
                    .context("Could not take text range")?;
                Ok(Some(Hover {
                    contents: HoverContents::Scalar(lsp_types::MarkedString::LanguageString(
                        lsp_types::LanguageString {
                            language: "baml".to_string(),
                            value: hover_lookup_text,
                        },
                    )),
                    range: Some(range),
                }))
            }
        }
    }

    /// Returns a list of functions from the WASM runtime.
    pub fn list_functions(&self) -> Result<Vec<BamlFunction>, &str> {
        if let Ok(runtime) = self.runtime() {
            Ok(runtime.list_functions())
        } else {
            Err("BAML Generate failed. Project has errors.")
        }
    }

    /// Returns a list of test cases from the WASM runtime.
    pub fn list_function_test_pairs(&self) -> Result<Vec<BamlFunctionTestCasePair>, &str> {
        if let Ok(runtime) = self.runtime() {
            Ok(runtime.list_function_test_pairs())
        } else {
            Err("BAML Generate failed. Project has errors.")
        }
    }

    /// Returns a list of generator configurations.
    pub fn list_generators(
        &self,
        feature_flags: &[String],
    ) -> Result<Vec<BamlGeneratorConfig>, &str> {
        if let Some(ref runtime) = self.current_runtime {
            Ok(runtime.list_generators())
        } else {
            Err("BAML Generate failed. Project has errors.")
        }
    }

    /// Returns the root path of this project.
    pub fn root_path(&self) -> &Path {
        &self.baml_project.root_dir_name
    }

    // Verifies whether a completion request is valid by checking for unbalanced prompt markers.
    // pub fn verify_completion_request(
    //     &self,
    //     doc: &lsp_types::TextDocumentItem,
    //     position: &lsp_types::Position,
    // ) -> bool {
    //     let text = &doc.text;
    //     let mut open_braces_count = 0;
    //     let mut close_braces_count = 0;
    //     let mut i = 0;

    //     let offset = doc.offset_at(position);
    //     let bytes = text.as_bytes();

    //     while i < offset.saturating_sub(1) {
    //         if bytes[i] == b'{' && bytes[i + 1] == b'{' {
    //             open_braces_count += 1;
    //             i += 2;
    //             continue;
    //         } else if bytes[i] == b'}' && bytes[i + 1] == b'}' {
    //             close_braces_count += 1;
    //             i += 2;
    //             continue;
    //         }
    //         i += 1;
    //     }

    //     if open_braces_count > close_braces_count {
    //         if let Ok(runtime) = self.runtime() {
    //             return runtime.check_if_in_prompt(position.line);
    //         }
    //     }
    //     false
    // }

    /// Runs generators without debouncing.
    /// (This async method simulates generator file generation and then calls one of the provided callbacks.)
    // #[cfg(feature = "async")]
    pub fn run_generators_without_debounce<F, E>(
        &mut self,
        feature_flags: &[String],
        on_success: F,
        on_error: E,
    ) where
        F: Fn(String) + Send,
        E: Fn(String) + Send,
    {
        let start = Instant::now();
        match self.baml_project.run_generators_native(None, feature_flags) {
            Ok(generators) => {
                let mut generated_file_count = 0;
                for gen in generators {
                    // Process each generator and simulate file generation.
                    generated_file_count += gen.files.len();
                    // (File system operations would be performed here.)
                }
                let elapsed = start.elapsed();
                let version = env!("CARGO_PKG_VERSION");
                if generated_file_count > 0 {
                    on_success(format!(
                        "BAML client generated! (took {}ms). CLI version: {}",
                        elapsed.as_millis(),
                        version
                    ));
                }
            }
            Err(e) => {
                tracing::error!("Failed to generate BAML client: {:?}", e);
                on_error(format!("Failed to generate BAML client: {e:?}"));
            }
        }
    }

    // Runs generators with debouncing (here simply an alias).
    // #[cfg(feature = "async")]
    // pub async fn run_generators_with_debounce<F, E>(&mut self, on_success: F, on_error: E)
    // where
    //     F: Fn(String) + Send,
    //     E: Fn(String) + Send,
    // {
    //     self.run_generators_without_debounce(on_success, on_error)
    //         .await;
    // }

    /// Checks if all generators use the same major.minor version.
    /// Returns Ok(()) if they do,
    /// otherwise returns an Err with a descriptive message.
    pub fn get_common_generator_version(&self) -> anyhow::Result<String> {
        // list generators. If we can't get the runtime, we'll error out.
        let generators = self
            .runtime()?
            .codegen_generators()
            .map(|gen| gen.version.as_str());

        common_version_up_to_patch(generators)
    }
}

/// Given a set of SemVer version strings, match them to the same `major.minor`, returning an error otherwise. Invalid semver strings are ignored for the check.
/// an error otherwise.
pub fn common_version_up_to_patch<'a>(
    gen_version_strings: impl IntoIterator<Item = &'a str>,
) -> anyhow::Result<String> {
    let mut major_minor_versions = std::collections::HashMap::new();
    let mut highest_patch_by_major_minor = std::collections::HashMap::new();

    // Track major.minor versions and find highest patch for each
    for version_str in gen_version_strings {
        if let Ok(version) = semver::Version::parse(version_str) {
            let major_minor = format!("{}.{}", version.major, version.minor);

            // Track generators with this major.minor
            major_minor_versions
                .entry(major_minor.clone())
                .or_insert_with(Vec::new)
                .push(version_str);

            // Track highest patch version for this major.minor
            highest_patch_by_major_minor
                .entry(major_minor)
                .and_modify(|highest_patch: &mut u64| {
                    if version.patch > *highest_patch {
                        *highest_patch = version.patch;
                    }
                })
                .or_insert(version.patch);
        } else {
            tracing::warn!("Invalid semver version in generator: {}", version_str);
            // Consider how to handle invalid versions - for now, we ignore them for the check
        }
    }

    // If there's more than one major.minor version, return an error
    if major_minor_versions.len() > 1 {
        let versions_str = major_minor_versions
            .keys()
            .map(|v| format!("'{v}'"))
            .collect::<Vec<_>>()
            .join(", ");

        let message = anyhow::anyhow!(
            "Multiple major.minor versions detected: {versions_str}. Major and minor versions must match across all generators."
        );
        Err(message)
    // If there's only one major.minor version, return it with the highest patch
    } else if let Some((version, _)) = major_minor_versions.into_iter().next() {
        if let Some(highest_patch) = highest_patch_by_major_minor.get(&version) {
            // Parse the version string to create a proper semver::Version
            if let Ok(mut v) = Version::parse(&format!("{version}.0")) {
                // Update with the highest patch version
                v.patch = *highest_patch;
                Ok(v.to_string())
            } else {
                Ok(format!("{version}.{highest_patch}"))
            }
        } else {
            Ok(version)
        }
    // Fallback to the runtime version if no valid versions were found
    } else {
        Err(anyhow::anyhow!("No valid generator versions found"))
    }
}

fn get_dummy_value(
    indent: usize,
    allow_multiline: bool,
    t: &baml_runtime::TypeIR,
) -> Option<String> {
    let indent_str = "  ".repeat(indent);
    match t {
        baml_runtime::TypeIR::Primitive(t, _) => {
            let dummy = match t {
                TypeValue::String => {
                    if allow_multiline {
                        format!(
                            "#\"\n{indent1}hello world\n{indent_str}\"#",
                            indent1 = "  ".repeat(indent + 1)
                        )
                    } else {
                        "\"a_string\"".to_string()
                    }
                }
                TypeValue::Int => "123".to_string(),
                TypeValue::Float => "0.5".to_string(),
                TypeValue::Bool => "true".to_string(),
                TypeValue::Null => "null".to_string(),
                TypeValue::Media(BamlMediaType::Image) => {
                    "{ url \"https://imgs.xkcd.com/comics/standards.png\" }".to_string()
                }
                TypeValue::Media(BamlMediaType::Audio) => {
                    "{ url \"https://actions.google.com/sounds/v1/emergency/beeper_emergency_call.ogg\" }".to_string()
                }
                TypeValue::Media(BamlMediaType::Pdf) => {
                    "{ url \"https://ia801801.us.archive.org/15/items/the-great-gatsby_202101/TheGreatGatsby.pdf\" }".to_string()
                }
                TypeValue::Media(BamlMediaType::Video) => {
                    "{ url \"https://samplelib.com/lib/preview/mp4/sample-5s.mp4\" }".to_string()
                }
            };

            Some(dummy)
        }
        baml_runtime::TypeIR::Literal(_, _) => None,
        baml_runtime::TypeIR::Enum { .. } => None,
        baml_runtime::TypeIR::Class { .. } => None,
        baml_runtime::TypeIR::RecursiveTypeAlias { .. } => None,
        baml_runtime::TypeIR::List(item, _) => {
            let dummy = get_dummy_value(indent + 1, allow_multiline, item);
            // Repeat it 2 times
            match dummy {
                Some(dummy) => {
                    if allow_multiline {
                        Some(format!(
                            "[\n{indent1}{dummy},\n{indent1}{dummy}\n{indent_str}]",
                            dummy = dummy,
                            indent1 = "  ".repeat(indent + 1)
                        ))
                    } else {
                        Some(format!("[{dummy}, {dummy}]"))
                    }
                }
                _ => None,
            }
        }
        baml_runtime::TypeIR::Map(k, v, _) => {
            let dummy_k = get_dummy_value(indent, false, k);
            let dummy_v = get_dummy_value(indent + 1, allow_multiline, v);
            match (dummy_k, dummy_v) {
                (Some(k), Some(v)) => {
                    if allow_multiline {
                        Some(format!(
                            r#"{{
{indent1}{k} {v}
{indent_str}}}"#,
                            indent1 = "  ".repeat(indent + 1),
                        ))
                    } else {
                        Some(format!("{{ {k} {v} }}"))
                    }
                }
                _ => None,
            }
        }
        baml_runtime::TypeIR::Union(fields, _) => fields
            .iter_include_null()
            .iter()
            .filter_map(|f| get_dummy_value(indent, allow_multiline, f))
            .next(),
        baml_runtime::TypeIR::Tuple(vals, _) => {
            let dummy = vals
                .iter()
                .filter_map(|f| get_dummy_value(0, false, f))
                .collect::<Vec<_>>()
                .join(", ");
            Some(format!("({dummy},)"))
        }
        baml_runtime::TypeIR::Arrow(_, _) => None,
        baml_runtime::TypeIR::Top(_) => panic!(
            "TypeIR::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the type resolution phase."
        ),
    }
}

fn get_dummy_field(indent: usize, name: &str, t: &baml_runtime::TypeIR) -> Option<String> {
    let indent_str = "  ".repeat(indent);
    let dummy = get_dummy_value(indent, true, t);
    dummy.map(|dummy| format!("{indent_str}{name} {dummy}"))
}
