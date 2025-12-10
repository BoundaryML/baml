// GEPA: Genetic Pareto
//
// This file contains the reflection functions used by `baml-cli optimize`
// to analyze test failures and propose improved prompts and schema annotations.
//
// You can customize these functions to change how optimization works.
// Run `baml-cli optimize --reset-gepa-prompts` to restore defaults.

// =============================================================================
// Data Models
// =============================================================================

/// Represents a field in a class schema that can be optimized
class SchemaFieldDefinition {
    field_name string
    field_type string
    description string?
    alias string?
}

/// Represents a class definition with its fields
class ClassDefinition {
    class_name string
    description string?
    fields SchemaFieldDefinition[]
}

/// Represents an enum definition with value descriptions
class EnumDefinition {
    enum_name string
    values string[]
    value_descriptions map<string, string>
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
// TODO: Include additional instructions about optimizing
//       for latency and token counts if the user requests it with --weight
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

        {% for enum in current_function.enums %}
        enum {{ enum.enum_name }} {
        {% for value in enum.values %}
            {{ value }}{% if enum.value_descriptions[value] %} // {{ enum.value_descriptions[value] }}{% endif %}

        {% endfor %}
        }
        {% endfor %}

        ## Test Failures ({{ failed_examples | length }} examples)

        {% for ex in failed_examples %}
        ### Failure {{ loop.index }}{% if ex.test_name %}: {{ ex.test_name }}{% endif %}

        {% if ex.test_source %}
        **Test Source Code (this shows what the test expects):**
        ```baml
        {{ ex.test_source }}
        ```
        {% endif %}

        **Test Inputs:** {{ ex.inputs | tojson }}
        **LLM Generated Output:** {{ ex.generated_outputs | tojson }}
        **Error:** {{ ex.feedback }}
        {% if ex.failure_location %}**Failure Type:** {{ ex.failure_location }}{% endif %}

        {% endfor %}

        {% if successful_examples %}
        ## Successful Examples (for reference)

        {% for ex in successful_examples %}
        - Inputs: {{ ex.inputs | tojson }} -> {{ ex.generated_outputs | tojson }}
        {% endfor %}
        {% endif %}

        ## Your Task

        **IMPORTANT:** The test source code shows the assertions that must pass.
        Look at the test's assert/check statements to understand what output is expected.
        If the prompt contains instructions that contradict what the tests expect,
        those instructions are BUGS that need to be fixed.

        Analyze the failures and propose improvements. Consider:

        1. **Jinja**
           - Prompts use jinja to inject function parameters.
           - There are special variables in the context for inserting other data.
           - Variables are inserted with \{\{ variable \}\} (double-curly brackets).
           - The ctx.output_format variable contains the schema and some instructions.
             You use that instead of saying "Respond in JSON with this schema... SCHEMA..."
           - _.role("system") and _.role("user") variables set the message block role.
           - ctx.output_format can take arguments:
             - ctx.output_format(quote_class_fields=true) renders classes with quotes around
               field names.
             - ctx.output_format(or_splitter=" X ") uses " X " to separate entries in unions
             - ctx.output_format(hoist_classes={"auto" | bool | field[]}) renders class
               definitions as classes, instead of as anonymous, possibly nested, JSON records.
               A list of names can be passed in order to isolate just some classes for
               hoisting.

        2. Attributes
           - descriptions are a good place to specify approximately how many items
             a list should have.
           - aliases are a good way to rename fields from the LLM's perspective.
             shorter aliases can save tokens, but may also confuse the LLM.

        3. **Prompt bugs:**
           - Does the prompt contain instructions that cause wrong outputs?
           - Are there hardcoded values that should be extracted from input instead?
           - Does the prompt tell the LLM to do something different from what tests expect?
           - Does the prompt use the jinja expression \{\{ctx.output_format\}\} (in double-curly brackets)
             to specify how to format outputs?

        4. **Prompt clarity:**
           - Is the instruction clear and specific?
           - Should examples be added or modified?
           - Are edge cases addressed?

        5. **Schema improvements:**
           - Should field @description annotations be added or improved?
           - Would @alias annotations help catch output variations?

        General tips:

          - Strongly prefer \{\{ctx.output_format\}\} (in double curly-brackets) over
            manually specifying output schemas.
          - If \{\{ctx.output_format\}\} is missing, this is likely the most important
            problem in the prompt.
          - Some models prefer to see \{\{ctx.output_format\}\} at the beginning of the
            prompt. Others do better with it at the end.

        Here is a small example of a BAML file with a good class and a function
        with a good prompt:
        ```example.baml
        class PhoneNumber {
          phone_number string @description("Phone number formatted like xxx-xxx-xxxx") @alias("phone")
        }

        function ExtractPhoneNumber(query: string) -> PhoneNumber {
          client GPT4o
          prompt #"
            \{\{ _.role("system") \}\}
            Extract the phone number.
            \{\{ ctx.output_format \}\}

            \{\{ _.role("user") \}\}
            User's query:
            \{\{ query \}\}
          "#
        }
        ```

        Note how it uses its function arguments in the prompt, and that we
        use ctx.output_format without preceding it by instructions like "use
        this format" (because those instructions are present already in
        ctx.output_format).

        Return your improvements. Fix any bugs in the prompt that cause test failures.
        Explain your reasoning in the rationale field.


        {{ ctx.output_format }}
    "##
}

/// Merge two successful variants into a combined improvement
function MergeVariants(variant_a: OptimizableFunction, variant_b: OptimizableFunction, variant_a_strengths: string[], variant_b_strengths: string[]) -> ImprovedFunction {
    client ReflectionModel
    prompt #"
        You are merging two successful BAML function variants into an improved version.

        ## Variant A: {{ variant_a.function_name }}

        Prompt:
        ```
        {{ variant_a.prompt_text }}
        ```

        Strengths: {{ variant_a_strengths | join(", ") }}

        ## Variant B: {{ variant_b.function_name }}

        Prompt:
        ```
        {{ variant_b.prompt_text }}
        ```

        Strengths: {{ variant_b_strengths | join(", ") }}

        ## Your Task

        Create a merged version that combines the best aspects of both variants.
        Consider:
        - What makes each variant successful?
        - How can their strengths be combined?
        - Are there any conflicts to resolve?

        Return the merged function with:
        - Combined prompt taking the best from both
        - Merged schema improvements
        - Explanation of your merge strategy

        {{ ctx.output_format }}
    "#
}

/// Analyze what types of failures are occurring
function AnalyzeFailurePatterns(function_name: string, failures: ReflectiveExample[]) -> FailureAnalysis {
    client ReflectionModel
    prompt #"
        Analyze the failure patterns for function {{ function_name }}.

        ## Failures

        {% for ex in failures %}
        ### Failure {{ loop.index }}
        Inputs: {{ ex.inputs | tojson }}
        Output: {{ ex.generated_outputs | tojson }}
        Issue: {{ ex.feedback }}
        {% endfor %}

        ## Your Task

        Categorize these failures and identify patterns:
        - Are failures due to prompt clarity issues?
        - Are they parsing/format problems?
        - Are they semantic understanding issues?
        - Are there common patterns across failures?

        {{ ctx.output_format }}
    "#
}

class FailureAnalysis {
    categories FailureCategory[]
    common_patterns string[]
    recommended_focus string
}

class FailureCategory {
    category string  // "prompt_clarity" | "parsing" | "semantic" | "edge_case" | "other"
    count int
    examples string[]
}
