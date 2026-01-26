# Implementation Plan: BEP-005 Prompt Optimization

## Overview

Implement `baml-cli optimize` - a new CLI command that uses DSPy's GEPA algorithm to optimize BAML function prompts **and schema annotations** (`@description`, `@alias`) based on test pass rates.

**Key Design Principles:**
1. **BAML-driven GEPA**: The reflection/proposal logic is written in BAML itself, stored in `.baml_optimize/gepa/baml_src/`
2. **Holistic optimization**: Optimizes prompts AND schema annotations together as a cohesive system
3. **User-customizable**: Users can modify the GEPA BAML files to customize optimization behavior

## Architecture Summary

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          baml-cli optimize                               │
├─────────────────────────────────────────────────────────────────────────┤
│  CLI Layer (engine/cli/src/commands.rs)                                 │
│    └── Commands::Optimize(OptimizeArgs)                                 │
├─────────────────────────────────────────────────────────────────────────┤
│  Orchestration (engine/baml-runtime/src/cli/optimize.rs)                │
│    ├── OptimizeArgs - CLI argument parsing                              │
│    ├── GEPAOrchestrator - main optimization loop                        │
│    └── run() - entry point                                              │
├─────────────────────────────────────────────────────────────────────────┤
│  GEPA Runtime (new: engine/baml-runtime/src/optimize/)                  │
│    ├── gepa_runtime.rs - loads & executes .baml_optimize/gepa/          │
│    ├── candidate.rs - prompt+schema candidate management                │
│    ├── evaluator.rs - test execution & scoring                          │
│    ├── schema_extractor.rs - extract reachable types for a function     │
│    ├── candidate_applier.rs - apply candidate changes to runtime        │
│    ├── pareto.rs - multi-objective frontier management                  │
│    └── storage.rs - JSON-based checkpoint/artifact persistence          │
├─────────────────────────────────────────────────────────────────────────┤
│  Embedded GEPA BAML (engine/baml-runtime/src/optimize/gepa_defaults/)   │
│    ├── gepa.baml - ProposeImprovements, MergeVariants functions         │
│    └── clients.baml - ReflectionModel client config                     │
├─────────────────────────────────────────────────────────────────────────┤
│  Existing Infrastructure (reused)                                        │
│    ├── BamlRuntime - function/test execution                            │
│    ├── IntermediateRepr - IR with Function, Class, Enum definitions     │
│    └── TestConstraintsResult - @@assert/@@check evaluation              │
└─────────────────────────────────────────────────────────────────────────┘
```

## What Gets Optimized

### Optimization Target: `OptimizableFunction`

For each function being optimized, we extract:

```rust
struct OptimizableFunction {
    function_name: String,
    prompt_text: String,
    classes: Vec<ClassDefinition>,   // All reachable classes
    enums: Vec<EnumDefinition>,      // All reachable enums
}

struct ClassDefinition {
    class_name: String,
    description: Option<String>,     // Class-level @description
    fields: Vec<SchemaFieldDefinition>,
}

struct SchemaFieldDefinition {
    field_name: String,
    field_type: String,
    description: Option<String>,     // @description("...")
    aliases: Vec<String>,            // @alias(...)
    is_optional: bool,
}

struct EnumDefinition {
    enum_name: String,
    values: Vec<String>,
    value_descriptions: HashMap<String, String>,  // Value-level descriptions
}
```

### What Changes in a Candidate

The GEPA `ProposeImprovements` function returns:

```rust
struct ImprovedFunction {
    prompt_text: String,
    classes: Vec<ClassDefinition>,   // Only modified classes
    enums: Vec<EnumDefinition>,      // Only modified enums
    rationale: String,
}
```

Changes can include:
- **Prompt text**: Completely rewritten or refined
- **Field descriptions**: Added/improved `@description` annotations
- **Field aliases**: Added `@alias` to catch LLM output variations
- **Class descriptions**: Class-level guidance for the LLM
- **Enum value descriptions**: Better descriptions for enum parsing

## Implementation Phases

### Phase 1: CLI Scaffold & GEPA BAML Setup

**Files to create/modify:**

1. **engine/cli/src/commands.rs** - Add `Optimize` variant
2. **engine/baml-runtime/src/cli/mod.rs** - Add `pub mod optimize;`
3. **engine/baml-runtime/src/cli/optimize.rs** - CLI args and entry point

**OptimizeArgs:**
```rust
#[derive(Parser, Debug)]
pub struct OptimizeArgs {
    #[arg(long, default_value = ".")]
    pub from: PathBuf,

    #[arg(long, short = 'f')]
    pub function: Vec<String>,

    #[arg(long, short = 't')]
    pub test: Vec<String>,

