client<llm> Bar {
  provider openai
}

function InputNone() -> int {
  client Bar
  prompt `fa`
}

function InputSimple(a: string, b: int | bool) -> int {
  client Bar
  prompt `fa`
}

class Email {
    subject string
    body string
}

function InputObject(email: Email) -> string {
  client Bar
  prompt ` subject line ${email.subject} `
}

enum Color {
  RED
  BLUE
  GREEN
}

function InputEnum(color: Color) -> string {
  client Bar
  prompt ` color ${color} `
}

// Try it with wierd spacing && comments

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
  prompt ` color ${color} `
}


function InputImage(img: image) -> string {
  client Bar
  prompt ` image ${img} `
}

// Test enum-string comparisons
function TestEnumStringEquality(color: Color) -> string {
  client Bar
  prompt `
    Testing enum-string comparisons:

    ${if (color == Color.RED)}
      Color is RED (comparing to value name)
    ${endif}

    ${if (Color.BLUE == color)}
      Color is BLUE (reverse comparison)
    ${endif}

    ${if (color != Color.GREEN)}
      Color is not GREEN
    ${endif}

    // Test ordering
    ${if (color < Color.YELLOW)}
      Color comes before YELLOW alphabetically
    ${endif}

    ${if (color >= Color.BLUE)}
      Color is BLUE || comes after it
    ${endif}

    // Display enum directly
    The color is: ${color}
  `
}

// Test enum comparisons in complex expressions
function TestEnumInExpressions(color: Color) -> string {
  client Bar
  prompt `
    ${if (color == Color.RED || color == Color.BLUE)}
      This is a primary color
    ${endif}

    // Ternary with enum comparison
    Result: ${if (color == Color.RED || color == Color.BLUE) { "Primary" } else { "Secondary" }}

    // Multiple comparisons
    ${if (color == Color.RED && true)}
      Color is RED && condition is true
    ${endif}

    // Nested conditions
    ${if (color != Color.GREEN)}
      ${if (color == Color.RED)}
        It's RED
      ${else if (color == Color.BLUE)}
        It's BLUE
      ${endif}
    ${endif}
  `
}

// warning: Use `Color.RED` instead of "RED" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:71
//    |
// 70 |
// 71 |     ${if (color == Color.RED)}
//    |
// warning: Use `Color.BLUE` instead of "BLUE" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:75
//    |
// 74 |
// 75 |     ${if (Color.BLUE == color)}
//    |
// warning: Use `Color.GREEN` instead of "GREEN" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:79
//    |
// 78 |
// 79 |     ${if (color != Color.GREEN)}
//    |
// warning: Use one of: Color.RED, Color.BLUE, Color.GREEN - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:84
//    |
// 83 |     // Test ordering
// 84 |     ${if (color < Color.YELLOW)}
//    |
// warning: Use `Color.BLUE` instead of "BLUE" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:88
//    |
// 87 |
// 88 |     ${if (color >= Color.BLUE)}
//    |
// warning: Use `Color.RED` instead of "RED" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:101
//    |
// 100 |   prompt `
// 101 |     ${ let is_primary = color == Color.RED || color == Color.BLUE }
//    |
// warning: Use `Color.BLUE` instead of "BLUE" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:101
//    |
// 100 |   prompt `
// 101 |     ${ let is_primary = color == Color.RED || color == Color.BLUE }
//    |
// warning: Use `Color.RED` instead of "RED" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:107
//    |
// 106 |     // Ternary with enum comparison
// 107 |     Result: ${if (color == Color.RED || color == Color.BLUE) { "Primary" } else { "Secondary" }}
//    |
// warning: Use `Color.BLUE` instead of "BLUE" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:107
//    |
// 106 |     // Ternary with enum comparison
// 107 |     Result: ${if (color == Color.RED || color == Color.BLUE) { "Primary" } else { "Secondary" }}
//    |
// warning: Use `Color.RED` instead of "RED" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:110
//    |
// 109 |     // Multiple comparisons
// 110 |     ${if (color == Color.RED && true)}
//    |
// warning: Use `Color.GREEN` instead of "GREEN" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:115
//    |
// 114 |     // Nested conditions
// 115 |     ${if (color != Color.GREEN)}
//    |
// warning: Use `Color.RED` instead of "RED" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:116
//    |
// 115 |     ${if (color != Color.GREEN)}
// 116 |     ${if (color == Color.RED)}
//    |
// warning: Use `Color.BLUE` instead of "BLUE" - comparing enums with strings will soon be deprecated.
//   -->  functions_v2/valid.baml:118
//    |
// 117 |         It's RED
// 118 |       ${else if (color == Color.BLUE)}
//    |

//----
//- diagnostics
// E0007
//
//   × type `Color` has no member `YELLOW`
//     ╭─[functions_v2_valid.baml:84:25]
//  84 │     ${if (color < Color.YELLOW)}
//     ·                         ──────
//     ╰────
//   ╰─▶   ☞ enum defined here
//           ╭─[functions_v2_valid.baml:25:6]
//        25 │ enum Color {
//           ·      ─────
//           ╰────
// E0004
//
//   × `Color` does not implement `Compare`, so it cannot be ordered with `>=`
//     ╭─[functions_v2_valid.baml:88:11]
//  88 │     ${if (color >= Color.BLUE)}
//     ·           ───────────────────
//     ╰────
// E0007
//
//   × type `Color` has no member `YELLOW`
//     ╭─[functions_v2_valid.baml:84:25]
//  84 │     ${if (color < Color.YELLOW)}
//     ·                         ──────
//     ╰────
//   ╰─▶   ☞ enum defined here
//           ╭─[functions_v2_valid.baml:25:6]
//        25 │ enum Color {
//           ·      ─────
//           ╰────
// E0004
//
//   × `Color` does not implement `Compare`, so it cannot be ordered with `>=`
//     ╭─[functions_v2_valid.baml:88:11]
//  88 │     ${if (color >= Color.BLUE)}
//     ·           ───────────────────
//     ╰────
// E0007
//
//   × type `Color` has no member `YELLOW`
//     ╭─[functions_v2_valid.baml:84:25]
//  84 │     ${if (color < Color.YELLOW)}
//     ·                         ──────
//     ╰────
//   ╰─▶   ☞ enum defined here
//           ╭─[functions_v2_valid.baml:25:6]
//        25 │ enum Color {
//           ·      ─────
//           ╰────
// E0004
//
//   × `Color` does not implement `Compare`, so it cannot be ordered with `>=`
//     ╭─[functions_v2_valid.baml:88:11]
//  88 │     ${if (color >= Color.BLUE)}
//     ·           ───────────────────
//     ╰────
// E0007
//
//   × type `Color` has no member `YELLOW`
//     ╭─[functions_v2_valid.baml:84:25]
//  84 │     ${if (color < Color.YELLOW)}
//     ·                         ──────
//     ╰────
//   ╰─▶   ☞ enum defined here
//           ╭─[functions_v2_valid.baml:25:6]
//        25 │ enum Color {
//           ·      ─────
//           ╰────
// E0004
//
//   × `Color` does not implement `Compare`, so it cannot be ordered with `>=`
//     ╭─[functions_v2_valid.baml:88:11]
//  88 │     ${if (color >= Color.BLUE)}
//     ·           ───────────────────
//     ╰────
