---
id: BEP-005
title: "Prompt Optimization"
shepherds: Greg Hale <imalsogreg@gmail.com>
status: Accepted
created: 2025-12-06
---

# BEP-005: Prompt Optimization

Comments are tracked on [a slack thread](https://gloo-global.slack.com/archives/C0A1GKH2M53/p1765220143370529).

## Summary

  - `baml-cli` supports a new command called `optimize` that writes
    and improves prompts for you, similar to [DSPy](https://dspy.ai/).
  - Prompt optimization attempts to maximize the number of passing BAML
    test cases, and optionally minimize tokens and latency.
  - The optimizer is based on the
    [GEPA algorithm](https://arxiv.org/abs/2507.19457), which is
    partially encoded as BAML functions that you can tweak.

## Motivation

Prompt Optimization is becoming an attractive way to add reliability to
LLM interactions. BAML's type-based interface to LLMs still requires
the user to write and maintain a prompt string, and optionally to
add descriptions and aliases to custom classes. The new `optimize`
command available in `baml-cli` automatically searches for prompts
that maximize passing tests and optimize things like token count,
latency, and custom properties.

## Basic Optimization

Start with any existing BAML project, or create a new one with
`baml-cli init`. For our examples, we will assume a project with
a class, a function, and a test like this:

```BAML
// example.baml
class Person {
  name string
  age int?
}

function ExtractSubject(sentence: string) -> Person? {
  client "anthropic/claude-haiku-3-5"
  prompt #"
    Extract the subject of the sentence {{ sentence }}.
    {{ ctx.output_format }}
  "#
}

test IndirectionTest {
  functions [ExtractSubject]
  args {
    sentence "Meg gave Pam a dog for her 30th birthday. She was 21.
  }
  @@assert({{ this != null }})
  @@assert( {{ this.name == Pam }})
  @@assert( {{ this.age != null }})
  @@assert( {{ this.age == 21 }})
}
```

This prompt could be difficult for the LLM to satisfy.
To optimize it, kick off an optimization job:

```sh
baml-cli optimize --beta --apply
```

This command should be run from your project root - usually the directory
containing `baml_src`. If you have multiple `baml_src` directories in
your project, run the `optimize` command from the directory that contains
the `baml_src` you are trying to optimize.

The `--beta` flag tells BAML to enable experimental features,
and will be required until we stabilize the optimizer, some
time after we gather initial user feedback.

The `--apply` flag tells BAML to overwrite the current prompt
with the best prompt identified by the optimization.

After optimization is finished, you can continue with a
normal BAML workflow. For example, running `baml-cli generate`
and using your BAML functions in client code just as before.

## Writing Evals

The key to achieving an optimized prompt is to provide
many realistic examples and test assertions.

Choose input values that represent common inputs you will
see in production, as well as inputs that represent corner
cases, like empty lists, malformatted strings and prompt
injection attacks.

## Customizing the optimization

The default behavior of the optimizer is to use the standard
GEPA algorithm, using GPT-4 to search the optimization space,
and to maximize the number of passing test cases. This
workflow can be customized in several ways.

### Optimizing for tokens and latency

You can give optimization weight to the number of input
tokens, response tokens, and response latency, using the
`--weights` flag.

```sh
# Weigh accuracy and token efficiency equally.
baml-cli optimize --beta --weight accuracy=0.5,tokens=0.5

# Consider latency in the weighting
baml-cli optimize --beta --weight accuracy=0.4,latency=0.6
```

Weights don't necessarily need to add up to 1.0, even though
it's customary to write them that way. They will be normalized
to sum to 1 by the optimizer.

### Creating custom metrics

You can define your own metrics by using `@@check`s instead of
`@@assert`s. Checks with the same name are grouped together
and treated as one metric.

In our original example, we can imagine being more interested
in identifying the subject of a sentence, and only partially
interested in linking that identity to a correct age. We can
achieve this by using checks:

```
test Test1 {
  functions [ExtractSubject]
  args {
    sentence "Meg gave Pam a dog for her 30th birthday. She was 21.
  }
  @@assert({{ this != null }})
  @@assert( {{ this.name == Pam }})
  @@check(correct_age, {{ this.age == 21 }})
}
```

```sh
baml-cli optimize --beta --weight accuracy=0.8,correct_age=0.2
```

### Customizing the optimization backend model

The GEPA model powering prompt optimization is controlled by
BAML functions. These functions are installed into your
environment when you first run prompt optimization. If you
want to edit them before optimization starts, you can create
first, edit them, and then run optimization:

```sh
# Create the gepa.baml files
baml-cli optimize --beta --reset-gepa-prompts

# ... edit .baml_optimize/gepa/baml_src/gepa.baml ...

# Run optimization with the customized gepa.baml
baml-cli optimize --beta
```

There are two safe ways to modify `gepa.baml`:

  1. Change the `client` field to use a model other
     than `openai/gpt-4`. For example, `anthropic/claude-opus-4-5`.
  2. Add text to the prompts for the functions named
     `ProposeImprovements`, `MergeVariants`, or
     `AnalyzeFailurePatterns`. These three functions constitute
     the LLM part of the GEPA algorithm.

It is not safe to change the classes or to delete text in `gepa.baml`
because the internal part of the implementation is not customizable,
and it expects the types to be in their current shape.

`gepa.baml` is defined like this:

```
// GEPA: Genetic Pareto
// =============================================================================
// Data Models
// =============================================================================

/// Represents a field in a class schema that can be optimized
class SchemaFieldDefinition {
    field_name string
    field_type string
    description string?
    alias string?
    is_optional bool
}

/// Represents a class definition with its fields
class ClassDefinition {
    class_name string
    description string?
    fields SchemaFieldDefinition[]
}

/// The complete optimizable context for a function
class OptimizableFunction {
    function_name string
    prompt_text string
    classes ClassDefinition[]
    enums EnumDefinition[]
    function_source string?  // The full BAML source code of the function
}

/// An example from test execution showing inputs, outputs, and feedback
class ReflectiveExample {
    inputs map<string, string>
    generated_outputs map<string, string>
    feedback string
    failure_location string?  // "prompt" | "parsing" | "assertion" | "unknown"
    test_source string?  // The BAML source code of the test (including assertions)
    test_name string?
}

/// The result of reflection: improved prompt and schema
class ImprovedFunction {
    prompt_text string
    classes ClassDefinition[]
    enums EnumDefinition[]
    rationale string
}

// =============================================================================
// Reflection Functions
// =============================================================================


/// Analyze test failures and propose improvements to prompt and schema
function ProposeImprovements(current_function: OptimizableFunction, failed_examples: ReflectiveExample[], successful_examples: ReflectiveExample[]?) -> ImprovedFunction {
    client ReflectionModel
    prompt ##"
        You are an expert at optimizing BAML functions. Your task is to improve
        both the prompt and the schema annotations to make the tests pass.

        ## Current Implementation

        Function: {{ current_function.function_name }}

        {% if current_function.function_source %}
        ### Full BAML Source Code
        ```baml
        {{ current_function.function_source }}
        ```
        {% endif %}

        ### Prompt Template
        ```
        {{ current_function.prompt_text }}
        ```

        ### Schema
        {% for class in current_function.classes %}
        class {{ class.class_name }}{% if class.description %} // {{ class.description }}{% endif %} {
        {% for field in class.fields %}
            {% if field.description %}/// @description("{{ field.description }}")
            {% endif %}{{ field.field_name }} {{ field.field_type }}{% if field.alias %} @alias({{ field.alias }}){% endif %}

        {% endfor %}
        }
        {% endfor %}


        {% for ex in failed_examples %}
        ### Failure {{ loop.index }}{% if ex.test_name %}: {{ ex.test_name }}{% endif %}

        **Test Inputs:** {{ ex.inputs | tojson }}
        **LLM Generated Output:** {{ ex.generated_outputs | tojson }}

        {% endfor %}

        ## Your Task

        **IMPORTANT:** The test source code shows the assertions that must pass.
        Look at the test's assert/check statements to understand what output is expected.
        If the prompt contains instructions that contradict what the tests expect,
        those instructions are BUGS that need to be fixed.

        Analyze the failures and propose improvements. Consider:

        1. **Prompt bugs:**
           - Does the prompt contain instructions that cause wrong outputs?
    ...
```

In summary, it synthesizes data about the existing BAML types, prompts, and
test results in order to generate new proposed types and prompts.

## Optimization Timing

GEPA optimization requires making many LLM calls. It uses LLMs
to run your original prompt, generate new prompts, analyze errors,
combine prompts together and run new prompts. Optimizing a prompt
can take several minutes to complete.

You can control the timing in a few ways.

```sh
# Limit the number of iterations through the full algorithm
baml-cli optimize --beta --trials 10

# Limit the absolute number of tests to run
baml-cli optimize --beta --max-evals 50

# Limit optimization to a single BAML function
baml-cli optimize --beta --function ExtractSubject

# Filter to specific tests
baml-cli optimize --beta --test "*::IndirectionTest"
```

Optimization runs generally start fresh from your own single
prompt. However, you can resume a prior optimization run,
adding more candidate prompts, using `--resume` followed by
the name of some prior run. Prior runs are tracked in the
optimization state directory, `.baml_optimize`.

```
# Resume an existing run
baml-cli optimize --beta --resume .baml_optimize/run_20251208_150606
```

## Understanding the Optimization Algorithm

The sections above are enough to get started. But an understanding
of the GEPA algorithm can be helpful for getting better results
from optimization.

GEPA stands for Genetic Pareto, meaning that it proceeds by tracking
the current Pareto frontier, and combining prompts from the frontier
to try to extend the frontier. A Pareto frontier prompt is any prompt
that is not strictly worse than any other prompt, in the various ways
that prompts can be good or bad. For the simple case where only the
number of failing tests matters, the Pareto frontier is simply the
single prompt with the least failures (or the set of all prompts with
the least failures, if there is a tie).

The Pareto frontier begins with only your original prompt, and the algorithm
proceeds in a loop until it reaches its exploration budget (maximum
number of trials or maximum number of evaluations).

 1. Evaluate and score the current Pareto frontier (it starts with the initial prompt)
 2. Propose prompt improvements by iterating on or combining prompts on the frontier
 3. Reflect on the improved prompts and score them
 4. Repeat

## Limitations

### Types

Optimization will modify your types' descriptions and aliases, but it will
not make other changes, such as renaming or adding fields. Modifying
your types would require you to modify any application code that uses your
generated BAML functions.

### Template strings

When optimization runs over your functions, it only looks for the classes
and enums already used by that function. The optimizer doesn't know how
to search for template_strings in your codebase that would be helpful
in the prompt.

### Error types

For the purpose of optimization, all failures are treated
equally. If a model is sporadically unavailable and the LLM provider 
returns 500, this can confuse the algorithm because it will appear
that the prompt is at fault for the failure.

## Future features

Based on user feedback, we are considering several improvements to
the prompt optimizer.

### Error robustness

We improve our error handling so that spurious errors don't penalize
a good prompt.

### Agentic features

In some circumstances it would be helpful for the reflection steps
to be able to run tools, such as fetching specific documentation or
rendering the prompt with its inputs to inspect it prior to calling
the LLM.


## References

GEPA: https://arxiv.org/abs/2507.19457
DSPy: https://dspy.ai/
