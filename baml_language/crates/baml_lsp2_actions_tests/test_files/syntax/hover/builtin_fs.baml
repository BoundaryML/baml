// Test hover on file system builtins

function TestFsOpen() -> string {
    let f = baml.fs<[CURSOR].open("example.txt");
    f.read()
}

//----
//- diagnostics
// Error: unresolved name: baml
//    ╭─[ hover_builtin_fs.baml:4:12 ]
//    │
//  4 │     let f = baml.fs.open("example.txt");
//    │            ──────┬──────  
//    │                  ╰──────── unresolved name: baml
//    │ 
//    │ Note: Error code: E0001
// ───╯
//
//- on_hover expressions
// hover at hover_builtin_fs.baml:4:20
// No hover content
