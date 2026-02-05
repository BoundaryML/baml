// Test hover on network builtins

function TestNetConnect() -> string {
    let s = baml.net<[CURSOR].connect("127.0.0.1:8080");
    s.read()
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at hover_builtin_net.baml:4:21
// No hover content
