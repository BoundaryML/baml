# BEP-005: Prompt Optimization Design

## Summary

 - Add prompt optimization to BAML via a new `baml-cli optimize`
   command
 - Use DSPy's GEPA algorithm
 - Optimize the prompt text, @description, and @alias data
 - Users define accuracy criteria using existing BAML tests with assertions
 - Also allow optimizing other outcomes: fewer input tokens, fewer
   response tokens, latency. @@check establishes new outcomes
 - Stores optimization state to enable resumable runs and provides detailed
   artifacts including all candidate prompts and their performance metrics.

## Motivation

Copy the graet things about DSPy. **The people demand prompt optimization!**

BAML already has the key components needed for automated optimization:
- **Functions with prompts** as the optimization target
- **Tests with assertions** as the success metric

## Scope: What Gets Optimized

### In Scope (Phase 1)
- **Function prompts**: The `prompt #"..."#` text
- **Class field descriptions**: `@description("...")` annotations
- **Class field aliases**: `@alias(...)` annotations  
- **Enum value descriptions**: Comments on enum values
- **Class descriptions**: Class-level `@description`

All optimized together as a cohesive system. Changes to schema descriptions/aliases can improve parsing without requiring test updates.

### Out of Scope
- **Structural changes**: Adding/removing fields, changing types (would break tests)
- **Multi-function workflows**: Optimizing multiple functions together (Phase 4+)
- **Few-shot examples**: Optimizing example selection (Phase 2+)

## Proposed Design

### High-Level Architecture

The optimization system follows GEPA's evolutionary approach:

1. **Initialization**: Start with the current prompt in each BAML function
2. **Evaluation**: Run tests to measure prompt performance (test pass rate + optional metrics)
3. **Reflection**: Analyze test failures to understand what went wrong
4. **Proposal**: Generate new prompt variations using LLM-based reflection on failures
5. **Selection**: Maintain a Pareto frontier of candidates balancing multiple objectives
6. **Iteration**: Repeat steps 2-5 until budget exhausted or convergence

Key differences from DSPy's GEPA:
- Uses BAML tests (with `@@assert` and `@@check`) instead of custom metrics
- Optimizes BAML prompt templates (with Jinja2) instead of Python strings
- Stores state in BAML-native formats alongside the codebase

### Command-Line Interface

```bash
# Basic usage - optimize all functions with tests
baml-cli optimize

# Optimize specific function(s)
baml-cli optimize --function ExtractReceipt --function ClassifyEmail

# Optimize with test filtering
baml-cli optimize --test "ExtractReceipt::*"

# Control optimization budget
baml-cli optimize --max-evals 50              # Total function evaluations
baml-cli optimize --trials 20                 # Optimization iterations

# Auto-sized optimization budgets
baml-cli optimize --auto light    # Quick exploration (6 candidates)
baml-cli optimize --auto medium   # Balanced (12 candidates)
baml-cli optimize --auto heavy    # Thorough (18 candidates)

# Multi-objective optimization
baml-cli optimize --weight accuracy=0.8,tokens=0.2
baml-cli optimize --weight accuracy=0.7,latency=0.2,prompt_tokens=0.1
baml-cli optimize --weight accuracy=0.9,completion_tokens=0.1

# Resume previous optimization run
baml-cli optimize --resume .baml_optimize/run_20250106_143022

# Reset GEPA reflection prompts to defaults
baml-cli optimize --reset-gepa-prompts

# Control parallelism
baml-cli optimize --parallel 8

# Output and logging
baml-cli optimize --output-dir .baml_optimize/custom_run
baml-cli optimize --verbose
```

### Syntax

The optimization system reuses existing BAML syntax with no new language features required:

