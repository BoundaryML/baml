// Test hover on File.close method

function TestFileClose() -> string {
    let f = baml.fs.open("example.txt");
    let content = f.read();
    f.close<[CURSOR]();
    content
}

//----
//- diagnostics
// Error: unresolved name: baml
//    ╭─[ hover_builtin_fs_close.baml:4:12 ]
//    │
//  4 │     let f = baml.fs.open("example.txt");
//    │            ──────┬──────  
//    │                  ╰──────── unresolved name: baml
//    │ 
//    │ Note: Error code: E0001
// ───╯
//
//- on_hover expressions
// hover at hover_builtin_fs_close.baml:6:12
// No hover content
