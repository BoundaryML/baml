// Test hover on network builtins

function TestNetConnect() -> uint8array? {
    let s = baml.net<[CURSOR].TcpStream.connect("127.0.0.1:8080");
    s.read(4096)
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at hover_builtin_net.baml:4:21
// No hover content
