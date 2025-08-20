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


function InputImage(image: image) -> string {
  client Bar
  prompt #" image {{ image }} "#
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

// warning: Type mismatch: '(color == RED)' compares values of different types (enum Color and literal["RED"]). Starting in baml 0.206.0, strings are not implicitly converted to enum values (e.g. you should use `MyEnum.VALUE_A` instead of `"VALUE_A"`).
//   -->  functions_v2/valid.baml:101
//    | 
// 100 |   prompt #"
// 101 |     {% set is_primary = color == "RED" or color == "BLUE" %}
//    | 
// warning: Type mismatch: '(color == BLUE)' compares values of different types (enum Color and literal["BLUE"]). Starting in baml 0.206.0, strings are not implicitly converted to enum values (e.g. you should use `MyEnum.VALUE_A` instead of `"VALUE_A"`).
//   -->  functions_v2/valid.baml:101
//    | 
// 100 |   prompt #"
// 101 |     {% set is_primary = color == "RED" or color == "BLUE" %}
//    | 
// warning: Type mismatch: '(color == RED)' compares values of different types (enum Color and literal["RED"]). Starting in baml 0.206.0, strings are not implicitly converted to enum values (e.g. you should use `MyEnum.VALUE_A` instead of `"VALUE_A"`).
//   -->  functions_v2/valid.baml:107
//    | 
// 106 |     // Ternary with enum comparison
// 107 |     Result: {{ "Primary" if color == "RED" or color == "BLUE" else "Secondary" }}
//    | 
// warning: Type mismatch: '(color == BLUE)' compares values of different types (enum Color and literal["BLUE"]). Starting in baml 0.206.0, strings are not implicitly converted to enum values (e.g. you should use `MyEnum.VALUE_A` instead of `"VALUE_A"`).
//   -->  functions_v2/valid.baml:107
//    | 
// 106 |     // Ternary with enum comparison
// 107 |     Result: {{ "Primary" if color == "RED" or color == "BLUE" else "Secondary" }}
//    | 
