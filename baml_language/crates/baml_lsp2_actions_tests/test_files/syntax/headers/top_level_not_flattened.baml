//# Top Container
function TopLevelNotFlattened() -> string {
    //## Only Child
    "done"
}

//----
//- diagnostics
// E0010
//
//   × invalid syntax
//    ╭─[headers_top_level_not_flattened.baml:1:1]
//  1 │ //# Top Container
//    · ────────┬────────
//    ·         ╰── header comments (`//#`) are only allowed in expression functions
//    ╰────
