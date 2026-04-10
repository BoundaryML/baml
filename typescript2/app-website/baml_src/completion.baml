

function Completion(prefix: string, suffix: string, language: string) -> string {
  client "openai/gpt-4o"
  prompt ##"
    {{ _.role("system", cache_control={"type": "ephemeral"}) }}

    You are a programmer that suggests code completions in the %INSERT-HERE% part below with  {{ language }} code. Only output the code that replaces %INSERT-HERE% part, NOT THE SUFFIX OR PREFIX. Respond only with code, and with no markdown formatting.

    Try to complete a whole section inside curlies when you can.

    {% if language == "baml" %}
    {{ BAMLBackground2()}}

    Examples:
    INPUT:
    ---
    class MyObject {{"{"}}%INSERT-HERE%
    }
    ---
    OUTPUT:
    ---
      property string
    ---
    In this example, we just inserted one line, with tabs for a fake property to aid the user.

    INPUT:
    ---
    function foo(input: string) -> string {{"{"}} %INSERT-HERE%
      prompt #"
        {{ "{{ input }}" }}
      "#
    }
    ---
    OUTPUT:
    ---
      client "openai/gpt-4o"
    ---
    In this example, no need to add the prompt because it was part of the suffix after INSERT-HERE

    INPUT:
    OUTPUT: N/A
    In this example there was nothing to complete, so we returned N/A.

    Ignore the "---" in your outputs.
    {% endif %}


    {{ _.role("user") }}
    INPUT:
    ---
    {{ prefix }}%INSERT-HERE%{{ suffix }}
    ---
  "##
}

function CompletionPython(prefix: string, suffix: string, language: string) -> string {
  client "openai/gpt-4o"
  prompt ##"
    {{ _.role("system", cache_control={"type": "ephemeral"}) }}

    You are a programmer that suggests code completions in the %INSERT-HERE% part below with  {{ language }} code. Only output the code that replaces %INSERT-HERE% part, NOT THE SUFFIX OR PREFIX. Respond only with code, and with no markdown formatting.

    Try to complete a whole section inside curlies when you can.



    Examples:
    INPUT:
    ---
    class MyClass:%INSERT-HERE%
        pass
    ---
    OUTPUT:
    ---
        def __init__(self):
            self.property = ""
    ---
    In this example, we just inserted one line, with spaces for a constructor with a property to aid the user.

    INPUT:
    ---
    def process_text(text: str) -> str:%INSERT-HERE%
        return text.upper()
    ---
    OUTPUT:
    ---
        """Process the input text and return transformed version"""
    ---
    In this example, we added a docstring before the function body

    INPUT:
    OUTPUT: N/A
    In this example there was nothing to complete, so we returned N/A.

    Remove the "---" in your outputs.

    {{ _.role("user") }}
    INPUT:
    ---
    {{ prefix }}%INSERT-HERE%{{ suffix }}
    ---
  "##
}

test TestName {
  functions [CompletionPython]
  args {
    prefix #"
      hello world
    "#
    suffix #"
      hello world
    "#
    language #"
      hello world
    "#
  }
}


function CompletionTypescript(prefix: string, suffix: string, language: string) -> string {
  client "openai/gpt-4o"
  prompt ##"
    {{ _.role("system", cache_control={"type": "ephemeral"}) }}

    You are a programmer that suggests code completions in the %INSERT-HERE% part below with  {{ language }} code. Only output the code that replaces %INSERT-HERE% part, NOT THE SUFFIX OR PREFIX. Respond only with code, and with no markdown formatting.

    Try to complete a whole section inside curlies when you can.


    Examples:
    INPUT:
    ---
    class MyClass {{"{"}}%INSERT-HERE%
      private name: string;
    }
    ---
    OUTPUT:
    ---
      constructor() {
        this.name = "";
      }
    ---
    In this example, we just inserted a constructor with a property initialization to aid the user.

    INPUT:
    ---
    function processText(text: string): string {{"{"}}%INSERT-HERE%
      return text.toUpperCase();
    }
    ---
    OUTPUT:
    ---
      // Process the input text and return transformed version
    ---
    In this example, we added a comment before the function body

    INPUT:
    OUTPUT: N/A
    In this example there was nothing to complete, so we returned N/A.

    Remove the "---" in your outputs.

    {{ _.role("user") }}
    INPUT:
    ---
    {{ prefix }}%INSERT-HERE%{{ suffix }}
    ---
  "##
}


test CompletionTest {
  functions [Completion]
  args {
    prefix "function foo(input: string) -> string {"
    suffix ""
    language "baml"
  }
}

