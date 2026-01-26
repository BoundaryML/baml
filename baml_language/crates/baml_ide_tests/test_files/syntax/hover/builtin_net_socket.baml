// Test hover on Socket type and methods

function TestSocketRead() -> string {
    let s<[CURSOR] = baml.net.connect("127.0.0.1:8080");
    s.read()
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at hover_builtin_net_socket.baml:4:10
// No hover content
