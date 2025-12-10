//! Pareto Frontier - Multi-objective optimization support
//!
//! Maintains a set of non-dominated candidates when optimizing for
//! multiple objectives (accuracy, tokens, latency, etc.)

use std::collections::HashMap;

use anyhow::{Context, Result};

use super::candidate::{Candidate, CandidateScores};

/// Direction of optimization for an objective
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    /// Higher is better (e.g., accuracy)
    Maximize,
    /// Lower is better (e.g., tokens, latency)
    Minimize,
}

/// An optimization objective with weight
#[derive(Clone, Debug)]
pub struct Objective {
    pub name: String,
    pub weight: f64,
    pub direction: Direction,
}

impl Objective {
    /// Get the value for this objective from scores
    pub fn get_value(&self, scores: &CandidateScores) -> f64 {
        match self.name.as_str() {
            "accuracy" => scores.test_pass_rate,
            "tokens" => scores.avg_prompt_tokens + scores.avg_completion_tokens,
            "prompt_tokens" => scores.avg_prompt_tokens,
            "completion_tokens" => scores.avg_completion_tokens,
            "latency" => scores.avg_latency_ms,
            name if name.starts_with("check:") => {
                let check_name = &name[6..];
                scores.check_scores.get(check_name).copied().unwrap_or(0.0)
            }
            _ => 0.0,
        }
    }

    /// Normalize a value for comparison (higher is always better after normalization)
    pub fn normalize(&self, value: f64, stats: &NormalizationStats) -> f64 {
        let normalized = if stats.std > 0.0 {
            (value - stats.mean) / stats.std
        } else {
            0.0
        };

        match self.direction {
            Direction::Maximize => normalized,
            Direction::Minimize => -normalized,
        }
    }
}

/// Statistics for normalizing objective values
#[derive(Clone, Debug, Default)]
pub struct NormalizationStats {
    pub mean: f64,
    pub std: f64,
    pub min: f64,
    pub max: f64,
}

impl NormalizationStats {
    pub fn from_values(values: &[f64]) -> Self {
        if values.is_empty() {
            return Self::default();
        }

        let n = values.len() as f64;
        let mean = values.iter().sum::<f64>() / n;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
        let std = variance.sqrt();
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        Self {
            mean,
            std,
            min,
            max,
        }
    }
}

/// Pareto frontier maintaining non-dominated candidates
pub struct ParetoFrontier {
    /// Indices of candidates on the frontier
    frontier: Vec<usize>,
    /// Objectives being optimized
    objectives: Vec<Objective>,
    /// Normalization statistics per objective
    stats: HashMap<String, NormalizationStats>,
}

impl ParetoFrontier {
    /// Create a new Pareto frontier with given objectives
    pub fn new(objectives: Vec<Objective>) -> Self {
        Self {
            frontier: Vec::new(),
            objectives,
            stats: HashMap::new(),
        }
    }

    /// Check if candidate A dominates candidate B
    /// A dominates B if A is at least as good on all objectives and strictly better on at least one
    pub fn dominates(&self, a_scores: &CandidateScores, b_scores: &CandidateScores) -> bool {
        let mut dominated_on_any = false;
        let mut worse_on_any = false;

        for obj in &self.objectives {
            let a_val = obj.get_value(a_scores);
            let b_val = obj.get_value(b_scores);

            let a_better = match obj.direction {
                Direction::Maximize => a_val > b_val,
                Direction::Minimize => a_val < b_val,
            };

            let b_better = match obj.direction {
                Direction::Maximize => b_val > a_val,
                Direction::Minimize => b_val < a_val,
            };

            if a_better {
                dominated_on_any = true;
            }
            if b_better {
                worse_on_any = true;
            }
        }

        dominated_on_any && !worse_on_any
    }

    /// Update normalization statistics from all candidates
    pub fn update_stats(&mut self, candidates: &[Candidate]) {
        for obj in &self.objectives {
            let values: Vec<f64> = candidates
                .iter()
                .filter_map(|c| c.scores.as_ref())
                .map(|s| obj.get_value(s))
                .collect();

            self.stats
                .insert(obj.name.clone(), NormalizationStats::from_values(&values));
        }
    }

    /// Add a candidate to the frontier if it's non-dominated
    /// Returns true if the candidate was added
    pub fn add(
        &mut self,
        candidate_idx: usize,
        scores: &CandidateScores,
        all_candidates: &[Candidate],
    ) -> bool {
        // Check if this candidate is dominated by any on the frontier
        for &frontier_idx in &self.frontier {
            if let Some(frontier_scores) = all_candidates
                .get(frontier_idx)
                .and_then(|c| c.scores.as_ref())
            {
                if self.dominates(frontier_scores, scores) {
                    // New candidate is dominated, don't add
                    return false;
                }
            }
        }

        // Remove any frontier candidates dominated by the new one
        // We need to collect indices to remove first to avoid borrow checker issues
        let to_remove: Vec<usize> = self
            .frontier
            .iter()
            .filter(|&&frontier_idx| {
                if let Some(frontier_scores) = all_candidates
                    .get(frontier_idx)
                    .and_then(|c| c.scores.as_ref())
                {
                    self.dominates(scores, frontier_scores)
                } else {
                    false
                }
            })
            .copied()
            .collect();

        self.frontier.retain(|idx| !to_remove.contains(idx));

        // Add the new candidate
        self.frontier.push(candidate_idx);
        true
    }

    /// Get the indices of candidates on the frontier
    pub fn frontier(&self) -> &[usize] {
        &self.frontier
    }

