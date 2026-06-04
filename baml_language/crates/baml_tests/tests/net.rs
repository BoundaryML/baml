//! Unified tests for network operations.

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
        sys_op baml.net.TcpStream.connect
        sys_op baml.net.TcpStream.read
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
        sys_op baml.net.TcpStream.connect
        sys_op baml.net.TcpStream.read
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
        sys_op baml.net.TcpStream.connect
        store_var sock
        load_var sock
        sys_op baml.net.TcpStream.read
        store_var first
        load_var sock
        sys_op baml.net.TcpStream.read
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