test CompletionTest3 {
  functions [Completion]
  args {
    prefix ##"function foo(input: string) -> string {
      client "openai/gpt-4o"
      prompt #"
    "##
    suffix ""
    language "baml"
  }
}

test CompletionTest2 {
  functions [Completion]
  args {
    prefix "function foo(input: string) -> string {\n"
    suffix "\n  prompt #\n\""
    language "baml"
  }
}


template_string BAMLBackground2() ##"
  <Overview>
    BAML is a domain-specific language for building LLM prompts as functions.
    You can build an agentic workflow with BAML.
  </Overview>

  <Schema>
    // Define output schemas using classes
    class MyObject {
      // Optional string fields use ?
      // @description is optional, but if you include it, it goes after the field.
      name string? @description("The name of the object")

      // Arrays of primitives
      // arrays cannot be optional.
      tags string[]

      // Enums must be declared separately and are optional
      status MyEnum?

      // Union types
      type "success" | "error"

      // Primitive types
      count int
      enabled bool
      score float

      // nested objects
      nested MyObject2

      // image type
      myImg image

      // checks and assertions. Uses jinja syntax inside the parentheses.
      // For a single property use one @
      bar int @assert(between_0_and_10, {{ "{{ this > 0 and this < 10 }}" }}) //this = MyObject.bar value
      quux string
      // assertions for multiple fields use @@ and go at the bottom of the class. Uses jinja syntax inside the parentheses.
      // Do NOT add descriptions after the assertion.
      @@assert(length_limit, {{ "{{ this.quux|length < this.baz }}" }})
    }

    // Enums are declared separately
    enum MyEnum {
      PENDING
      ACTIVE @description("Item is currently active")
      COMPLETE
    }

    // Comments use double slashes
    // Recursive types and inline definitions are not supported
  </Schema>

  <Functions>
    // Functions define inputs, outputs and prompts
    // function name is always PascalCase
    function MyFunction(input: MyObject) -> string {
      client "openai/gpt-4o"
      // prompt with jinja syntax inside here. with double curly braces for variables.
      // make sure to include: {{ "{{ ctx.output_format }}"}} in the prompt, which prints the output schema instructions so the LLM returns the output in the correct format (json or string, etc.). DO NOT write the output schema manually.
      prompt #"

      "#
    }

    <LLMClients>
      You can use any of the following:
      - openai/gpt-4o
      - openai/gpt-4o-mini
      - anthropic/claude-3-5-sonnet-latest (note the "3-5")
      - anthropic/claude-3-5-haiku-latest
    </LLMClients>

    <Prompt>
      When writing the prompt:
      1. Make sure to include the input in the prompt (even if it's an image) using {{ "{{ input }}" }}
      2. Make sure to include {{ "{{ ctx.output_format }}" }} in the prompt so the LLM knows how to format the output.
      3. You do not need to specify to "answer in JSON format". Only write in the prompt brief instruction, and any other task-specific things to keep in mind for the task.
      4. Write a {{ "{{ _.role(\"user\") }}" }} tag to indicate where the user's inputs start. So if there's a convo you can write
      #"{{ "{{ _.role(\"user\") }}" }} {{ "{{ some-variable }}" }}#
    </Prompt>
  </Functions>

  The @asserts only go in the "output" types. Don't use them in inputs.
  Do NOT use numbers as confidence intervals if you need to use them. Prefer an enum with descriptions or literals like "high", "medium", "low".

  Dedent all declarations.
"##

template_string BamlTests() ##"
  You will generate them in the BAML syntax, which follows the following format:
  // Tests in BAML follow this format:
  // test TestName {
  //   functions [FunctionToTest]
  //   args {
  //     argumentName value
  //   }
  // }

  // For primitive inputs (string, int, float, bool):
  test SimpleTest {
    functions [MyFunction]
    args {
      input {
        name "Test Object"
        tags ["tag1", "tag2"]
        status ACTIVE
        type "success"
        count 42
        enabled true
        score 9.5
      }
    }
  }

  // For image inputs:
  test ImageTest {
    functions [MyFunction]
    args {
      imageArg {
        file "../images/test.png"
        // Optional: media_type "image/png"
      }
      // Or using URL:
      // imageArg {
      //   url "https://example.com/image.png"
      // }
    }
  }

  // For array/object inputs:
  test ComplexTest {
    functions [MyFunction]
    args {
      input {
        name "Complex Object"
        tags [
          "tag1",
          #"
            Multi-line
            tag here
          "#
        ]
        status PENDING
        type "error"
        count 100
        enabled false
        score 7.8
      }
    }
  }
"##
