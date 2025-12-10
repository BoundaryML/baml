//! Prompt Optimization via GEPA (Generative Evolution of Prompts and Annotations)
//!
//! This module implements BEP-005: Prompt Optimization, using DSPy's GEPA algorithm
//! to optimize BAML function prompts and schema annotations (@description, @alias).
//!
//! Key components:
//! - `gepa_runtime`: Loads and executes the GEPA BAML functions for reflection
//! - `schema_extractor`: Extracts optimizable types from the IR
//! - `candidate`: Data structures for prompt/schema candidates
//! - `evaluator`: Runs tests and collects scores
//! - `pareto`: Multi-objective Pareto frontier management
//! - `storage`: JSON-based persistence for checkpoints and artifacts
//! - `orchestrator`: Main GEPA optimization loop
//! - `applier`: Applies candidate changes to create modified runtimes

pub mod applier;
pub mod candidate;
pub mod evaluator;
pub mod gepa_defaults;
pub mod gepa_runtime;
pub mod orchestrator;
pub mod pareto;
pub mod schema_extractor;
pub mod storage;
