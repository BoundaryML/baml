use tokio::net::TcpListener;

pub struct PortPicks {
    pub playground_port: u16,
    pub playground_listener: TcpListener,
    pub proxy_port: u16,
    pub proxy_listener: TcpListener,
}

pub async fn pick() -> anyhow::Result<PortPicks> {
    const MAX_PORT_ATTEMPTS: u16 = 100;

    for playground_port in 4000..4000 + MAX_PORT_ATTEMPTS {
        let proxy_port = playground_port + 1;

        if let (Ok(playground_listener), Ok(proxy_listener)) = (
            TcpListener::bind(("127.0.0.1", playground_port)).await,
            TcpListener::bind(("127.0.0.1", proxy_port)).await
        ) {
            return Ok(PortPicks { playground_port, playground_listener, proxy_port, proxy_listener });
        }
    }

    Err(anyhow::anyhow!("Failed to find an available port after {} attempts", MAX_PORT_ATTEMPTS))
}
