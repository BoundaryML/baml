//! Storage - JSON-based persistence for optimization state and artifacts
//!
//! The types here are similar to those stored elsewhere. These ones exist
//! purely to define the disk format. Changing them in a backwards-incompatible
//! way would invalidate old optimize runs. Forwards-incompatible changes are
//! OK as long as you do not share new serialized state with old versions
//! of `baml-cli`.
//!
//! All optimization state is stored in human-readable JSON format for:
//! - Language-agnostic resumability
//! - Human readability and debugging
//! - Git-friendly diffs
//! - Security (no code execution risk)

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::candidate::{Candidate, CandidateScores, OptimizableFunction, ReflectiveExample};

/// Optimization run configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OptimizationConfig {
    pub function_name: String,
    pub trials: usize,
    pub max_evals: usize,
    pub parallel: usize,
    pub objectives: Vec<ObjectiveConfig>,
    pub test_filter: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObjectiveConfig {
    pub name: String,
    pub weight: f64,
    pub direction: String, // "maximize" or "minimize"
}

/// Full optimization state for checkpointing
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OptimizationState {
    pub version: String,
    pub baml_cli_version: String,
    pub iteration: usize,
    pub total_evals: usize,
    pub budget_remaining: usize,
    pub pareto_frontier_indices: Vec<usize>,
    pub candidate_lineage: HashMap<usize, CandidateLineage>,
    pub normalization_stats: Option<NormalizationStats>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CandidateLineage {
    pub parents: Option<Vec<usize>>,
    pub method: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NormalizationStats {
    pub tokens: StatsSummary,
    pub latency: StatsSummary,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatsSummary {
    pub mean: f64,
    pub std: f64,
    pub min: f64,
    pub max: f64,
}

/// Final optimization results
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OptimizationResults {
    pub function_name: String,
    pub iterations_completed: usize,
    pub total_evaluations: usize,
    pub best_candidate_id: usize,
    pub best_test_pass_rate: f64,
    pub pareto_frontier: Vec<ParetoCandidate>,
    pub improvement_over_initial: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParetoCandidate {
    pub id: usize,
    pub test_pass_rate: f64,
    pub avg_tokens: f64,
    pub avg_latency_ms: f64,
}

/// Handles storage of optimization artifacts
pub struct OptimizationStorage {
    run_dir: PathBuf,
}

impl OptimizationStorage {
    /// Create a new optimization storage for a run
    pub fn new(run_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(run_dir).context("Failed to create run directory")?;

        // Create subdirectories
        std::fs::create_dir_all(run_dir.join("candidates"))?;
        std::fs::create_dir_all(run_dir.join("evaluations"))?;
        std::fs::create_dir_all(run_dir.join("reflections"))?;

        Ok(Self {
            run_dir: run_dir.to_path_buf(),
        })
    }

    /// Load an existing optimization storage
    pub fn from_existing(run_dir: &Path) -> Result<Self> {
        if !run_dir.exists() {
            anyhow::bail!("Run directory does not exist: {}", run_dir.display());
        }

        Ok(Self {
            run_dir: run_dir.to_path_buf(),
        })
    }

    /// Get the run directory path
    pub fn run_dir(&self) -> &Path {
        &self.run_dir
    }

    // =========================================================================
    // Config
    // =========================================================================

    /// Save optimization configuration
    pub fn save_config(&self, config: &OptimizationConfig) -> Result<()> {
        let path = self.run_dir.join("config.json");
        let json = serde_json::to_string_pretty(config)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load optimization configuration
    pub fn load_config(&self) -> Result<OptimizationConfig> {
        let path = self.run_dir.join("config.json");
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;
        let config: OptimizationConfig = serde_json::from_str(&content)?;
        Ok(config)
    }

    // =========================================================================
    // Candidates
    // =========================================================================

    /// Save a candidate as a BAML file with metadata comments
    /// TODO: Render description and alias with valid baml syntax,
    /// instead of in comments above the field.
    pub fn save_candidate(&self, candidate: &Candidate) -> Result<PathBuf> {
        let filename = format!(
            "{:02}_{}.baml",
            candidate.id,
            candidate_method_name(candidate)
        );
        let path = self.run_dir.join("candidates").join(&filename);

        let mut content = String::new();

        // Header comments
        content.push_str(&format!("// Generated candidate #{}\n", candidate.id));
        content.push_str(&format!("// Iteration: {}\n", candidate.iteration));
        if !candidate.parent_ids.is_empty() {
            content.push_str(&format!(
                "// Parent candidates: {:?}\n",
                candidate.parent_ids
            ));
        }
        if let Some(ref scores) = candidate.scores {
            content.push_str(&format!(
                "// Score: {:.2} ({} / {} tests passed)\n",
                scores.test_pass_rate, scores.tests_passed, scores.tests_total
            ));
        }
        content.push('\n');

        // Function definition
        content.push_str(&format!("function {}", candidate.function.function_name));
        // Note: We don't have the full function signature here, just the prompt
        content.push_str(" {\n");
        content.push_str("  prompt #\"\n");
        for line in candidate.function.prompt_text.lines() {
            content.push_str("    ");
            content.push_str(line);
            content.push('\n');
        }
        content.push_str("  \"#\n");
        content.push_str("}\n\n");

        // Class definitions with updated annotations
        for class in &candidate.function.classes {
            if let Some(ref desc) = class.description {
                content.push_str(&format!("/// @description(\"{}\")\n", desc));
            }
            content.push_str(&format!("class {} {{\n", class.class_name));

            for field in &class.fields {
                if let Some(ref desc) = field.description {
                    content.push_str(&format!("  /// @description(\"{}\")\n", desc));
                }
                content.push_str(&format!("  {} {}", field.field_name, field.field_type));
                if let Some(alias) = field.alias.as_ref() {
                    content.push_str(&format!(" @alias({alias})"));
                }
                content.push('\n');
            }

            content.push_str("}\n\n");
        }

        // Enum definitions
        for enum_def in &candidate.function.enums {
            content.push_str(&format!("enum {} {{\n", enum_def.enum_name));
            for value in &enum_def.values {
                content.push_str(&format!("  {}", value));
                if let Some(desc) = enum_def.value_descriptions.get(value) {
                    content.push_str(&format!(" // {}", desc));
                }
                content.push('\n');
            }
            content.push_str("}\n\n");
        }

        std::fs::write(&path, content)?;

        // Also save the full candidate as JSON for programmatic access
        let json_path = self
            .run_dir
            .join("candidates")
            .join(format!("{:02}_candidate.json", candidate.id));
        let json = serde_json::to_string_pretty(candidate)?;
        std::fs::write(&json_path, json)?;

        Ok(path)
    }

    /// Load all candidates from the run directory
    pub fn load_candidates(&self) -> Result<Vec<Candidate>> {
        let candidates_dir = self.run_dir.join("candidates");
        let mut candidates = Vec::new();

        if !candidates_dir.exists() {
            return Ok(candidates);
        }

        for entry in std::fs::read_dir(&candidates_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map(|e| e == "json").unwrap_or(false)
                && path
                    .file_name()
                    .map(|n| n.to_string_lossy().ends_with("_candidate.json"))
                    .unwrap_or(false)
            {
                let content = std::fs::read_to_string(&path)?;
                let candidate: Candidate = serde_json::from_str(&content)?;
                candidates.push(candidate);
            }
        }

        // Sort by ID
        candidates.sort_by_key(|c| c.id);

        Ok(candidates)
    }

    // =========================================================================
    // Evaluations
    // =========================================================================

    /// Save evaluation results for a candidate
    pub fn save_evaluation(&self, candidate_id: usize, scores: &CandidateScores) -> Result<()> {
        let path = self
            .run_dir
            .join("evaluations")
            .join(format!("{:02}_evaluation.json", candidate_id));
        let json = serde_json::to_string_pretty(scores)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load evaluation results for a candidate
    pub fn load_evaluation(&self, candidate_id: usize) -> Result<Option<CandidateScores>> {
        let path = self
            .run_dir
            .join("evaluations")
            .join(format!("{:02}_evaluation.json", candidate_id));

        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path)?;
        let scores: CandidateScores = serde_json::from_str(&content)?;
        Ok(Some(scores))
    }

    // =========================================================================
    // Reflections
    // =========================================================================

    /// Save reflection data for an iteration
    pub fn save_reflection(
        &self,
        iteration: usize,
        parent_id: usize,
        failures: &[ReflectiveExample],
    ) -> Result<()> {
        let path = self
            .run_dir
            .join("reflections")
            .join(format!("iteration_{:02}.json", iteration));

        #[derive(Serialize)]
        struct ReflectionData<'a> {
            iteration: usize,
            parent_candidate_id: usize,
            failure_count: usize,
            failures: &'a [ReflectiveExample],
        }

        let data = ReflectionData {
            iteration,
            parent_candidate_id: parent_id,
            failure_count: failures.len(),
            failures,
        };

        let json = serde_json::to_string_pretty(&data)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    // =========================================================================
    // State (Checkpointing)
    // =========================================================================

    /// Save the current optimization state for resumability
    pub fn save_state(&self, state: &OptimizationState) -> Result<()> {
        let path = self.run_dir.join("state.json");
        let json = serde_json::to_string_pretty(state)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load the optimization state
    pub fn load_state(&self) -> Result<Option<OptimizationState>> {
        let path = self.run_dir.join("state.json");

        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path)?;
        let state: OptimizationState = serde_json::from_str(&content)?;
        Ok(Some(state))
    }

    // =========================================================================
    // Results
    // =========================================================================

    /// Save the final optimization results
    pub fn save_results(&self, results: &OptimizationResults) -> Result<()> {
        let path = self.run_dir.join("final_results.json");
        let json = serde_json::to_string_pretty(results)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load the final optimization results
    pub fn load_results(&self) -> Result<OptimizationResults> {
        let path = self.run_dir.join("final_results.json");
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read results from {}", path.display()))?;
        let results: OptimizationResults = serde_json::from_str(&content)?;
        Ok(results)
    }

    /// Save the Pareto frontier
    pub fn save_pareto_frontier(&self, frontier: &[ParetoCandidate]) -> Result<()> {
        let path = self.run_dir.join("pareto_frontier.json");
        let json = serde_json::to_string_pretty(frontier)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Get path to the best candidate file
    pub fn best_candidate_path(&self, candidate_id: usize) -> PathBuf {
        let filename = format!("{:02}_", candidate_id);
        let candidates_dir = self.run_dir.join("candidates");

        // Find the .baml file for this candidate
        if let Ok(entries) = std::fs::read_dir(&candidates_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(&filename) && name.ends_with(".baml") {
                    return entry.path();
                }
            }
        }

        // Fallback
        candidates_dir.join(format!("{:02}_candidate.baml", candidate_id))
    }
}

fn candidate_method_name(candidate: &Candidate) -> &'static str {
    use super::candidate::CandidateMethod;
    match candidate.method {
        CandidateMethod::Initial => "initial",
        CandidateMethod::Reflection => "reflection",
        CandidateMethod::Merge => "merge",
    }
}
