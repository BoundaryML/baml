//! In-memory transport for substitution tests. Never opens a connection.
use std::sync::Arc;
use baml_http_types::*;

pub fn provider() -> Arc<dyn HttpProvider> { Arc::new(TestTransport) }
struct TestTransport;

impl HttpProvider for TestTransport {
    fn send(&self, request: Request) -> BoxFuture<Result<Response, Error>> {
        Box::pin(async move {
            Ok(Response {
                status: 200,
                headers: vec![],
                url: request.url,
                body: Box::pin(futures::stream::iter([
                    Ok(Bytes::from_static(b"replacement ")),
                    Ok(Bytes::from_static(b"transport")),
                ])),
            })
        })
    }
    fn connect_websocket(&self, _: WsConnectRequest) -> BoxFuture<Result<WebSocket, Error>> {
        Box::pin(async { Err(Error::unsupported("replacement websocket")) })
    }
    fn bind(&self, _: String) -> BoxFuture<Result<Arc<dyn Listener>, Error>> {
        Box::pin(async { Err(Error::unsupported("replacement server")) })
    }
    fn check_tls(&self, _: &TlsConfig) -> Result<(), Error> {
        Err(Error::unsupported("replacement TLS"))
    }
}
