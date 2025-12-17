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
    candidate::{
        Candidate, CandidateScores, CurrentMetrics, ObjectiveStatus, OptimizableFunction,
        OptimizationObjectives,
    },
    evaluator::{Evaluator, TestEvalResult},
    gepa_runtime::GEPARuntime,
    pareto::{Direction, Objective, ParetoFrontier},
    schema_extractor::{extract_optimizable_function, filter_functions},
    storage::{
        CandidateLineage, NormalizationStats, ObjectiveConfig, OptimizationConfig,
        OptimizationResults, OptimizationState, OptimizationStorage, ParetoCandidate, StatsSummary,
    },
    tui::is_stop_requested,
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
    /// Suppress all stdout output (used when TUI is active)
    pub quiet: bool,
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

    /// Get path to a specific candidate file
    pub fn candidate_path(&self, candidate_id: usize) -> PathBuf {
        self.storage.best_candidate_path(candidate_id)
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

/// Macro to conditionally print based on quiet mode
macro_rules! qprint {
    ($self:expr, $($arg:tt)*) => {
        if !$self.config.quiet {
            println!($($arg)*);
        }
    };
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
        qprint!(
            self,
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
            // Check for stop signal from TUI (user pressed Enter to apply a candidate)
            if is_stop_requested(self.storage.run_dir()) {
                qprint!(self, "\nOptimization stopped by user request.");
                break;
            }

            self.current_iteration += 1;

            qprint!(
                self,
                "\n[Iteration {}/{}]",
                self.current_iteration,
                self.config.trials
            );

            // Decide whether to do a merge or reflection
            // Do a merge every 3rd iteration if we have 2+ candidates on Pareto frontier
            let should_merge = self.current_iteration % 3 == 0
                && self.pareto.len() >= 2
                && self.config.objectives.len() > 1;

            if should_merge {
                // Try to merge two diverse candidates from the Pareto frontier
                if let Some((idx_a, idx_b)) = self.pareto.select_for_merge(&self.candidates) {
                    if self.do_merge_iteration(idx_a, idx_b).await? {
                        continue;
                    }
                    // If merge failed, fall through to regular reflection
                    qprint!(self, "  Merge failed, falling back to reflection...");
                }
            }

            // Regular reflection iteration
            // Select a candidate from the Pareto frontier for reflection
            let parent_idx = self
                .pareto
                .select_for_reflection(&self.candidates)
                .unwrap_or(0);

            let parent = &self.candidates[parent_idx];

            qprint!(
                self,
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

            // Check if we have multiple objectives (beyond just accuracy)
            let has_multiple_objectives = self.config.objectives.len() > 1
                || (self.config.objectives.len() == 1
                    && self.config.objectives[0].name != "accuracy");

            if failures.is_empty() {
                qprint!(self, "  All tests passing!");

                // Check if we've converged
                if self.check_convergence() {
                    qprint!(self, "  Converged - stopping optimization");
                    break;
                }

                // For single-objective (accuracy only), skip if all tests pass
                if !has_multiple_objectives {
                    qprint!(self, "  Single objective (accuracy) at 100%, but checking for Pareto stability...");
                    continue;
                }

                // For multi-objective, continue optimizing other metrics even with 100% accuracy
                qprint!(
                    self,
                    "  Continuing to optimize other objectives (tokens, latency, etc.)..."
                );
            } else {
                qprint!(self, "  Reflecting on {} failures...", failures.len());
            }

            // Save reflection data (even if no failures, for multi-objective optimization)
            self.storage
                .save_reflection(self.current_iteration, parent_idx, &failures)?;

            // Build optimization objectives and current metrics for the reflection
            let optimization_objectives = self.build_optimization_objectives(parent);
            let current_metrics = self.build_current_metrics(parent);

            // Call GEPA ProposeImprovements
            // When there are no failures but multiple objectives, we still want to
            // optimize for token usage, latency, etc.
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
                    Some(&optimization_objectives),
                    current_metrics.as_ref(),
                )
                .await
                .context("Failed to propose improvements")?;

            qprint!(self, "  GEPA proposed improvements: {}", improved.rationale);

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
            qprint!(self, "  Evaluating new candidate #{}...", new_id);
            self.evaluate_candidate(new_id).await?;

            let new_scores = self.candidates[new_id].scores.as_ref().unwrap();
            qprint!(
                self,
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
        qprint!(self, "  Extracting current prompt and schema...");

        // Extract optimizable function
        let opt_func = extract_optimizable_function(&self.user_runtime, &self.function_name)?;

        qprint!(
            self,
            "  Found {} classes, {} enums",
            opt_func.classes.len(),
            opt_func.enums.len()
        );

        // Create initial candidate
        let initial = Candidate::initial(opt_func);
        self.candidates.push(initial);

        // Evaluate initial candidate
        qprint!(self, "  Evaluating initial candidate...");
        self.evaluate_candidate(0).await?;

        let scores = self.candidates[0].scores.as_ref().unwrap();
        qprint!(
            self,
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

    /// Perform a merge iteration - combine two Pareto candidates
    /// Returns true if the merge was successful, false otherwise
    async fn do_merge_iteration(&mut self, idx_a: usize, idx_b: usize) -> Result<bool> {
        let candidate_a = &self.candidates[idx_a];
        let candidate_b = &self.candidates[idx_b];

        qprint!(
            self,
            "  Merging candidates #{} and #{} (Pareto frontier size: {})",
            idx_a,
            idx_b,
            self.pareto.len()
        );

        // Identify strengths of each candidate
        let strengths_a = candidate_a
            .scores
            .as_ref()
            .map(|s| self.pareto.identify_strengths(s, &self.candidates))
            .unwrap_or_default();

        let strengths_b = candidate_b
            .scores
            .as_ref()
            .map(|s| self.pareto.identify_strengths(s, &self.candidates))
            .unwrap_or_default();

        qprint!(
            self,
            "    Candidate #{} strengths: {:?}",
            idx_a,
            strengths_a
        );
        qprint!(
            self,
            "    Candidate #{} strengths: {:?}",
            idx_b,
            strengths_b
        );

        // Call GEPA MergeVariants
        let merged = match self
            .gepa_runtime
            .merge_variants(
                &candidate_a.function,
                &candidate_b.function,
                &strengths_a,
                &strengths_b,
            )
            .await
        {
            Ok(m) => m,
            Err(e) => {
                log::warn!("MergeVariants failed: {}", e);
                return Ok(false);
            }
        };

        qprint!(self, "  Merge rationale: {}", merged.rationale);

        // Create new merged candidate
        let new_id = self.candidates.len();
        let new_candidate = Candidate::from_merge(
            new_id,
            self.current_iteration,
            idx_a,
            idx_b,
            &candidate_a.function,
            merged,
        );

        self.candidates.push(new_candidate);

        // Evaluate the merged candidate
        qprint!(self, "  Evaluating merged candidate #{}...", new_id);
        self.evaluate_candidate(new_id).await?;

        let new_scores = self.candidates[new_id].scores.as_ref().unwrap();
        qprint!(
            self,
            "  Merged candidate #{}: {:.1}% pass rate ({}/{} tests)",
            new_id,
            new_scores.test_pass_rate * 100.0,
            new_scores.tests_passed,
            new_scores.tests_total
        );

        // Show objective values
        for obj in &self.config.objectives {
            let value = obj.get_value(new_scores);
            qprint!(self, "    {}: {:.2}", obj.name, value);
        }

        // Update Pareto frontier
        let added_to_pareto = self.pareto.add(new_id, new_scores, &self.candidates);
        if added_to_pareto {
            qprint!(self, "  Merged candidate added to Pareto frontier!");
        }

        // Save checkpoint
        self.save_checkpoint()?;

        Ok(true)
    }

    /// Check if optimization has converged
    ///
    /// For single-objective (accuracy only): converge when all tests pass
    /// For multi-objective: converge only when no improvement is possible or
    /// we've had no Pareto improvement for several iterations
    fn check_convergence(&self) -> bool {
        // If we only have one objective (accuracy), converge when all tests pass
        let has_only_accuracy =
            self.config.objectives.len() == 1 && self.config.objectives[0].name == "accuracy";

        if has_only_accuracy {
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
            return false;
        }

        // For multi-objective optimization, we should NOT converge just because
        // accuracy hits 100%. We want to continue optimizing other objectives
        // (tokens, latency, etc.) even after accuracy is perfect.

        // Check if Pareto frontier has been stable (no new additions) for
        // several iterations. This indicates we've likely found the optimal
        // trade-off surface.
        let iterations_since_pareto_change = self.iterations_since_pareto_change();

        // If we haven't added to the Pareto frontier in 3+ iterations and
        // all tests are passing, we've likely converged
        if iterations_since_pareto_change >= 3 {
            if let Some(best_idx) = self.pareto.best_weighted(&self.candidates) {
                if let Some(scores) = self
                    .candidates
                    .get(best_idx)
                    .and_then(|c| c.scores.as_ref())
                {
                    if scores.test_pass_rate >= 1.0 {
                        qprint!(
                            self,
                            "  Pareto frontier stable for {} iterations with 100% accuracy",
                            iterations_since_pareto_change
                        );
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Count iterations since the last Pareto frontier change
    fn iterations_since_pareto_change(&self) -> usize {
        // Find the highest iteration number among Pareto frontier candidates
        let max_pareto_iteration = self
            .pareto
            .frontier()
            .iter()
            .filter_map(|&idx| self.candidates.get(idx))
            .map(|c| c.iteration)
            .max()
            .unwrap_or(0);

        self.current_iteration.saturating_sub(max_pareto_iteration)
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

    /// Build optimization objectives with current values for the reflection function
    fn build_optimization_objectives(&self, candidate: &Candidate) -> OptimizationObjectives {
        let objectives = self
            .config
            .objectives
            .iter()
            .map(|obj| {
                let current_value = candidate
                    .scores
                    .as_ref()
                    .map(|s| obj.get_value(s))
                    .unwrap_or(0.0);

                let status = match obj.name.as_str() {
                    "accuracy" => {
                        if current_value >= 1.0 {
                            "All tests passing".to_string()
                        } else if current_value >= 0.8 {
                            "Good, minor improvements needed".to_string()
                        } else if current_value >= 0.5 {
                            "Needs improvement".to_string()
                        } else {
                            "Significant work needed".to_string()
                        }
                    }
                    "tokens" | "prompt_tokens" | "completion_tokens" => {
                        format!("{:.0} tokens avg", current_value)
                    }
                    "latency" => {
                        format!("{:.0}ms avg", current_value)
                    }
                    _ => format!("{:.2}", current_value),
                };

                ObjectiveStatus {
                    name: obj.name.clone(),
                    weight: obj.weight,
                    direction: match obj.direction {
                        Direction::Maximize => "maximize".to_string(),
                        Direction::Minimize => "minimize".to_string(),
                    },
                    current_value,
                    status,
                }
            })
            .collect();

        OptimizationObjectives { objectives }
    }

    /// Build current metrics from candidate scores
    fn build_current_metrics(&self, candidate: &Candidate) -> Option<CurrentMetrics> {
        candidate.scores.as_ref().map(|s| CurrentMetrics {
            test_pass_rate: s.test_pass_rate,
            tests_passed: s.tests_passed,
            tests_total: s.tests_total,
            avg_prompt_tokens: s.avg_prompt_tokens,
            avg_completion_tokens: s.avg_completion_tokens,
            avg_total_tokens: s.avg_prompt_tokens + s.avg_completion_tokens,
            avg_latency_ms: s.avg_latency_ms,
        })
    }
}
