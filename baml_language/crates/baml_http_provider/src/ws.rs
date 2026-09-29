//! WebSocket connections, on tungstenite.

use baml_http_types::{BoxFuture, CloseFrame, Error, Message, WebSocket, WsConnectRequest};
use futures::{SinkExt, StreamExt, future};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{
        self,
        client::IntoClientRequest,
        http::{HeaderName, HeaderValue},
        protocol::frame::coding::CloseCode,
    },
};

pub(crate) fn connect(request: WsConnectRequest) -> BoxFuture<Result<WebSocket, Error>> {
    Box::pin(async move {
        crate::ensure_crypto_provider()?;
        let url = request.url;
        let mut handshake = url.as_str().into_client_request().map_err(|error| {
            Error::invalid_request(format!("invalid WebSocket URL '{url}': {error}"))
        })?;
        for (name, value) in &request.headers {
            let name = HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
                Error::invalid_request(format!("invalid WebSocket header name '{name}': {error}"))
            })?;
            let value = HeaderValue::from_str(value).map_err(|error| {
                Error::invalid_request(format!(
                    "invalid WebSocket header value for '{name}': {error}"
                ))
            })?;
            handshake.headers_mut().insert(name, value);
        }
        let (stream, _) = tokio_tungstenite::connect_async(handshake)
            .await
            .map_err(|error| Error::io(error.to_string()))?;
        Ok(from_tungstenite(stream))
    })
}

fn to_tungstenite(message: Message) -> tungstenite::Message {
    match message {
        Message::Text(text) => tungstenite::Message::text(text),
        Message::Binary(bytes) => tungstenite::Message::binary(bytes),
        Message::Close(frame) => {
            tungstenite::Message::Close(frame.map(|frame| tungstenite::protocol::CloseFrame {
                code: CloseCode::from(frame.code),
                reason: frame.reason.into(),
            }))
        }
    }
}

/// A tungstenite stream as a message-level [`WebSocket`]. Tungstenite answers
/// pings itself while reading, so control frames never surface.
pub(crate) fn from_tungstenite<S>(stream: WebSocketStream<S>) -> WebSocket
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (sink, source) = stream.split();
    let sink = sink
        .sink_map_err(|error| Error::io(error.to_string()))
        .with(|message: Message| future::ready(Ok::<_, Error>(to_tungstenite(message))));
    let source = source.filter_map(|frame| {
        future::ready(match frame {
            Ok(tungstenite::Message::Text(text)) => Some(Ok(Message::Text(text.as_str().into()))),
            Ok(tungstenite::Message::Binary(bytes)) => Some(Ok(Message::Binary(bytes.to_vec()))),
            Ok(tungstenite::Message::Close(frame)) => {
                Some(Ok(Message::Close(frame.map(|frame| CloseFrame {
                    code: u16::from(frame.code),
                    reason: frame.reason.as_str().to_string(),
                }))))
            }
            Ok(
                tungstenite::Message::Ping(_)
                | tungstenite::Message::Pong(_)
                | tungstenite::Message::Frame(_),
            ) => None,
            Err(error) => Some(Err(Error::io(error.to_string()))),
        })
    });
    WebSocket {
        sink: Box::pin(sink),
        source: Box::pin(source),
    }
}