    #[arg(long, default_value_t = 50)]
    pub max_evals: usize,

    #[arg(long, default_value_t = 20)]
    pub trials: usize,

    #[arg(long)]
    pub auto: Option<AutoBudget>,  // light, medium, heavy

    #[arg(long)]
    pub weight: Vec<String>,  // accuracy=0.8,tokens=0.2

    #[arg(long)]
    pub resume: Option<PathBuf>,

    #[arg(long, default_value_t = false)]
    pub reset_gepa_prompts: bool,

    #[arg(long, default_value_t = 4)]
    pub parallel: usize,

    #[arg(long)]
    pub output_dir: Option<PathBuf>,

    #[arg(long, default_value_t = false)]
    pub verbose: bool,

    #[command(flatten)]
    dotenv: dotenv::DotenvArgs,
}
```

4. **Embedded GEPA BAML files** in `engine/baml-runtime/src/optimize/gepa_defaults/`
   - `gepa.baml` - Core reflection functions
   - `clients.baml` - Default ReflectionModel (gpt-4o)

These are embedded via `include_str!()` and written to `.baml_optimize/gepa/baml_src/` on first run.

### Phase 2: Schema Extraction

**New module: `engine/baml-runtime/src/optimize/schema_extractor.rs`**

```rust
/// Extract all types reachable from a function's input/output types
pub fn extract_optimizable_function(
    ir: &IntermediateRepr,
    function_name: &str,
) -> Result<OptimizableFunction>;

/// Walk the type graph to find all reachable classes and enums
fn collect_reachable_types(
    ir: &IntermediateRepr,
    root_type: &FieldType,
    classes: &mut Vec<ClassDefinition>,
    enums: &mut Vec<EnumDefinition>,
);
```

This needs to:
1. Get the function's output type from IR
2. Recursively walk the type to find all referenced classes/enums
3. Extract `@description` and `@alias` attributes from each field
4. Build the `OptimizableFunction` structure to pass to GEPA

### Phase 3: GEPA Runtime

**New module: `engine/baml-runtime/src/optimize/gepa_runtime.rs`**

```rust
pub struct GEPARuntime {
    runtime: Arc<BamlRuntime>,  // Runtime for the GEPA baml_src
    env_vars: HashMap<String, String>,
}

impl GEPARuntime {
    /// Initialize GEPA runtime from .baml_optimize/gepa/baml_src/
    /// Creates default files if they don't exist
    pub fn new(
        optimize_dir: &Path,
        env_vars: HashMap<String, String>,
        reset_defaults: bool,
    ) -> Result<Self>;

    /// Check version and warn if outdated
    pub fn check_version(&self) -> Result<VersionStatus>;

    /// Call ProposeImprovements function
    pub async fn propose_improvements(
        &self,
        current: &OptimizableFunction,
        failures: &[ReflectiveExample],
        successes: Option<&[ReflectiveExample]>,
    ) -> Result<ImprovedFunction>;

    /// Call MergeVariants function
    pub async fn merge_variants(
        &self,
        variant_a: &OptimizableFunction,
        variant_b: &OptimizableFunction,
        a_strengths: &[String],
        b_strengths: &[String],
    ) -> Result<ImprovedFunction>;
}
```

**Key insight**: We create a separate `BamlRuntime` instance for the GEPA BAML files. This runtime uses the `ReflectionModel` client defined in the user's (or default) `clients.baml`.

### Phase 4: Candidate Management & Application

**New module: `engine/baml-runtime/src/optimize/candidate.rs`**

```rust
pub struct Candidate {
    pub id: usize,
    pub iteration: usize,
    pub parent_ids: Vec<usize>,
    pub method: CandidateMethod,  // Initial, Reflection, Merge
    pub function: OptimizableFunction,
    pub scores: Option<CandidateScores>,
}

pub struct CandidateScores {
    pub test_pass_rate: f64,
    pub avg_prompt_tokens: f64,
    pub avg_completion_tokens: f64,
    pub avg_latency_ms: f64,
    pub check_scores: HashMap<String, f64>,
}
```

**New module: `engine/baml-runtime/src/optimize/candidate_applier.rs`**

```rust
/// Apply a candidate's changes to create a modified runtime for evaluation
pub fn apply_candidate(
    base_runtime: &BamlRuntime,
    candidate: &Candidate,
) -> Result<BamlRuntime>;
```

This is the trickiest part. Options:

**Option A: IR Modification (Recommended)**
- Clone the `IntermediateRepr`
- Modify the function's `prompt_template` in `FunctionConfig`
- Modify class/enum definitions to update descriptions and aliases
- Create new `BamlRuntime` with modified IR

**Option B: Temporary BAML Files**
- Write candidate BAML to temp directory
- Merge with original baml_src
- Load new runtime from merged directory

**Recommendation**: Option A is cleaner but requires understanding IR mutation. Option B is more straightforward but involves filesystem operations.

### Phase 5: Test Evaluation

**New module: `engine/baml-runtime/src/optimize/evaluator.rs`**

```rust
pub struct Evaluator {
    test_filter: TestFilter,
    env_vars: HashMap<String, String>,
    parallel: usize,
}

