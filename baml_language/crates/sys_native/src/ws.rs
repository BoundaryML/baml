//! A connected `baml.ws.WebSocket`: the platform's socket, shared by every
//! instance that holds it.

use std::sync::{Arc, OnceLock};

use sys_ops::io::VmBamlError;
use sys_types::{
    OpOutput,
    resource::{CloseEvent, WebSocketNextOutput, WebSocketResource, WebSocketSendData},
};
use tokio::sync::Mutex as TokioMutex;
use tokio_tungstenite::tungstenite::{Error as WsError, Message as WsMessage};

/// The write half of a connected WebSocket.
///
/// Type-erased rather than tied to one transport: a client socket
/// (`baml.ws.connect`) splits a plain-or-TLS TCP stream, while a server socket
/// (an HTTP upgrade inside `baml.http.Server.serve`) splits hyper's upgraded
/// IO. Both reach the whole `baml.ws.WebSocket` surface through this one type.
pub(crate) type WsSink = Box<dyn futures::Sink<WsMessage, Error = WsError> + Send + Unpin>;
/// The read half of a connected WebSocket. See [`WsSink`].
pub(crate) type WsSource =
    Box<dyn futures::Stream<Item = Result<WsMessage, WsError>> + Send + Unpin>;

/// A connected WebSocket.
///
/// Both transport halves are `Option` because the closing handshake releases
/// the socket eagerly: [`Self::finish`] publishes `close` and drops the halves
/// together, so a terminated stream is never polled again and every later
/// `send`/`next` answers from `close` alone.
pub(crate) struct WebSocketHandle {
    sink: TokioMutex<Option<WsSink>>,
    source: TokioMutex<Option<WsSource>>,
    /// How the connection ended. `1005` ("no status received") when the
    /// peer's close frame carried no code and `1006` ("abnormal closure")
    /// when the stream ended with no closing handshake at all — the same two
    /// synthetic codes a browser reports on its `CloseEvent`.
    close: OnceLock<CloseEvent>,
}

impl WebSocketHandle {
    pub(crate) fn new(sink: WsSink, source: WsSource) -> Arc<Self> {
        Arc::new(Self {
            sink: TokioMutex::new(Some(sink)),
            source: TokioMutex::new(Some(source)),
            close: OnceLock::new(),
        })
    }

    /// End the connection: complete the closing handshake, release the socket,
    /// and publish `close`.
    ///
    /// Closing the sink flushes the close frame tungstenite queues in reply to
    /// the peer's, which `read` alone leaves pending — without it the peer sees
    /// the connection vanish mid-handshake.
    ///
    /// The read half calls this the moment the connection ends, handing over
    /// its own `source` guard. Holding that guard across the whole call is what
    /// makes "the transport is gone" and "the close event is published"
    /// inseparable: no other caller can reach the source without the guard, and
    /// by then `close` is set.
    async fn finish(&self, source: &mut Option<WsSource>, close: CloseEvent) -> &CloseEvent {
        use futures::SinkExt;

        if let Some(mut sink) = self.sink.lock().await.take() {
            // Best effort — the peer may already be gone.
            let _ = sink.close().await;
        }
        *source = None;
        self.close.get_or_init(|| close)
    }

    /// Hang up: the side that owns this connection is done with it.
    ///
    /// Used by the HTTP server when a `WsAccept` handler returns, so a served
    /// connection's lifetime is the handler's rather than the garbage
    /// collector's.
    ///
    /// Takes the full teardown when nothing else is reading. If a `next` is in
    /// flight it closes only the write half and leaves that reader to publish
    /// the close event when the peer's echo (or EOF) arrives — waiting on the
    /// read lock here would block until a frame that may never come.
    pub(crate) async fn hangup(&self) {
        use futures::SinkExt;

        match self.source.try_lock() {
            Ok(mut source) => {
                if self.close.get().is_none() {
                    // `SinkExt::close` sends a status-less close frame, so
                    // `1005` is what both ends observe.
                    self.finish(&mut source, close_event(1005, String::new()))
                        .await;
                }
            }
            Err(_) => {
                if let Some(mut sink) = self.sink.lock().await.take() {
                    let _ = sink.close().await;
                }
            }
        }
    }
}

fn close_event(code: u16, reason: String) -> CloseEvent {
    CloseEvent {
        code: i64::from(code),
        reason,
    }
}

impl WebSocketResource for WebSocketHandle {
    fn send(self: Arc<Self>, data: WebSocketSendData) -> OpOutput<()> {
        use futures::SinkExt;

        let frame = match data {
            WebSocketSendData::String(text) => WsMessage::text(text),
            WebSocketSendData::Uint8Array(bytes) => WsMessage::binary(bytes),
        };
        OpOutput::async_op(async move {
            let mut sink = self.sink.lock().await;
            let sink = sink.as_mut().ok_or_else(|| VmBamlError::Io {
                message: "WebSocket send failed: the connection is closed".to_string(),
            })?;
            sink.send(frame).await.map_err(|error| VmBamlError::Io {
                message: format!("WebSocket send failed: {error}"),
            })?;
            Ok(())
        })
    }

