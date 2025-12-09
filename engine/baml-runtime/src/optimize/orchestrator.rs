#![allow(clippy::print_stdout)]
//! GEPA Orchestrator - Main optimization loop
//!
//! Implements the GEPA (Generative Evolution of Prompts and Annotations) algorithm:
//! 1. Initialize with current prompt/schema
//! 2. Evaluate candidate on tests
//! 3. Reflect on failures
//! 4. Propose improvements
//! 5. Update Pareto frontier
//! 6. Iterate until budget exhausted

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};

use super::{
    applier::CandidateApplier,
    candidate::{Candidate, CandidateScores, OptimizableFunction},
    evaluator::{Evaluator, TestEvalResult},
    gepa_runtime::GEPARuntime,
    pareto::{Objective, ParetoFrontier},
    schema_extractor::{extract_optimizable_function, filter_functions},
    storage::{
        CandidateLineage, NormalizationStats, ObjectiveConfig, OptimizationConfig,
        OptimizationResults, OptimizationState, OptimizationStorage, ParetoCandidate, StatsSummary,
    },
};
use crate::{test_executor::TestFilter, BamlRuntime, InternalRuntimeInterface};

/// Configuration for the orchestrator
pub struct OrchestratorConfig {
    pub function_filter: Vec<String>,
    pub test_filter: TestFilter,
    pub trials: usize,
    pub max_evals: usize,
    pub parallel: usize,
    pub objectives: Vec<Objective>,
    pub verbose: bool,
    pub env_vars: HashMap<String, String>,
    pub baml_src_path: PathBuf,
    pub feature_flags: internal_baml_core::feature_flags::FeatureFlags,
}

/// Result of a completed optimization run
pub struct OptimizationRunResult {
    pub function_name: String,
    pub candidates: Vec<Candidate>,
    pub pareto_frontier: Vec<usize>,
    pub best_candidate_id: usize,
    pub storage: OptimizationStorage,
}

impl OptimizationRunResult {
    /// Get the best candidate
    pub fn best_candidate(&self) -> Option<&Candidate> {
        self.candidates.get(self.best_candidate_id)
    }

    /// Get path to the best candidate file
    pub fn best_candidate_path(&self) -> PathBuf {
        self.storage.best_candidate_path(self.best_candidate_id)
    }

    /// Get the size of the Pareto frontier
    pub fn pareto_frontier_size(&self) -> usize {
        self.pareto_frontier.len()
    }
}

/// Main GEPA orchestrator
pub struct GEPAOrchestrator {
    user_runtime: Arc<BamlRuntime>,
    gepa_runtime: GEPARuntime,
    storage: OptimizationStorage,
    config: OrchestratorConfig,
    evaluator: Evaluator,
    applier: CandidateApplier,
    pareto: ParetoFrontier,
    candidates: Vec<Candidate>,
    function_name: String,
    current_iteration: usize,
    total_evals: usize,
}

impl GEPAOrchestrator {
    /// Create a new orchestrator
    pub fn new(
        user_runtime: Arc<BamlRuntime>,
        gepa_runtime: GEPARuntime,
        storage: OptimizationStorage,
        config: OrchestratorConfig,
    ) -> Result<Self> {
        // Find functions to optimize
        let ir = user_runtime.ir();
        let functions = filter_functions(ir, &config.function_filter);

        if functions.is_empty() {
            anyhow::bail!("No functions with tests found to optimize");
        }

        // For now, optimize the first matching function
        // TODO: Support optimizing multiple functions
        let function_name = functions.into_iter().next().unwrap();

        let evaluator = Evaluator::new(config.env_vars.clone(), config.parallel);

        let applier = CandidateApplier::new(
            &config.baml_src_path,
            config.env_vars.clone(),
            config.feature_flags.clone(),
        );

        let pareto = ParetoFrontier::new(config.objectives.clone());

        Ok(Self {
            user_runtime,
            gepa_runtime,
            storage,
            config,
            evaluator,
            applier,
            pareto,
            candidates: Vec::new(),
            function_name,
            current_iteration: 0,
            total_evals: 0,
        })
    }

