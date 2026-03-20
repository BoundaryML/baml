// Test hover on File type and methods

function TestFileRead() -> string {
    let f<[CURSOR] = baml.fs.open("example.txt");
    f.read()
}

//----
//- diagnostics
// Error: unresolved name: baml
//    ╭─[ hover_builtin_fs_file.baml:4:12 ]
//    │
//  4 │     let f = baml.fs.open("example.txt");
//    │            ──────┬──────  
//    │                  ╰──────── unresolved name: baml
//    │ 
//    │ Note: Error code: E0001
// ───╯
//
//- on_hover expressions
// hover at hover_builtin_fs_file.baml:4:10
// ```baml
// f: unknown
// ```
