//! Tests for network operations (baml.net.connect, socket.read).

mod common;

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;
use tokio::{io::AsyncWriteExt, net::TcpListener};

#[tokio::test]
async fn net_connect_and_read() -> anyhow::Result<()> {
    // Start a TCP server that sends a message and closes
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    // Spawn server task
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        socket.write_all(b"Hello from server!").await.unwrap();
        socket.shutdown().await.unwrap();
    });

    // Compile and run BAML code
    let source = format!(
        r#"
        function main() -> string {{
            let sock = baml.net.connect("{addr}");
            sock.read()
        }}
        "#
    );

    let snapshot = compile_for_engine(&source);
    let engine = BexEngine::new(
        snapshot,
        std::sync::Arc::new(sys_types::SysOps::native()),
        None,
    )?;
    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await?;

    // Wait for server to finish
    server.await?;

    // Check result
    match result {
        BexExternalValue::String(s) => {
            assert_eq!(s, "Hello from server!");
        }
        other => panic!("Expected string, got: {other:?}"),
    }

    Ok(())
}

#[tokio::test]
async fn net_connect_failure() -> anyhow::Result<()> {
    // Try to connect to a port that's not listening
    let source = r#"
        function main() -> string {
            let sock = baml.net.connect("127.0.0.1:1");
            sock.read()
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = BexEngine::new(
        snapshot,
        std::sync::Arc::new(sys_types::SysOps::native()),
        None,
    )?;
    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Failed to connect") || err.contains("Connection refused"),
        "Expected connection error, got: {err}"
    );

    Ok(())
}

#[tokio::test]
async fn net_multiple_reads() -> anyhow::Result<()> {
    // Start a TCP server that sends data in chunks
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        socket.write_all(b"chunk1").await.unwrap();
        // Small delay to ensure data is sent separately
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        socket.write_all(b"chunk2").await.unwrap();
        socket.shutdown().await.unwrap();
    });

    // Read twice from the socket
    let source = format!(
        r#"
        function main() -> string {{
            let sock = baml.net.connect("{addr}");
            let first = sock.read();
            let second = sock.read();
            first
        }}
        "#
    );

    let snapshot = compile_for_engine(&source);
    let engine = BexEngine::new(
        snapshot,
        std::sync::Arc::new(sys_types::SysOps::native()),
        None,
    )?;
    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await?;

    server.await?;

    // First read should get "chunk1"
    match result {
        BexExternalValue::String(s) => {
            assert_eq!(s, "chunk1");
        }
        other => panic!("Expected string, got: {other:?}"),
    }

    Ok(())
}
