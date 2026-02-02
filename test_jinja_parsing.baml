// Test file for Jinja template parsing

function TestJinjaExpressions(input: string, name: string) -> string {
  client GPT4
  prompt #"
    Hello {{ name }}!

    You said: {{ input }}

    {{ ctx.output_format }}
  "#
}

function TestJinjaStatements(items: string[]) -> string {
  client GPT4
  prompt #"
    {% if items %}
      Here are your items:
      {% for item in items %}
        - {{ item }}
      {% endfor %}
    {% else %}
      No items provided.
    {% endif %}
  "#
}

function TestJinjaComments(user: string) -> string {
  client GPT4
  prompt #"
    {# This is a comment that should be stripped #}
    Hello {{ user }}!
    {# Another comment #}
    How can I help you today?
  "#
}

function TestMixedContent(value: int) -> string {
  client GPT4
  prompt #"
    Plain text here.
    {{ _.role("system") }}
    You are a helpful assistant.

    {{ _.role("user") }}
    The value is: {{ value }}

    {% if value > 10 %}
      That's a large value!
    {% endif %}
  "#
}
