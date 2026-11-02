//! Tests for network operations that require Rust-side infrastructure.
//!
//! These tests need host-created TCP/UDP servers (to write raw bytes to
//! accepted connections or provide UDP echo peers with a known address,
//! since `baml.net` lacks `local_addr`) and insta bytecode snapshots.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

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
            function main() -> uint8array? {{
                let sock = baml.net.TcpStream.connect("{addr}");
                sock.read(1024)
            }}
        "#
    ));
    server.await.unwrap();

    insta::assert_snapshot!(stabilize_bytecode(&output.bytecode, &addr), @r#"
    function main() -> uint8array | null {
        load_const "{ADDR}"
        load_const <omitted>
        call baml.net.TcpStream.connect
        load_const 1024
        load_type baml.io.Read
        load_const "read"
        virtual_call nargs=2 ntypeargs=0 self_arg=0
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
            function main() -> uint8array? {
                let sock = baml.net.TcpStream.connect("127.0.0.1:1");
                sock.read(1024)
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> uint8array | null {
        load_const "127.0.0.1:1"
        load_const <omitted>
        call baml.net.TcpStream.connect
        load_const 1024
        load_type baml.io.Read
        load_const "read"
        virtual_call nargs=2 ntypeargs=0 self_arg=0
        return
    }
    "#);
    // Error message contains OS error code which differs across platforms
    // (111 on Linux, 61 on macOS).
    assert!(output.result.is_err());
}

#[tokio::test]
async fn net_write_accepts_text_and_writes_every_byte() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut data = Vec::new();
        socket.read_to_end(&mut data).await.unwrap();
        data
    });

    let output = baml_test!(&format!(
        r#"
            function main() -> int {{
                let sock = baml.net.TcpStream.connect("{addr}");
                let written = sock.write("hello");
                sock.close();
                written
            }}
        "#
    ));

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
    assert_eq!(server.await.unwrap(), b"hello");
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
            function main() -> uint8array? {{
                let sock = baml.net.TcpStream.connect("{addr}");
                let first = sock.read(1024);
                let second = sock.read(1024);
                first
            }}
        "#
    ));
    server.await.unwrap();

    insta::assert_snapshot!(stabilize_bytecode(&output.bytecode, &addr), @r#"
    function main() -> uint8array | null {
        load_const "{ADDR}"
        load_const <omitted>
        call baml.net.TcpStream.connect
        store_var sock
        load_var sock
        load_const 1024
        load_type baml.io.Read
        load_const "read"
        virtual_call nargs=2 ntypeargs=0 self_arg=0
        store_var first
        load_var sock
        load_const 1024
        load_type baml.io.Read
        load_const "read"
        virtual_call nargs=2 ntypeargs=0 self_arg=0
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
async fn net_read_is_cancellable() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let _conn = listener.accept().await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    });

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let sock = baml.net.TcpStream.connect("{addr}");
                let tok = baml.spawn.CancelToken.new();
                let read = spawn with tok {{
                    sock.read(1024)
                }};
                let deadline = spawn {{
                    baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
                    tok.cancel()
                }};
                let outcome = (await read) catch (e) {{
                    baml.panics.Cancelled => "cancelled"
                }};
                match (outcome) {{
                    let reason: string => reason,
                    null => "eof",
                    let chunk: uint8array => "read",
                }}
            }}
        "#
    ));
    server.await.unwrap();

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("cancelled".into()))
    );
}

#[tokio::test]
async fn net_close_cancels_pending_read() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let _conn = listener.accept().await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    });

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let sock = baml.net.TcpStream.connect("{addr}");
                let read = spawn {{ sock.read(1024) }};
                baml.sys.sleep(baml.time.Duration.from_milliseconds(25n));
                sock.close();
                (await read) catch (e) {{
                    baml.errors.Io => {{ return "closed"; }}
                }};
                "completed"
            }}
        "#
    ));
    server.await.unwrap();

    assert_eq!(output.result, Ok(BexExternalValue::String("closed".into())));
}

#[tokio::test]
async fn net_read_completes_before_cancellation() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        socket.write_all(b"prompt").await.unwrap();
        socket.shutdown().await.unwrap();
    });

    let output = baml_test!(&format!(
        r#"
            function main() -> uint8array? {{
                let sock = baml.net.TcpStream.connect("{addr}");
                let tok = baml.spawn.CancelToken.new();
                let read = spawn with tok {{
                    sock.read(1024)
                }};
                let deadline = spawn {{
                    baml.sys.sleep(baml.time.Duration.from_seconds(5n));
                    tok.cancel()
                }};
                let chunk = await read;
                deadline.cancel();
                chunk
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
            function main() -> uint8array? {{
                let sock = baml.net.TcpStream.connect(
                    "{addr}",
                    timeout = baml.time.Duration.from_seconds(5n),
                );
                sock.read(1024)
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

#[tokio::test]
async fn net_listener_accepts_a_connection() {
    // A free port for the BAML listener: bind, read the address, release it.
    let addr = {
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        probe.local_addr().unwrap().to_string()
    };

    // The client retries until the BAML listener is bound, then sends.
    let client_addr = addr.clone();
    let client = tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut socket = loop {
            match tokio::net::TcpStream::connect(&client_addr).await {
                Ok(socket) => break socket,
                Err(_) if tokio::time::Instant::now() < deadline => {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                Err(error) => panic!("the BAML listener never bound: {error}"),
            }
        };
        socket.write_all(b"accepted").await.unwrap();
        socket.shutdown().await.unwrap();
    });

    // `accept` hands back a stream the listener's own operation built.
    let output = baml_test!(&format!(
        r#"
            function main() -> uint8array? {{
                let listener = baml.net.TcpListener.bind("{addr}");
                let conn = listener.accept();
                let data = conn.read(1024);
                conn.close();
                listener.close();
                data
            }}
        "#
    ));
    client.await.unwrap();

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Uint8Array(b"accepted".to_vec()))
    );
}