```baml
// Existing BAML function - the prompt will be optimized
function ExtractReceipt(image: image) -> Receipt {
  client GPT4o
  prompt #"
    Extract structured receipt information from this image.
    
    Return the merchant name, date, items, and total.
  "#
}

// Existing BAML tests - these define the optimization objective
test ReceiptTest1 {
  functions [ExtractReceipt]
  args {
    image { file "test_receipts/starbucks.jpg" }
  }
  // Assertions are the success criteria
  @@assert({{ this.merchant == "Starbucks" }})
  @@assert({{ this.total > 0 }})
  @@check(correct_items, {{ this.items|length == 2 }})
}

test ReceiptTest2 {
  functions [ExtractReceipt]
  args {
    image { file "test_receipts/target.jpg" }
  }
  @@assert({{ this.merchant == "Target" }})
  @@assert({{ this.total == 45.67 }})
}

// Example: Custom checks for multi-objective optimization (Phase 3)
test ReceiptWithGroundedness {
  functions [ExtractReceipt]
  args {
    image { file "test_receipts/complex.jpg" }
  }
  @@assert({{ this.merchant != "" }})
  // Custom checks can be weighted in optimization
  @@check(groundedness, {{ this.confidence > 0.8 }})
  @@check(safety, {{ this.contains_no_pii }})
}
```

### BAML-Driven GEPA Reflection

A key design principle: **GEPA's reflection logic is implemented in BAML itself**. This makes the optimization process transparent, customizable, and dogfoods BAML for optimizing BAML.

#### gepa.baml Location and Versioning

GEPA reflection functions live in `.baml_optimize/gepa/baml_src/`:

```
.baml_optimize/
└── gepa/
    └── baml_src/
        ├── gepa.baml          # Reflection functions
        ├── clients.baml       # Client configs
        └── .gepa_version      # Tracks baml-cli version
```

**First run behavior:**
```bash
$ baml-cli optimize

Creating .baml_optimize/gepa/baml_src/ with defaults from baml-cli 0.73.0...
Using reflection model: gpt-4o (default)
```

**Customization workflow:**
```bash
# User modifies reflection logic
$ vim .baml_optimize/gepa/baml_src/gepa.baml

# Or changes the reflection model
$ vim .baml_optimize/gepa/baml_src/clients.baml

# Next run uses custom GEPA implementation
$ baml-cli optimize
```

**Version tracking:**

The `.gepa_version` file contains:
```json
{
  "baml_cli_version": "0.73.0",
  "created_at": "2025-01-06T14:30:22Z",
  "gepa_baml_hash": "a3f5c9d..."
}
```

Modifications are detected by comparing file hash to embedded default. On version mismatch:
```bash
$ baml-cli --version
baml-cli 0.74.0

$ baml-cli optimize

Warning: Your GEPA implementation is from baml-cli 0.73.0
         Run 'baml-cli optimize --reset-gepa-prompts' to upgrade
```

#### Default gepa.baml Implementation

The default `gepa.baml` embedded in `baml-cli` includes:

**Data Models:**
```baml
class SchemaFieldDefinition {
  field_name string
  field_type string
  description string?
  aliases string[]
  is_optional bool
}

class ClassDefinition {
  class_name string
  description string?
  fields SchemaFieldDefinition[]
}

class EnumDefinition {
  enum_name string
  values string[]
  value_descriptions map<string, string>
}

class OptimizableFunction {
  function_name string
  prompt_text string
  classes ClassDefinition[]  // All reachable classes
  enums EnumDefinition[]     // All reachable enums
}

class ReflectiveExample {
  inputs map<string, string>
  generated_outputs map<string, string>
  feedback string
  failure_location string?  // "prompt" | "parsing" | "schema"
}

class ImprovedFunction {
  prompt_text string
  classes ClassDefinition[]  // Only modified classes
  enums EnumDefinition[]     // Only modified enums
  rationale string
}
```

