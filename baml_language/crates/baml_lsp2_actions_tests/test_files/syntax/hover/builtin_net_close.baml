// Test hover on Socket.close method

function TestSocketClose() -> string {
    let s = baml.net.connect("127.0.0.1:8080");
    let data = s.read();
    s.close<[CURSOR]();
    data
}

//----
//- diagnostics
// Error: unresolved name: baml
//    ╭─[ hover_builtin_net_close.baml:4:12 ]
//    │
//  4 │     let s = baml.net.connect("127.0.0.1:8080");
//    │            ────────┬────────  
//    │                    ╰────────── unresolved name: baml
//    │ 
//    │ Note: Error code: E0001
// ───╯
//
//- on_hover expressions
// hover at hover_builtin_net_close.baml:6:12
// No hover content
