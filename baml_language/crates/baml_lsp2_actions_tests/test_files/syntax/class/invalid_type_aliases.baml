class One {
  f int
}

// Already existing name.
type One = int

// Unexpected keyword.
typpe Two = float

// Unknown identifier.
type Three = i

// Unknown identifier in union.
type Four = int | string | b

// error: Error validating: Unexpected keyword used in assignment: typpe
//   -->  class/invalid_type_aliases.baml:9
//    |
//  8 | // Unexpected keyword.
//  9 | typpe Two = float
//    |
// error: The class "One" cannot be re-defined because a type_alias with that name already exists.
//   -->  class/invalid_type_aliases.baml:1
//    |
//    |
//  1 | class One {
//    |
//   -->  class/invalid_type_aliases.baml:6
//    |
//  5 | // Already existing name.
//  6 | reflect.Type One = int
//    |
// error: The type_alias "One" cannot be re-defined because a class with that name already exists.
//   -->  class/invalid_type_aliases.baml:6
//    |
//  5 | // Already existing name.
//  6 | reflect.Type One = int
//    |
//   -->  class/invalid_type_aliases.baml:1
//    |
//    |
//  1 | class One {
//    |
// error: Error validating: Type alias points to unknown identifier `i`
//   -->  class/invalid_type_aliases.baml:12
//    |
// 11 | // Unknown identifier.
// 12 | reflect.Type Three = i
//    |
// error: Error validating: Type alias points to unknown identifier `b`
//   -->  class/invalid_type_aliases.baml:15
//    |
// 14 | // Unknown identifier in union.
// 15 | reflect.Type Four = int | string | b
//    |

//----
//- diagnostics
// E0010
//
//   × invalid syntax
//    ╭─[class_invalid_type_aliases.baml:9:1]
//  9 │ typpe Two = float
//    · ──┬──
//    ·   ╰── unknown keyword `typpe`; did you mean `type`? usage: `type Name = expression`
//    ╰────
