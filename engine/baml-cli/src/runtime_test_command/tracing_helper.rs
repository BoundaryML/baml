use tokio::{io::AsyncWriteExt, net::TcpStream};

pub async fn forward_to_port(port: u16, message: &str) -> tokio::io::Result<()> {
    const HOST: &str = "127.0.0.1";
    // Forward message to the port.
    let mut stream = TcpStream::connect(format!("{}:{}", HOST, port)).await?;
    stream.write_all(message.as_bytes()).await?;
    stream.write_all(b"<BAML_END_MSG>\n").await?;
    stream.flush().await
}
