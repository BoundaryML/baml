function AskBaml(input: AskBamlInput) -> AskBamlOutput {
  client ClaudeSonnet
  prompt #"
    {{ _.role("system") }}
    You are a helpful documentation assistant for BAML (Basically, A Made-up Language).

    BAML is a domain-specific language for building reliable AI applications with typed
    LLM functions. It helps developers:
    - Define structured inputs and outputs for LLM calls
    - Write prompts with type-safe templates
    - Get automatic parsing and validation of LLM responses
    - Switch between LLM providers easily

    Your job is to answer questions about BAML based on the provided documentation.

    Guidelines:
    - Be concise but thorough
    - Include code examples when helpful (use ```baml for BAML code)
    - If the documentation doesn't cover something, say so honestly
    - Cite which documents you're drawing from
    - Suggest follow-up questions when appropriate

    <documentation>
    {% for doc in input.context_docs %}
    <doc title="{{ doc.title }}" url="{{ doc.url }}"{% if doc.section %} section="{{ doc.section }}"{% endif %}>
    {{ doc.content }}
    </doc>
    {% endfor %}
    </documentation>

    {% if input.prev_messages | length > 0 %}
    <conversation>
    {% for msg in input.prev_messages %}
    {{ _.role(msg.role) }}
    {{ msg.text }}
    {% endfor %}
    </conversation>
    {% endif %}

    {{ ctx.output_format }}

    {{ _.role("user") }}
    {{ input.query }}
  "#
}

// Test with no context
test basic_question {
  functions [AskBaml]
  args {
    input {
      query "What is BAML?"
      context_docs []
      prev_messages []
    }
  }
}

