//! Tests for network operations that require Rust-side infrastructure.
//!
//! These tests need host-created TCP/UDP servers (to write raw bytes to
//! accepted connections or provide UDP echo peers with a known address,
//! since `baml.net` lacks `local_addr`) and insta bytecode snapshots.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use tokio::{io::AsyncWriteExt, net::TcpListener};

/// Replace the dynamic address in bytecode with a stable placeholder.
fn stabilize_bytecode(bytecode: &str, addr: &str) -> String {
    bytecode.replace(addr, "{ADDR}")
}

#[tokio::test]
async fn net_connect_and_read() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        socket.write_all(b"Hello from server!").await.unwrap();
        socket.shutdown().await.unwrap();
    });

    let output = baml_test!(&format!(
        r#"
            function main() -> uint8array {{
                let sock = baml.net.TcpStream.connect("{addr}");
                sock.read()
            }}
        "#
    ));
    server.await.unwrap();

    insta::assert_snapshot!(stabilize_bytecode(&output.bytecode, &addr), @r#"
    function main() -> uint8array {
        load_const "{ADDR}"
        load_const <omitted>
        call baml.net.TcpStream.connect
        load_const <omitted>
        call baml.net.TcpStream.read
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Uint8Array(b"Hello from server!".to_vec()))
    );
}

#[tokio::test]
async fn net_connect_failure() {
    let output = baml_test!(
        r#"
            function main() -> uint8array {
                let sock = baml.net.TcpStream.connect("127.0.0.1:1");
                sock.read()
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> uint8array {
        load_const "127.0.0.1:1"
        load_const <omitted>
        call baml.net.TcpStream.connect
        load_const <omitted>
        call baml.net.TcpStream.read
        return
    }
    "#);
    // Error message contains OS error code which differs across platforms
    // (111 on Linux, 61 on macOS).
    assert!(output.result.is_err());
}

#[tokio::test]
async fn net_multiple_reads() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        socket.write_all(b"chunk1").await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        socket.write_all(b"chunk2").await.unwrap();
        socket.shutdown().await.unwrap();
    });

    let output = baml_test!(&format!(
        r#"
            function main() -> uint8array {{
                let sock = baml.net.TcpStream.connect("{addr}");
                let first = sock.read();
                let second = sock.read();
                first
            }}
        "#
    ));
    server.await.unwrap();

    insta::assert_snapshot!(stabilize_bytecode(&output.bytecode, &addr), @r#"
    function main() -> uint8array {
        load_const "{ADDR}"
        load_const <omitted>
        call baml.net.TcpStream.connect
        store_var sock
        load_var sock
        load_const <omitted>
        call baml.net.TcpStream.read
        store_var first
        load_var sock
        load_const <omitted>
        call baml.net.TcpStream.read
        store_var second
        load_var first
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Uint8Array(b"chunk1".to_vec()))
    );
}

// Kept in Rust (not the baml_src corpus): the silent peer must be a
// controlled Rust listener that accepts and holds the connection open. A
// corpus version using `baml.http.Server.bind` as the silent peer is flaky —
// the BAML listener object can be GC-collected between `connect` and the
// throwing `read`, resetting the connection into an `Io` error instead of the
// expected `Timeout`, intermittently under load.
#[tokio::test]
async fn net_read_timeout_fires() {
    // The server accepts the connection but never writes, so a bare read() would
    // block forever. A short read timeout must surface as baml.errors.Timeout.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let _conn = listener.accept().await.unwrap();
        // Hold the connection open (silent) past the client's deadline.
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    });

    let output = baml_test!(&format!(
        r#"
            function main() -> uint8array {{
                let sock = baml.net.TcpStream.connect("{addr}");
                sock.read(timeout = baml.time.Duration.from_milliseconds(50n))
            }}
        "#
    ));
    server.await.unwrap();

    let err = output
        .result
        .expect_err("read with a 50ms timeout against a silent peer should time out")
        .to_string();
    assert!(
        err.contains("baml.errors.Timeout"),
        "expected a baml.errors.Timeout throw, got: {err}"
    );
}

#[tokio::test]
async fn net_read_succeeds_within_timeout() {
    // A generous read timeout must not interfere with a prompt response.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        socket.write_all(b"prompt").await.unwrap();
        socket.shutdown().await.unwrap();
    });

    let output = baml_test!(&format!(
        r#"
            function main() -> uint8array {{
                let sock = baml.net.TcpStream.connect("{addr}");
                sock.read(timeout = baml.time.Duration.from_seconds(5n))
            }}
        "#
    ));
    server.await.unwrap();

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Uint8Array(b"prompt".to_vec()))
    );
}

#[tokio::test]
async fn net_connect_timeout_param_accepted() {
    // Exercises the connect(timeout=…) path end-to-end: a generous deadline
    // against a live listener still connects and reads normally.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        socket.write_all(b"connected").await.unwrap();
        socket.shutdown().await.unwrap();
    });

    let output = baml_test!(&format!(
        r#"
            function main() -> uint8array {{
                let sock = baml.net.TcpStream.connect(
                    "{addr}",
                    timeout = baml.time.Duration.from_seconds(5n),
                );
                sock.read()
            }}
        "#
    ));
    server.await.unwrap();

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Uint8Array(b"connected".to_vec()))
    );
}

#[tokio::test]
async fn net_udp_send_recv() {
    // A peer that echoes back whatever datagram it receives, to the sender.
    let peer = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let peer_addr = peer.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let mut buf = vec![0u8; 1024];
        let (n, from) = peer.recv_from(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"ping");
        peer.send_to(b"pong", from).await.unwrap();
    });

    let output = baml_test!(&format!(
        r#"
            function main() -> uint8array {{
                let sock = baml.net.UdpSocket.bind("127.0.0.1:0");
                sock.send_to("ping".to_utf8(), "{peer_addr}");
                let dgram = sock.recv_from();
                dgram.data
            }}
        "#
    ));
    server.await.unwrap();

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Uint8Array(b"pong".to_vec()))
    );
}

#[tokio::test]
async fn net_udp_recv_succeeds_within_timeout() {
    // A generous recv timeout must not interfere with a prompt datagram.
    let peer = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let peer_addr = peer.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let mut buf = vec![0u8; 1024];
        let (n, from) = peer.recv_from(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"ping");
        peer.send_to(b"pong", from).await.unwrap();
    });

    let output = baml_test!(&format!(
        r#"
            function main() -> uint8array {{
                let sock = baml.net.UdpSocket.bind("127.0.0.1:0");
                sock.send_to("ping".to_utf8(), "{peer_addr}", timeout = baml.time.Duration.from_seconds(5n));
                let dgram = sock.recv_from(timeout = baml.time.Duration.from_seconds(5n));
                dgram.data
            }}
        "#
    ));
    server.await.unwrap();

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Uint8Array(b"pong".to_vec()))
    );
}