impl Evaluator {
    /// Evaluate a candidate by running matching tests
    pub async fn evaluate(
        &self,
        runtime: &BamlRuntime,
        candidate: &mut Candidate,
    ) -> Result<()>;

    /// Collect failure details for reflection
    pub fn collect_failures(
        &self,
        test_results: &[TestResult],
    ) -> Vec<ReflectiveExample>;
}
```

Reuses existing test infrastructure from `test_executor/mod.rs`.

### Phase 6: Pareto Frontier

**New module: `engine/baml-runtime/src/optimize/pareto.rs`**

```rust
pub struct ParetoFrontier {
    candidates: Vec<usize>,  // Indices into candidate pool
    objectives: Vec<Objective>,
}

pub enum Objective {
    Accuracy { weight: f64 },
    PromptTokens { weight: f64, direction: Direction },
    CompletionTokens { weight: f64, direction: Direction },
    Latency { weight: f64, direction: Direction },
    CustomCheck { name: String, weight: f64 },
}

impl ParetoFrontier {
    pub fn dominates(a: &CandidateScores, b: &CandidateScores, objectives: &[Objective]) -> bool;
    pub fn add(&mut self, candidate_idx: usize, scores: &CandidateScores);
    pub fn frontier(&self) -> &[usize];
    pub fn best_weighted(&self, all_candidates: &[Candidate]) -> Option<usize>;
}
```

### Phase 7: Storage & Checkpointing

**New module: `engine/baml-runtime/src/optimize/storage.rs`**

```rust
pub struct OptimizationStorage {
    run_dir: PathBuf,
}

impl OptimizationStorage {
    pub fn new(base_dir: &Path) -> Result<Self>;
    pub fn from_existing(run_dir: &Path) -> Result<Self>;

    // Config
    pub fn save_config(&self, config: &OptimizationConfig) -> Result<()>;
    pub fn load_config(&self) -> Result<OptimizationConfig>;

    // Candidates (as .baml files)
    pub fn save_candidate(&self, candidate: &Candidate) -> Result<()>;
    pub fn load_candidates(&self) -> Result<Vec<Candidate>>;

    // Evaluations (JSON)
    pub fn save_evaluation(&self, id: usize, scores: &CandidateScores) -> Result<()>;

    // Reflections (JSON)
    pub fn save_reflection(&self, iteration: usize, examples: &[ReflectiveExample]) -> Result<()>;

    // State (JSON, not pickle!)
    pub fn save_state(&self, state: &OptimizationState) -> Result<()>;
    pub fn load_state(&self) -> Result<Option<OptimizationState>>;

    // Results
    pub fn save_results(&self, frontier: &ParetoFrontier, candidates: &[Candidate]) -> Result<()>;
}
```

**State format** (JSON per BEP):
```json
{
  "version": "1.0",
  "baml_cli_version": "0.73.0",
  "iteration": 15,
  "total_evals": 450,
  "budget_remaining": 550,
  "rng_seed": 42,
  "pareto_frontier_indices": [3, 7, 12, 15],
  "candidate_lineage": {...},
  "normalization_stats": {...}
}
```

### Phase 8: Main Orchestration Loop

**In `engine/baml-runtime/src/cli/optimize.rs`:**

```rust
pub struct GEPAOrchestrator {
    user_runtime: Arc<BamlRuntime>,
    gepa_runtime: GEPARuntime,
    evaluator: Evaluator,
    storage: OptimizationStorage,
    pareto: ParetoFrontier,
    candidates: Vec<Candidate>,
    config: OptimizationConfig,
}

