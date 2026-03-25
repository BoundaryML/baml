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
// <no-diagnostics-expected>