    /// Run the optimization loop
    pub async fn run(&mut self) -> Result<OptimizationRunResult> {
        println!(
            "\n[Optimization] Optimizing function: {}",
            self.function_name
        );

        // Save configuration
        self.save_config()?;

        // Initialize with current prompt/schema
        self.initialize().await?;

        // Main GEPA loop
        while self.current_iteration < self.config.trials
            && self.total_evals < self.config.max_evals
        {
            self.current_iteration += 1;

            println!(
                "\n[Iteration {}/{}]",
                self.current_iteration, self.config.trials
            );

            // Select a candidate from the Pareto frontier for reflection
            let parent_idx = self
                .pareto
                .select_for_reflection(&self.candidates)
                .unwrap_or(0);

            let parent = &self.candidates[parent_idx];

            println!(
                "  Selected parent candidate #{} (pass rate: {:.1}%)",
                parent.id,
                parent
                    .scores
                    .as_ref()
                    .map(|s| s.test_pass_rate * 100.0)
                    .unwrap_or(0.0)
            );

            // Collect failures from recent evaluations
            let (_, results) = self
                .evaluator
                .evaluate(
                    self.user_runtime.clone(),
                    &self.function_name,
                    &self.config.test_filter,
                )
                .await?;

            let failures = self.evaluator.collect_failures(
                &self.user_runtime,
                &self.function_name,
                &results,
                3,
            );
            let successes = self.evaluator.collect_successes(&results, 2);

            if failures.is_empty() {
                println!("  All tests passing! Checking for convergence...");

                // Check if we've converged
                if self.check_convergence() {
                    println!("  Converged - stopping optimization");
                    break;
                }

                continue;
            }

            println!("  Reflecting on {} failures...", failures.len());

            // Save reflection data
            self.storage
                .save_reflection(self.current_iteration, parent_idx, &failures)?;

            // Call GEPA ProposeImprovements
            let improved = self
                .gepa_runtime
                .propose_improvements(
                    &parent.function,
                    &failures,
                    if successes.is_empty() {
                        None
                    } else {
                        Some(&successes)
                    },
                )
                .await
                .context("Failed to propose improvements")?;

            println!("  GEPA proposed improvements: {}", improved.rationale);

            // Create new candidate
            let new_id = self.candidates.len();
            let new_candidate = Candidate::from_reflection(
                new_id,
                self.current_iteration,
                parent_idx,
                &parent.function,
                improved,
            );

            self.candidates.push(new_candidate);

            // Evaluate new candidate
            println!("  Evaluating new candidate #{}...", new_id);
            self.evaluate_candidate(new_id).await?;

            let new_scores = self.candidates[new_id].scores.as_ref().unwrap();
            println!(
                "  Candidate #{}: {:.1}% pass rate ({}/{} tests)",
                new_id,
                new_scores.test_pass_rate * 100.0,
                new_scores.tests_passed,
                new_scores.tests_total
            );

            // Update Pareto frontier
            self.pareto.add(new_id, new_scores, &self.candidates);

            // Save checkpoint
            self.save_checkpoint()?;
        }

        // Finalize and return results
        self.finalize().await
    }

    /// Initialize with the current prompt/schema as candidate 0
    async fn initialize(&mut self) -> Result<()> {
        println!("  Extracting current prompt and schema...");

        // Extract optimizable function
        let opt_func = extract_optimizable_function(&self.user_runtime, &self.function_name)?;

        println!(
            "  Found {} classes, {} enums",
            opt_func.classes.len(),
            opt_func.enums.len()
        );

        // Create initial candidate
        let initial = Candidate::initial(opt_func);
        self.candidates.push(initial);

        // Evaluate initial candidate
        println!("  Evaluating initial candidate...");
        self.evaluate_candidate(0).await?;

        let scores = self.candidates[0].scores.as_ref().unwrap();
        println!(
            "  Initial: {:.1}% pass rate ({}/{} tests)",
            scores.test_pass_rate * 100.0,
            scores.tests_passed,
            scores.tests_total
        );

        // Add to Pareto frontier
        self.pareto.add(0, scores, &self.candidates);

        // Save initial candidate
        self.storage.save_candidate(&self.candidates[0])?;

        Ok(())
    }

    /// Evaluate a candidate and update its scores
    async fn evaluate_candidate(&mut self, candidate_idx: usize) -> Result<()> {
        let candidate = &self.candidates[candidate_idx];

        // For the initial candidate (id 0), use the base runtime
        // For improved candidates, apply the changes to create a modified runtime
        let runtime_to_use = if candidate_idx == 0 {
            self.user_runtime.clone()
        } else {
            // Create an ImprovedFunction from the candidate's function
            let improved = super::candidate::ImprovedFunction {
                prompt_text: candidate.function.prompt_text.clone(),
                classes: candidate.function.classes.clone(),
                enums: candidate.function.enums.clone(),
                rationale: String::new(), // Not needed for evaluation
            };

            // Apply the changes to create a modified runtime
            match self
                .applier
                .apply(&self.user_runtime, &self.function_name, &improved)
            {
                Ok(modified_runtime) => Arc::new(modified_runtime),
                Err(e) => {
                    log::warn!(
                        "Failed to apply candidate {} changes, using base runtime: {}",
                        candidate_idx,
                        e
                    );
                    self.user_runtime.clone()
                }
            }
        };

        let (scores, _results) = self
            .evaluator
            .evaluate(
                runtime_to_use,
                &self.function_name,
                &self.config.test_filter,
            )
            .await?;

        self.candidates[candidate_idx].scores = Some(scores.clone());
        self.total_evals += scores.tests_total;

        // Save evaluation
        self.storage.save_evaluation(candidate_idx, &scores)?;

        // Save candidate
        self.storage
            .save_candidate(&self.candidates[candidate_idx])?;

        Ok(())
    }

