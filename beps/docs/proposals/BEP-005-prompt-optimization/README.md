---
id: BEP-005
title: "Prompt Optimization"
shepherds: Greg Hale <imalsogreg@gmail.com>
status: Draft
created: 2025-12-06
---

# BEP-005: Prompt Optimization

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

## Out of scope - optimizing BAML types

Optimizing the types would require updating the test arguments, making it
hard to compare across versions of types. It's a little too much.

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

# Configure reflection model (for generating new prompts)
baml-cli optimize --reflection-model gpt-4o
baml-cli optimize --reflection-model claude-sonnet-4

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
    ├── checkpoints/
    │   ├── checkpoint_10.pkl          # Resumable state
    │   └── checkpoint_20.pkl
    ├── pareto_frontier.json           # Current best candidates
    └── final_results.json             # Summary statistics
```

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
@@dynamic
class Receipt {
  merchant string
  total float
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