    /// Get the size of the frontier
    pub fn len(&self) -> usize {
        self.frontier.len()
    }

    /// Check if the frontier is empty
    pub fn is_empty(&self) -> bool {
        self.frontier.is_empty()
    }

    /// Select a candidate from the frontier for reflection
    /// Uses weighted scoring to pick a good candidate
    pub fn select_for_reflection(&self, candidates: &[Candidate]) -> Option<usize> {
        if self.frontier.is_empty() {
            return None;
        }

        // For now, select the candidate with the highest weighted score
        self.frontier
            .iter()
            .max_by(|&&a, &&b| {
                let a_score =
                    self.weighted_score(candidates.get(a).and_then(|c| c.scores.as_ref()));
                let b_score =
                    self.weighted_score(candidates.get(b).and_then(|c| c.scores.as_ref()));
                a_score
                    .partial_cmp(&b_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied()
    }

    /// Compute a weighted score for a candidate (higher is better)
    pub fn weighted_score(&self, scores: Option<&CandidateScores>) -> f64 {
        let Some(scores) = scores else {
            return 0.0;
        };

        let mut total = 0.0;
        let mut total_weight = 0.0;

        for obj in &self.objectives {
            let value = obj.get_value(scores);
            let stats = self.stats.get(&obj.name).cloned().unwrap_or_default();
            let normalized = obj.normalize(value, &stats);

            total += normalized * obj.weight;
            total_weight += obj.weight;
        }

        if total_weight > 0.0 {
            total / total_weight
        } else {
            scores.test_pass_rate // Fallback to accuracy
        }
    }

    /// Get the best candidate by weighted score
    pub fn best_weighted(&self, candidates: &[Candidate]) -> Option<usize> {
        self.select_for_reflection(candidates)
    }
}

/// Parse weight arguments from CLI (e.g., "accuracy=0.8,tokens=0.2")
pub fn parse_weight_args(weight_args: &[String]) -> Result<Vec<Objective>> {
    if weight_args.is_empty() {
        // Default: optimize accuracy only
        return Ok(vec![Objective {
            name: "accuracy".to_string(),
            weight: 1.0,
            direction: Direction::Maximize,
        }]);
    }

    let mut objectives = Vec::new();

    for arg in weight_args {
        for part in arg.split(',') {
            let parts: Vec<&str> = part.split('=').collect();
            if parts.len() != 2 {
                anyhow::bail!("Invalid weight format: '{}'. Expected 'name=weight'", part);
            }

            let name = parts[0].trim();
            let weight: f64 = parts[1]
                .trim()
                .parse()
                .with_context(|| format!("Invalid weight value: '{}'", parts[1]))?;

            let direction = match name {
                "accuracy" => Direction::Maximize,
                "tokens" | "prompt_tokens" | "completion_tokens" | "latency" => Direction::Minimize,
                name if name.starts_with("check:") => Direction::Maximize,
                _ => anyhow::bail!(
                    "Unknown objective: '{}'. Valid objectives: accuracy, tokens, prompt_tokens, completion_tokens, latency, check:<name>",
                    name
                ),
            };

            objectives.push(Objective {
                name: name.to_string(),
                weight,
                direction,
            });
        }
    }

    // Ensure accuracy is included
    if !objectives.iter().any(|o| o.name == "accuracy") {
        objectives.insert(
            0,
            Objective {
                name: "accuracy".to_string(),
                weight: 0.5, // Lower weight since other objectives were specified
                direction: Direction::Maximize,
            },
        );
    }

    Ok(objectives)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_scores(pass_rate: f64, tokens: f64, latency: f64) -> CandidateScores {
        CandidateScores {
            test_pass_rate: pass_rate,
            tests_passed: (pass_rate * 10.0) as usize,
            tests_total: 10,
            avg_prompt_tokens: tokens * 0.7,
            avg_completion_tokens: tokens * 0.3,
            avg_latency_ms: latency,
            check_scores: HashMap::new(),
        }
    }

    #[test]
    fn test_dominates() {
        let objectives = vec![
            Objective {
                name: "accuracy".to_string(),
                weight: 1.0,
                direction: Direction::Maximize,
            },
            Objective {
                name: "tokens".to_string(),
                weight: 0.5,
                direction: Direction::Minimize,
            },
        ];

        let frontier = ParetoFrontier::new(objectives);

        // A is better on both objectives
        let a = make_scores(0.9, 100.0, 1000.0);
        let b = make_scores(0.8, 150.0, 1000.0);
        assert!(frontier.dominates(&a, &b));

        // A is better on accuracy, worse on tokens - no dominance
        let a = make_scores(0.9, 200.0, 1000.0);
        let b = make_scores(0.8, 100.0, 1000.0);
        assert!(!frontier.dominates(&a, &b));
        assert!(!frontier.dominates(&b, &a));
    }

    #[test]
    fn test_parse_weight_args() {
        let args = vec!["accuracy=0.8,tokens=0.2".to_string()];
        let objectives = parse_weight_args(&args).unwrap();

        assert_eq!(objectives.len(), 2);
        assert_eq!(objectives[0].name, "accuracy");
        assert_eq!(objectives[0].weight, 0.8);
        assert_eq!(objectives[1].name, "tokens");
        assert_eq!(objectives[1].weight, 0.2);
    }

    #[test]
    fn test_default_objectives() {
        let objectives = parse_weight_args(&[]).unwrap();
        assert_eq!(objectives.len(), 1);
        assert_eq!(objectives[0].name, "accuracy");
    }
}
