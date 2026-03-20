// Test hover on Socket type and methods

function TestSocketRead() -> string {
    let s<[CURSOR] = baml.net.connect("127.0.0.1:8080");
    s.read()
}

//----
//- diagnostics
// Error: unresolved name: baml
//    ╭─[ hover_builtin_net_socket.baml:4:12 ]
//    │
//  4 │     let s = baml.net.connect("127.0.0.1:8080");
//    │            ────────┬────────  
//    │                    ╰────────── unresolved name: baml
//    │ 
//    │ Note: Error code: E0001
// ───╯
//
//- on_hover expressions
// hover at hover_builtin_net_socket.baml:4:10
// ```baml
// s: unknown
// ```