**Core Reflection Function:**
```baml
function ProposeImprovements(
  current_function: OptimizableFunction,
  failed_examples: ReflectiveExample[],
  successful_examples: ReflectiveExample[]?
) -> ImprovedFunction {
  client ReflectionModel
  prompt #"
    You are optimizing a BAML function. Improve both the prompt and schema.
    
    ## Current Implementation
    
    Prompt:
    ```
    {{ current_function.prompt_text }}
    ```
    
    Schema:
    {% for class in current_function.classes %}
    class {{ class.class_name }} {
      {% for field in class.fields %}
      /// @description("{{ field.description or 'none' }}")
      {{ field.field_name }} {{ field.field_type }}{% if field.aliases %} @alias({{ field.aliases | join(", ") }}){% endif %}
      {% endfor %}
    }
    {% endfor %}
    
    ## Failures
    {% for ex in failed_examples %}
    Inputs: {{ ex.inputs }}
    Generated: {{ ex.generated_outputs }}
    Issue: {{ ex.feedback }}
    {% endfor %}
    
    ## Your Task
    
    Analyze failures and propose improvements to:
    1. Prompt text - clarity, instructions, examples
    2. Field descriptions - guide LLM parsing
    3. Field aliases - catch output variations
    
    Consider: Do the prompt and schema work well together?
    
    Return improvements as ImprovedFunction JSON.
  "#
}

function MergeVariants(
  variant_a: OptimizableFunction,
  variant_b: OptimizableFunction,
  variant_a_strengths: string[],
  variant_b_strengths: string[]
) -> ImprovedFunction {
  client ReflectionModel
  prompt #"
    Merge two successful BAML function variants.
    Combine their strengths into a single improved version.
    
    [Details omitted for brevity]
  "#
}
```

**Default clients.baml:**
```baml
client<llm> ReflectionModel {
  provider openai
  options {
    model "gpt-4o"
    temperature 1.0
    max_tokens 8000
  }
}
```

#### Schema Optimization Examples

**Example 1: Adding aliases based on failures**

Before:
```baml
class Receipt {
  /// @description("Merchant name")
  merchant string
}
```

After reflection on failures where LLM outputs "store_name":
```baml
class Receipt {
  /// @description("Merchant name exactly as shown on receipt")
  merchant string @alias("store_name", "shop_name", "vendor")
}
```

**Example 2: Improving descriptions**

Before:
```baml
class Receipt {
  /// @description("Total")
  total float
}
```

After reflection on failures with parsing errors:
```baml
class Receipt {
  /// @description("Total amount in decimal format (e.g., 12.99, not '$12.99')")
  total float @alias("amount", "total_amount")
}
```

**Example 3: Coordinated prompt and schema improvements**

GEPA optimizes prompt and schema together, ensuring they work cohesively:

```baml
// Before
function ExtractReceipt(image: image) -> Receipt {
  prompt #"Extract receipt information"#
}

class Receipt {
  merchant string
  total float
}

// After GEPA optimization
function ExtractReceipt(image: image) -> Receipt {
  prompt #"
    Extract structured receipt data:
    - merchant: exact name as printed
    - total: decimal amount (e.g., 45.67)
  "#
}

class Receipt {
  /// @description("Merchant name preserving exact capitalization")
  merchant string @alias("store_name", "vendor")
  
  /// @description("Total in decimal format, no currency symbols")
  total float @alias("amount", "sum")
}
```

### Semantics

#### Test-Based Objective Function

The optimization objective is computed from BAML test results:

1. **Primary metric: Test pass rate**
   - Each test case yields a binary pass/fail based on `@@assert` statements
   - First failing `@@assert` stops evaluation of remaining assertions
   - Pass rate = (passed tests) / (total tests)
   - This is always the primary component of the objective

