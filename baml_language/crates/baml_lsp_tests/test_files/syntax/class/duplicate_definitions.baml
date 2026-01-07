class A {
  field string
}

class A {
  field string
}

type A = int

// error: The class "A" cannot be re-defined because it is already defined as one of these: "class", "type_alias".
//   -->  class/duplicate_definitions.baml:1
//    | 
//    | 
//  1 | class A {
//    | 
//   -->  class/duplicate_definitions.baml:5
//    | 
//  4 | 
//  5 | class A {
//    | 
//   -->  class/duplicate_definitions.baml:9
//    | 
//  8 | 
//  9 | type A = int
//    | 
// error: The class "A" cannot be re-defined because it is already defined as one of these: "class", "type_alias".
//   -->  class/duplicate_definitions.baml:5
//    | 
//  4 | 
//  5 | class A {
//    | 
//   -->  class/duplicate_definitions.baml:1
//    | 
//    | 
//  1 | class A {
//    | 
//   -->  class/duplicate_definitions.baml:9
//    | 
//  8 | 
//  9 | type A = int
//    | 
// error: The type_alias "A" cannot be re-defined because it is already defined as a class.
//   -->  class/duplicate_definitions.baml:9
//    | 
//  8 | 
//  9 | type A = int
//    | 
//   -->  class/duplicate_definitions.baml:1
//    | 
//    | 
//  1 | class A {
//    | 
//   -->  class/duplicate_definitions.baml:5
//    | 
//  4 | 
//  5 | class A {
//    |

//----
//- diagnostics
// Error: Duplicate class `A`
//    ╭─[ class_duplicate_definitions.baml:1:1 ]
//    │
//  1 │ class A {
//    │ │ 
//    │ ╰─ class `A` redefined here
//    │ │ 
//    │ ╰─ `A` first defined in class_duplicate_definitions.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
// Error: Duplicate type alias `A`
//    ╭─[ class_duplicate_definitions.baml:1:1 ]
//    │
//  1 │ class A {
//    │ │ 
//    │ ╰─ type alias `A` redefined here
//    │ │ 
//    │ ╰─ `A` first defined in class_duplicate_definitions.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