    /// Check if optimization has converged
    fn check_convergence(&self) -> bool {
        // Check if all tests pass
        if let Some(best_idx) = self.pareto.best_weighted(&self.candidates) {
            if let Some(scores) = self
                .candidates
                .get(best_idx)
                .and_then(|c| c.scores.as_ref())
            {
                if scores.test_pass_rate >= 1.0 {
                    return true;
                }
            }
        }

        // Check if we've had no improvement in recent iterations
        // TODO: Implement proper convergence detection
        false
    }

    /// Save the optimization configuration
    fn save_config(&self) -> Result<()> {
        let config = OptimizationConfig {
            function_name: self.function_name.clone(),
            trials: self.config.trials,
            max_evals: self.config.max_evals,
            parallel: self.config.parallel,
            objectives: self
                .config
                .objectives
                .iter()
                .map(|o| ObjectiveConfig {
                    name: o.name.clone(),
                    weight: o.weight,
                    direction: match o.direction {
                        super::pareto::Direction::Maximize => "maximize".to_string(),
                        super::pareto::Direction::Minimize => "minimize".to_string(),
                    },
                })
                .collect(),
            test_filter: vec![], // TODO: Serialize test filter
        };

        self.storage.save_config(&config)
    }

    /// Save a checkpoint
    fn save_checkpoint(&self) -> Result<()> {
        let state = OptimizationState {
            version: "1.0".to_string(),
            baml_cli_version: env!("CARGO_PKG_VERSION").to_string(),
            iteration: self.current_iteration,
            total_evals: self.total_evals,
            budget_remaining: self.config.max_evals.saturating_sub(self.total_evals),
            pareto_frontier_indices: self.pareto.frontier().to_vec(),
            candidate_lineage: self
                .candidates
                .iter()
                .map(|c| {
                    (
                        c.id,
                        CandidateLineage {
                            parents: if c.parent_ids.is_empty() {
                                None
                            } else {
                                Some(c.parent_ids.clone())
                            },
                            method: format!("{:?}", c.method),
                        },
                    )
                })
                .collect(),
            normalization_stats: None, // TODO: Compute stats
        };

        self.storage.save_state(&state)
    }

    /// Finalize the optimization run
    async fn finalize(&mut self) -> Result<OptimizationRunResult> {
        // Update Pareto frontier stats
        self.pareto.update_stats(&self.candidates);

        // Find best candidate
        let best_idx = self.pareto.best_weighted(&self.candidates).unwrap_or(0);

        let initial_score = self.candidates[0]
            .scores
            .as_ref()
            .map(|s| s.test_pass_rate)
            .unwrap_or(0.0);

        let best_score = self.candidates[best_idx]
            .scores
            .as_ref()
            .map(|s| s.test_pass_rate)
            .unwrap_or(0.0);

        // Save Pareto frontier
        let pareto_candidates: Vec<ParetoCandidate> = self
            .pareto
            .frontier()
            .iter()
            .filter_map(|&idx| {
                self.candidates.get(idx).and_then(|c| {
                    c.scores.as_ref().map(|s| ParetoCandidate {
                        id: idx,
                        test_pass_rate: s.test_pass_rate,
                        avg_tokens: s.avg_prompt_tokens + s.avg_completion_tokens,
                        avg_latency_ms: s.avg_latency_ms,
                    })
                })
            })
            .collect();

        self.storage.save_pareto_frontier(&pareto_candidates)?;

        // Save final results
        let results = OptimizationResults {
            function_name: self.function_name.clone(),
            iterations_completed: self.current_iteration,
            total_evaluations: self.total_evals,
            best_candidate_id: best_idx,
            best_test_pass_rate: best_score,
            pareto_frontier: pareto_candidates,
            improvement_over_initial: best_score - initial_score,
        };

        self.storage.save_results(&results)?;

        Ok(OptimizationRunResult {
            function_name: self.function_name.clone(),
            candidates: std::mem::take(&mut self.candidates),
            pareto_frontier: self.pareto.frontier().to_vec(),
            best_candidate_id: best_idx,
            storage: OptimizationStorage::from_existing(self.storage.run_dir())?,
        })
    }
}
