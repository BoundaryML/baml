// Test network operations typecheck correctly

function ConnectAndRead() -> uint8array? {
    let sock = baml.net.TcpStream.connect("127.0.0.1:8080");
    sock.read(4096)
}

function ConnectAndReadInline() -> uint8array? {
    baml.net.TcpStream.connect("localhost:3000").read(4096)
}

function MultipleConnections() -> uint8array {
    let s1 = baml.net.TcpStream.connect("server1:80");
    let s2 = baml.net.TcpStream.connect("server2:80");
    let data1 = s1.read(4096) ?? b"";
    let data2 = s2.read(4096) ?? b"";
    data1.concat(data2)
}

function ConnectReadAndClose() -> uint8array? {
    let sock = baml.net.TcpStream.connect("127.0.0.1:8080");
    let data = sock.read(4096);
    sock.close();
    data
}

function ListenAndAccept() -> uint8array? {
    let listener = baml.net.TcpListener.bind("127.0.0.1:0");
    let sock = listener.accept();
    sock.read(4096)
}

function UdpRoundtrip() -> uint8array {
    let sock = baml.net.UdpSocket.bind("0.0.0.0:0");
    sock.send_to("ping".to_utf8(), "127.0.0.1:9");
    let dgram = sock.recv_from();
    dgram.data
}
