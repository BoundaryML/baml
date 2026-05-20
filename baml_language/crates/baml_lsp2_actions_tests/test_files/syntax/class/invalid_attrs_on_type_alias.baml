type DescNotAllowed = string @description("This is not allowed")

type AliasNotAllowed = float @alias("Alias not allowed")

type SkipNotAllowed = float @skip

type AttrNotFound = int @assert({{ this > 0 }}) @unknown

// error: Error validating: type aliases may only have @check and @assert attributes
//   -->  class/invalid_attrs_on_type_alias.baml:1
//    |
//    |
//  1 | type DescNotAllowed = string @description("This is not allowed")
//    |
// error: Error validating: type aliases may only have @check and @assert attributes
//   -->  class/invalid_attrs_on_type_alias.baml:3
//    |
//  2 |
//  3 | type AliasNotAllowed = float @alias("Alias not allowed")
//    |
// error: Error validating: type aliases may only have @check and @assert attributes
//   -->  class/invalid_attrs_on_type_alias.baml:5
//    |
//  4 |
//  5 | type SkipNotAllowed = float @skip
//    |
// error: Attribute not known: "@unknown".
//   -->  class/invalid_attrs_on_type_alias.baml:7
//    |
//  6 |
//  7 | type AttrNotFound = int @assert({{ this > 0 }}) @unknown
//    |

//----
//- diagnostics
// error: `@description` is only allowed on class fields and enum variants; remove it here
//    ╭─[ class_invalid_attrs_on_type_alias.baml:1:30 ]
//    │
//  1 │ type DescNotAllowed = string @description("This is not allowed")
//    │                              ─────────────────┬─────────────────  
//    │                                               ╰─────────────────── field attribute here
//    │ 
//    │ Note: Error code: E0106
// ───╯
// error: `@alias` is only allowed on class fields and enum variants; remove it here
//    ╭─[ class_invalid_attrs_on_type_alias.baml:3:30 ]
//    │
//  3 │ type AliasNotAllowed = float @alias("Alias not allowed")
//    │                              ─────────────┬─────────────  
//    │                                           ╰─────────────── field attribute here
//    │ 
//    │ Note: Error code: E0106
// ───╯
// error: `@skip` is only allowed on class fields and enum variants; remove it here
//    ╭─[ class_invalid_attrs_on_type_alias.baml:5:29 ]
//    │
//  5 │ type SkipNotAllowed = float @skip
//    │                             ──┬──  
//    │                               ╰──── field attribute here
//    │ 
//    │ Note: Error code: E0106
// ───╯
// error: Unknown attribute `@unknown`
//    ╭─[ class_invalid_attrs_on_type_alias.baml:7:49 ]
//    │
//  7 │ type AttrNotFound = int @assert({{ this > 0 }}) @unknown
//    │                                                 ────┬───  
//    │                                                     ╰───── unknown attribute
//    │ 
//    │ Note: Error code: E0015
// ───╯
