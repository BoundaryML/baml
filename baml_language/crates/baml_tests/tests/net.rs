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
            function main() -> string {{
                let sock = baml.net.connect("{addr}");
                sock.read()
            }}
        "#
    ));
    server.await.unwrap();

    insta::assert_snapshot!(stabilize_bytecode(&output.bytecode, &addr), @r#"
    function main() -> string {
        load_const "{ADDR}"
        dispatch_future baml.net.connect
        await
        dispatch_future baml.net.Socket.read
        await
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Hello from server!".to_string()))
    );
}

#[tokio::test]
async fn net_connect_failure() {
    let output = baml_test!(
        r#"
            function main() -> string {
                let sock = baml.net.connect("127.0.0.1:1");
                sock.read()
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "127.0.0.1:1"
        dispatch_future baml.net.connect
        await
        dispatch_future baml.net.Socket.read
        await
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
            function main() -> string {{
                let sock = baml.net.connect("{addr}");
                let first = sock.read();
                let second = sock.read();
                first
            }}
        "#
    ));
    server.await.unwrap();

    insta::assert_snapshot!(stabilize_bytecode(&output.bytecode, &addr), @r#"
    function main() -> string {
        load_const "{ADDR}"
        dispatch_future baml.net.connect
        await
        store_var sock
        load_var sock
        dispatch_future baml.net.Socket.read
        await
        store_var first
        load_var sock
        dispatch_future baml.net.Socket.read
        await
        store_var second
        load_var first
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("chunk1".to_string()))
    );
}