impl GEPAOrchestrator {
    pub async fn run(&mut self) -> Result<OptimizationResult> {
        // 1. Initialize with current prompt/schema as candidate 0
        self.initialize_candidate()?;

        // 2. Evaluate initial candidate
        self.evaluate_candidate(0).await?;

        // 3. Main GEPA loop
        for iteration in 0..self.config.trials {
            // a. Select candidate(s) from Pareto frontier
            let selected = self.pareto.select_for_reflection();

            // b. Collect failures from selected candidates
            let failures = self.collect_failures(&selected);

            // c. Call GEPA ProposeImprovements
            let improved = self.gepa_runtime.propose_improvements(
                &self.candidates[selected[0]].function,
                &failures,
                None,
            ).await?;

            // d. Create new candidate from improved function
            let new_candidate = self.create_candidate(improved, &selected)?;
            let new_idx = self.candidates.len();
            self.candidates.push(new_candidate);

            // e. Evaluate new candidate
            self.evaluate_candidate(new_idx).await?;

            // f. Update Pareto frontier
            self.pareto.add(new_idx, &self.candidates[new_idx].scores.unwrap());

            // g. Checkpoint
            self.storage.save_state(&self.current_state())?;
            self.storage.save_candidate(&self.candidates[new_idx])?;

            // h. Check convergence / budget
            if self.should_stop() {
                break;
            }
        }

        // 4. Output results
        self.finalize()
    }
}
```

## File Structure Summary

```
engine/
├── cli/src/commands.rs                         # MODIFY: Add Optimize command
├── baml-runtime/
│   ├── Cargo.toml                              # MODIFY: Add sha2 for hashing
│   └── src/
│       ├── cli/
│       │   ├── mod.rs                          # MODIFY: Add optimize module
│       │   └── optimize.rs                     # NEW: CLI args & orchestrator
│       └── optimize/                           # NEW MODULE
│           ├── mod.rs
│           ├── gepa_runtime.rs                 # Load & call GEPA BAML
│           ├── schema_extractor.rs             # Extract types for optimization
│           ├── candidate.rs                    # Candidate data structures
│           ├── candidate_applier.rs            # Apply changes to runtime
│           ├── evaluator.rs                    # Run tests, collect failures
│           ├── pareto.rs                       # Multi-objective frontier
│           ├── storage.rs                      # JSON persistence
│           └── gepa_defaults/                  # Embedded defaults
│               ├── mod.rs                      # include_str! macros
│               ├── gepa.baml                   # Default GEPA functions
│               └── clients.baml                # Default ReflectionModel
```

## Implementation Order

1. **CLI Scaffold** - `baml-cli optimize --help` works
2. **Embedded GEPA BAML** - Default gepa.baml/clients.baml files
3. **GEPA Runtime Setup** - Create `.baml_optimize/gepa/` on first run, load runtime
4. **Schema Extractor** - Extract `OptimizableFunction` from IR
5. **Basic Evaluator** - Run tests, compute pass rate
6. **Storage Layer** - Save/load candidates and state as JSON
7. **Candidate Applier** - Apply candidate changes to create modified runtime
8. **Single Iteration** - One round of evaluate → reflect → propose
9. **Full GEPA Loop** - Multiple iterations with checkpointing
10. **Pareto Frontier** - Multi-objective optimization
11. **Resume Capability** - Load from checkpoint, version warnings
12. **UX Polish** - Progress reporting, `--reset-gepa-prompts`, diffs

## Key Design Decisions

### 1. GEPA as BAML (per BEP)
The GEPA reflection logic is BAML code that users can customize. This:
- Dogfoods BAML
- Makes optimization transparent
- Allows advanced users to tune reflection behavior

### 2. Schema + Prompt Optimization Together
GEPA's `ProposeImprovements` receives the full schema context and can modify:
- Prompt text
- Field `@description` annotations
- Field `@alias` annotations
- Class-level descriptions

This enables holistic optimization where prompt and schema work together.

### 3. JSON State (not pickle)
All checkpoints are JSON for:
- Language-agnostic resumability
- Human readability
- Git-friendliness
- Security (no code execution)

### 4. Candidate Application Strategy
**Recommended: IR Modification**

We'll need to:
1. Clone the IR
2. Find the target function and modify `prompt_template`
3. Find referenced classes/enums and modify their attributes
4. Create a new runtime with the modified IR

This requires adding helper methods to work with IR, but avoids filesystem overhead.

## Testing Strategy

1. **Unit tests** for schema extraction, Pareto frontier
2. **Integration tests** for GEPA runtime (mock LLM responses)
3. **E2E test** with a simple function and tests, verifying:
   - Candidate generation
   - Score improvement over iterations
   - Checkpoint/resume
   - Output artifacts

## Open Questions for Implementation

1. **IR Mutability**: Can we efficiently clone and modify `IntermediateRepr`? Need to check if types are `Clone`.

2. **GEPA Runtime Isolation**: Should the GEPA runtime share env vars with user runtime? Probably yes for API keys.

3. **Parallel Candidate Evaluation**: Should we evaluate multiple candidates in parallel? Start sequential for simplicity.

4. **Failure Sampling**: How many failures to sample for reflection? BEP says default 3.