2. **Secondary metrics (optional weights)**
   
   Like DSPy GEPA, BAML supports multi-objective optimization with the following metrics:
   
   - **`tokens`**: Minimize total tokens (prompt + completion). Useful for reducing API costs.
   - **`latency`**: Minimize inference latency (milliseconds). Useful for real-time applications.
   - **`prompt_tokens`**: Minimize prompt tokens specifically. Useful when optimizing prompt length.
   - **`completion_tokens`**: Minimize completion tokens. Useful for controlling output verbosity.
   - **Custom metrics via `@@check`**: User-defined checks can be weighted (Phase 3 feature)
     - `groundedness`: For RAG applications, measure citation quality
     - `safety`: Domain-specific safety constraints
     - `compliance`: Regulatory or policy compliance checks


#### Optimization State Storage

The optimizer stores artifacts in `baml_src/../.baml_optimize/run_<timestamp>/`:

```
.baml_optimize/
└── run_20250106_143022/
    ├── config.json                    # Optimization parameters
    ├── candidates/
    │   ├── 00_initial.baml            # Initial prompts
    │   ├── 01_candidate.baml          # Generated variations
    │   ├── 02_candidate.baml
    │   └── ...
    ├── evaluations/
    │   ├── 00_initial.json            # Test results per candidate
    │   ├── 01_candidate.json
    │   └── ...
    ├── reflections/
    │   ├── iteration_01.json          # Failure analysis
    │   └── ...
    ├── state.json                     # Resumable optimization state
    ├── pareto_frontier.json           # Current best candidates
    └── final_results.json             # Summary statistics
```

**State Format (JSON, not pickle):**

All optimization state is stored in human-readable JSON format for language-agnostic resumability:

```json
{
  "version": "1.0",
  "baml_cli_version": "0.73.0",
  "iteration": 15,
  "total_evals": 450,
  "budget_remaining": 550,
  "rng_seed": 42,
  "pareto_frontier_indices": [3, 7, 12, 15],
  "candidate_lineage": {
    "0": {"parents": null, "method": "initial"},
    "1": {"parents": [0], "method": "reflection"},
    "2": {"parents": [0, 1], "method": "merge"}
  },
  "normalization_stats": {
    "tokens": {"mean": 1500, "std": 300, "min": 800, "max": 2500},
    "latency": {"mean": 1200, "std": 200, "min": 800, "max": 1800}
  }
}
```

This replaces pickle files, ensuring:
- Language-agnostic: Works across Python, Rust, TypeScript implementations
- Human-readable: Can inspect/debug state manually
- Git-friendly: Can diff checkpoints
- Secure: No code execution risk

#### Candidate BAML File Format

Each candidate file contains only the optimized functions:

```baml
// Generated candidate #5
// Iteration: 3
// Parent candidates: [2, 4]
// Score: 0.85 (accuracy=0.90, tokens=-0.05)

function ExtractReceipt(image: image) -> Receipt {
  client GPT4o
  prompt #"
    Carefully analyze the receipt image and extract:
    1. Merchant name (exactly as shown)
    2. Purchase date (in ISO format)
    3. Line items with prices
    4. Total amount
    
    Pay special attention to currency formatting.
  "#
}
```

#### Reflection and Proposal

The reflection phase analyzes test failures to guide prompt evolution:

1. **Collect failure data**:
   - For each failed test, capture: inputs, outputs, assertions that failed
   - Sample a minibatch of failures (default: 3) to avoid overwhelming the reflection model

2. **Generate reflective dataset**:
   ```json
   {
     "function": "ExtractReceipt",
     "examples": [
       {
         "inputs": {"image": "test_receipts/starbucks.jpg"},
         "outputs": {"merchant": "STARBUCKS", "total": 8.50},
         "feedback": "Assertion failed: this.merchant == 'Starbucks'. The merchant name should match the expected casing exactly."
       },
       {
         "inputs": {"image": "test_receipts/target.jpg"},
         "outputs": {"merchant": "Target", "total": 45.0},
         "feedback": "Assertion failed: this.total == 45.67. The total is incorrect, possibly due to missing cents."
       }
     ]
   }
   ```

