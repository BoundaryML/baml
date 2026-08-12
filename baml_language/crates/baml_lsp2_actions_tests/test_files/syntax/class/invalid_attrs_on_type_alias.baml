type DescNotAllowed = string @description("This is not allowed")

type AliasNotAllowed = float @alias("Alias not allowed")

type SkipNotAllowed = float @skip

type AttrNotFound = int @unknown

//----
//- diagnostics
// E0106
//
//   × `@description` is only allowed on class fields and enum variants; remove it here
//    ╭─[class_invalid_attrs_on_type_alias.baml:1:30]
//  1 │ type DescNotAllowed = string @description("This is not allowed")
//    ·                              ─────────────────┬─────────────────
//    ·                                               ╰── field attribute here
//    ╰────
// E0106
//
//   × `@alias` is only allowed on class fields and enum variants; remove it here
//    ╭─[class_invalid_attrs_on_type_alias.baml:3:30]
//  3 │ type AliasNotAllowed = float @alias("Alias not allowed")
//    ·                              ─────────────┬─────────────
//    ·                                           ╰── field attribute here
//    ╰────
// E0106
//
//   × `@skip` is only allowed on class fields and enum variants; remove it here
//    ╭─[class_invalid_attrs_on_type_alias.baml:5:29]
//  5 │ type SkipNotAllowed = float @skip
//    ·                             ──┬──
//    ·                               ╰── field attribute here
//    ╰────
// E0015
//
//   × unknown attribute `@unknown`
//    ╭─[class_invalid_attrs_on_type_alias.baml:7:25]
//  7 │ type AttrNotFound = int @unknown
//    ·                         ────┬───
//    ·                             ╰── unknown attribute
//    ╰────
