client<llm> Bar {
  provider baml-openai-chat
}

function InputNone() -> int {
  client Bar
  prompt #"fa"#
}

function InputSimple(a: string, b: int | bool) -> int {
  client Bar
  prompt #"fa"#
}

class Email {
    subject string
    body string
}

function InputObject(email: Email) -> string {
  client Bar
  prompt #" subject line {{ email.subject }} "#
}

enum Color {
  RED
  BLUE
  GREEN
}

function InputEnum(color: Color) -> string {
  client Bar
  prompt #" color {{ color }} "#
}

// Try it with wierd spacing and comments

function InputEnum2(
  color: Color,
  bar: string, // ok

  // This param is great!
  foo: string[],

  // But we should do something...
  baz: (string | int?)[]
)
// Err
->
// Do something
string

// Or else
{
  client Bar
  prompt #" color {{ color }} "#
}


function InputImage(img: image) -> string {
  client Bar
  prompt #" image {{ img }} "#
}

// Test enum-string comparisons
function TestEnumStringEquality(color: Color) -> string {
  client Bar
  prompt #"
    Testing enum-string comparisons:

    {% if color == "RED" %}
      Color is RED (comparing to value name)
    {% endif %}

    {% if "BLUE" == color %}
      Color is BLUE (reverse comparison)
    {% endif %}

    {% if color != "GREEN" %}
      Color is not GREEN
    {% endif %}

    // Test ordering
    {% if color < "YELLOW" %}
      Color comes before YELLOW alphabetically
    {% endif %}

    {% if color >= "BLUE" %}
      Color is BLUE or comes after it
    {% endif %}

    // Display enum directly
    The color is: {{ color }}
  "#
}

// Test enum comparisons in complex expressions
function TestEnumInExpressions(color: Color) -> string {
  client Bar
  prompt #"
    {% set is_primary = color == "RED" or color == "BLUE" %}
    {% if is_primary %}
      This is a primary color
    {% endif %}

    // Ternary with enum comparison
    Result: {{ "Primary" if color == "RED" or color == "BLUE" else "Secondary" }}

    // Multiple comparisons
    {% if color == "RED" and true %}
      Color is RED and condition is true
    {% endif %}

    // Nested conditions
    {% if color != "GREEN" %}
      {% if color == "RED" %}
        It's RED
      {% elif color == "BLUE" %}
        It's BLUE
      {% endif %}
    {% endif %}
  "#
}

// warning: Use `Color.RED` instead of "RED" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:71
//    | 
// 70 | 
// 71 |     {% if color == "RED" %}
//    | 
// warning: Use `Color.BLUE` instead of "BLUE" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:75
//    | 
// 74 | 
// 75 |     {% if "BLUE" == color %}
//    | 
// warning: Use `Color.GREEN` instead of "GREEN" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:79
//    | 
// 78 | 
// 79 |     {% if color != "GREEN" %}
//    | 
// warning: Use one of: Color.RED, Color.BLUE, Color.GREEN - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:84
//    | 
// 83 |     // Test ordering
// 84 |     {% if color < "YELLOW" %}
//    | 
// warning: Use `Color.BLUE` instead of "BLUE" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:88
//    | 
// 87 | 
// 88 |     {% if color >= "BLUE" %}
//    | 
// warning: Use `Color.RED` instead of "RED" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:101
//    | 
// 100 |   prompt #"
// 101 |     {% set is_primary = color == "RED" or color == "BLUE" %}
//    | 
// warning: Use `Color.BLUE` instead of "BLUE" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:101
//    | 
// 100 |   prompt #"
// 101 |     {% set is_primary = color == "RED" or color == "BLUE" %}
//    | 
// warning: Use `Color.RED` instead of "RED" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:107
//    | 
// 106 |     // Ternary with enum comparison
// 107 |     Result: {{ "Primary" if color == "RED" or color == "BLUE" else "Secondary" }}
//    | 
// warning: Use `Color.BLUE` instead of "BLUE" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:107
//    | 
// 106 |     // Ternary with enum comparison
// 107 |     Result: {{ "Primary" if color == "RED" or color == "BLUE" else "Secondary" }}
//    | 
// warning: Use `Color.RED` instead of "RED" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:110
//    | 
// 109 |     // Multiple comparisons
// 110 |     {% if color == "RED" and true %}
//    | 
// warning: Use `Color.GREEN` instead of "GREEN" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:115
//    | 
// 114 |     // Nested conditions
// 115 |     {% if color != "GREEN" %}
//    | 
// warning: Use `Color.RED` instead of "RED" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:116
//    | 
// 115 |     {% if color != "GREEN" %}
// 116 |       {% if color == "RED" %}
//    | 
// warning: Use `Color.BLUE` instead of "BLUE" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:118
//    | 
// 117 |         It's RED
// 118 |       {% elif color == "BLUE" %}
//    | 
