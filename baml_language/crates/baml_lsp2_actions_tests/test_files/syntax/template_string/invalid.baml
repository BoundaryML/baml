function FunctionWithBadParams(
    param: Unknown,
    param2: Unknown2[],
    param3: string
) -> string {
    `${param.foo} ${param2[0].doc} ${param3}`
}

//----
//- diagnostics
// E0002
//
//   × unresolved type: Unknown
//    ╭─[template_string_invalid.baml:2:12]
//  2 │     param: Unknown,
//    ·            ───────
//    ╰────
// E0002
//
//   × unresolved type: Unknown2
//    ╭─[template_string_invalid.baml:3:13]
//  3 │     param2: Unknown2[],
//    ·             ──────────
//    ╰────
