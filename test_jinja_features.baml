// Comprehensive test of Jinja template parsing features

enum Category {
  Technical
  Sales
  Support
}

class User {
  name string
  email string
  premium bool
}

function TestAllJinjaFeatures(user: User, items: string[], category: Category) -> string {
  client GPT4
  prompt #"
    {{ _.role("system") }}
    You are a helpful AI assistant.

    {{ _.role("user") }}
    Hello {{ user.name }}!

    {# User information section #}
    Email: {{ user.email }}
    Account Type: {% if user.premium %}Premium{% else %}Free{% endif %}

    {# Display items if any #}
    {% if items %}
      Your items:
      {% for item in items %}
        {{ loop.index }}. {{ item }}
      {% endfor %}
    {% else %}
      No items to display.
    {% endif %}

    Category: {{ category }}

    {# Output format directive #}
    {{ ctx.output_format }}

    Additional notes:
    - This demonstrates variable interpolation
    - Control flow with if/else
    - Loops with item access
    - Comments that are stripped
    - Special BAML constructs
  "#
}

function TestFilters(text: string, numbers: int[]) -> string {
  client GPT4
  prompt #"
    Original: {{ text }}
    Uppercase: {{ text|upper }}
    Lowercase: {{ text|lower }}
    Length: {{ text|length }}

    Numbers: {{ numbers }}
    Sum: {{ numbers|sum }}
    Max: {{ numbers|max }}
    Min: {{ numbers|min }}
  "#
}

function TestNestedExpressions(data: string) -> string {
  client GPT4
  prompt #"
    {# Complex nested expression #}
    Result: {{ data|length > 10 }}

    {% if data|length > 10 %}
      Long data: {{ data|upper }}
    {% else %}
      Short data: {{ data|lower }}
    {% endif %}
  "#
}
