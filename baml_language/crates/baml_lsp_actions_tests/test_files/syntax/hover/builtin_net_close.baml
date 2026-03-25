// Test hover on Socket.close method

function TestSocketClose() -> string {
    let s = baml.net.connect("127.0.0.1:8080");
    let data = s.read();
    s.close<[CURSOR]();
    data
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at hover_builtin_net_close.baml:6:12
// No hover content