    fn next(self: Arc<Self>) -> OpOutput<WebSocketNextOutput> {
        use futures::StreamExt;

        OpOutput::async_op(async move {
            if let Some(close) = self.close.get() {
                return Ok(WebSocketNextOutput::CloseEvent(close.clone()));
            }
            let mut source = self.source.lock().await;
            loop {
                // Re-checked under the lock: a concurrent `next` may have ended
                // the connection, which publishes the close event and drops the
                // transport in one step.
                let Some(stream) = source.as_mut() else {
                    let close = self.close.get().unwrap_or_else(|| {
                        unreachable!("WebSocket transport released without a close event")
                    });
                    break Ok(WebSocketNextOutput::CloseEvent(close.clone()));
                };
                let frame = match stream.next().await {
                    Some(Ok(frame)) => frame,
                    // End of stream with no closing handshake.
                    None => {
                        let close = self
                            .finish(&mut source, close_event(1006, String::new()))
                            .await;
                        break Ok(WebSocketNextOutput::CloseEvent(close.clone()));
                    }
                    // tokio-tungstenite fuses the stream on any read error —
                    // every later poll is `None` — so the connection is over
                    // as of this frame. Publish that now, so `send` and
                    // `hangup` answer from `close`, then report what ended it.
                    Some(Err(error)) => {
                        self.finish(&mut source, close_event(1006, String::new()))
                            .await;
                        return Err(VmBamlError::Io {
                            message: format!("WebSocket receive failed: {error}"),
                        }
                        .into());
                    }
                };
                match frame {
                    WsMessage::Text(text) => {
                        break Ok(WebSocketNextOutput::String(text.as_str().to_string()));
                    }
                    WsMessage::Binary(bytes) => {
                        break Ok(WebSocketNextOutput::Uint8Array(bytes.to_vec()));
                    }
                    WsMessage::Close(frame) => {
                        let close = frame.map_or_else(
                            || close_event(1005, String::new()),
                            |frame| {
                                close_event(
                                    u16::from(frame.code),
                                    frame.reason.as_str().to_string(),
                                )
                            },
                        );
                        let close = self.finish(&mut source, close).await;
                        break Ok(WebSocketNextOutput::CloseEvent(close.clone()));
                    }
                    // Tungstenite answers pings itself while reading; neither
                    // control frame is part of the BAML surface.
                    WsMessage::Ping(_) | WsMessage::Pong(_) | WsMessage::Frame(_) => {}
                }
            }
        })
    }

    fn close(self: Arc<Self>, code: i64, reason: String) -> OpOutput<()> {
        use futures::SinkExt;
        use tokio_tungstenite::tungstenite::protocol::{CloseFrame, frame::coding::CloseCode};

        // RFC 6455 §7.4: an endpoint may send 1000-1003, 1007-1013, and the
        // registered (3000-3999) and private (4000-4999) ranges. The undefined
        // codes and the four a peer may only ever infer (1004/1005/1006/1015)
        // are caller errors — `is_allowed` draws exactly that line.
        let close_code = match u16::try_from(code).map(CloseCode::from) {
            Ok(close_code) if close_code.is_allowed() => close_code,
            Ok(_) | Err(_) => {
                return OpOutput::err(VmBamlError::InvalidArgument {
                    message: format!("{code} is not a valid WebSocket close code"),
                });
            }
        };

        // RFC 6455 §5.5: a control frame carries at most 125 payload bytes,
        // and a close frame spends two on the status code. Tungstenite checks
        // this only when reading — `Frame::close` neither checks nor
        // truncates — so an oversized reason would reach the wire as a
        // protocol violation for the peer to fail the connection over.
        if reason.len() > 123 {
            return OpOutput::err(VmBamlError::InvalidArgument {
                message: format!(
                    "WebSocket close reason is {} bytes; a close frame carries at most 123",
                    reason.len()
                ),
            });
        }

        OpOutput::async_op(async move {
            // Best effort from here on: `close` declares only
            // `InvalidArgument`, and a peer that has already gone away is not a
            // caller error. The socket itself is released by `next` when the
            // peer's echo (or the end of the stream) arrives.
            if let Some(sink) = self.sink.lock().await.as_mut() {
                let _ = sink
                    .send(WsMessage::Close(Some(CloseFrame {
                        code: close_code,
                        reason: reason.into(),
                    })))
                    .await;
            }
            Ok(())
        })
    }

    fn measure(&self, meter: &mut sys_types::Meter) {
        // The transport's buffers are tungstenite's and unreported.
        if let Some(close) = self.close.get() {
            meter.bytes(close.reason.capacity());
        }
    }
}