3. **Propose new prompt**:
   - Use reflection LLM (e.g., GPT-4o, Claude Sonnet) to analyze failure patterns
   - Prompt template (simplified from GEPA's InstructionProposalSignature):
     ```
     You are optimizing a BAML prompt. Here is the current prompt:
     
     <current_prompt>
     {current_prompt_text}
     </current_prompt>
     
     Here are examples where the prompt failed:
     
     <failures>
     {reflective_dataset}
     </failures>
     
     Based on these failures, propose an improved version of the prompt that:
     1. Addresses the specific issues shown in the failures
     2. Maintains the overall structure and intent
     3. Is clear and concise
     
     New prompt:
     ```

4. **Merge successful variants** (optional):
   - When multiple candidates perform well on different test subsets
   - Use reflection LLM to synthesize a combined prompt
   - Helps escape local optima by combining diverse successful strategies

#### Pareto Frontier Selection

When optimizing multiple objectives, maintain a Pareto frontier:

1. A candidate A dominates B if A is better on at least one objective and no worse on all others
2. The Pareto frontier is the set of non-dominated candidates
3. When selecting candidates for reflection, sample from the frontier (rather than always using the single "best")
4. Final output presents the entire frontier, letting users choose their preferred trade-off

### Integration with Existing BAML Features

#### Function Variants and Clients

The optimizer respects BAML's existing client configuration:

```baml
function ExtractReceipt(image: image) -> Receipt {
  client GPT4o
  prompt #"..."#
}

// The optimizer will use GPT4o for all evaluations
// To optimize for a different model, create a variant:

function ExtractReceipt(image: image) -> Receipt {
  client GPT4oMini  // Changed client
  prompt #"..."#
}
```

Users can optimize separately for different models by using function variants or by modifying the client between optimization runs.

#### Dynamic Types

If a function uses `@@dynamic` types, tests can override type definitions:

```baml
class Receipt {
  merchant string
  total float
  @@dynamic
}

test ReceiptWithCustomFields {
  functions [ExtractReceipt]
  args {
    image { file "test_receipts/custom.jpg" }
  }
  type_builder {
    dynamic Receipt {
      merchant string
      total float
      loyalty_number string  // Additional field for this test
    }
  }
  @@assert({{ this.loyalty_number != "" }})
}
```

The optimizer will respect these test-specific type extensions when evaluating candidates.

#### Expression language

This proposal should be orthogonal to expression language as possible,
so we can ship it ASAP.

When we ship expression language, we get two benefits:

  1. More interesting checks and asserts - e.g. checks and asserts
     could load data from external sources and make their own LLM
     calls, so that we could test RAG groundedness directly.
     
  2. **Cool Alert** 😎. We can optimize expression functions. Write
     a test block for expression function, `baml-cli optimize`
     will build a pareto frontier for the full workflow by optimizing
     every LLM call made in the full workflow and evaluating the
     Workflow's own test checks & asserts. I think DSPy does something
     like this too.

### Backwards Compatibility

Fully backward compatible.

## Open Questions

1. **Reflection Model Selection**: Should we default to a specific reflection model, or require users to specify one?
   - **Proposal**: Default to `gpt-4o` with opt-in to others via `--reflection-model`


2. **Automatic Prompt Updates**: Should the optimizer automatically update BAML source files, or just recommend changes?
   - **Proposal**: Never auto-update source. Instead, generate a diff/patch that users can review and apply

3. **Validation**: How do we ensure optimized prompts don't break type safety or introduce security issues?

## References

- [GEPA Paper: Reflective Prompt Evolution Can Outperform Reinforcement Learning](https://arxiv.org/abs/2507.19457)
- [DSPy GEPA Implementation](https://github.com/stanfordnlp/dspy/tree/main/dspy/teleprompt/gepa)
- [MIPRO Optimizer (DSPy)](https://github.com/stanfordnlp/dspy/blob/main/dspy/teleprompt/mipro_optimizer_v2.py)
- [BAML Test Documentation](https://docs.boundaryml.com/guide/baml-basics/testing-functions)
