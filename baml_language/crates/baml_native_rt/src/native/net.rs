//! `baml.net`: TCP and UDP sockets of Tokio's, each owned by the object
//! that stands for it as Rust state the heap drops, which closes the
//! socket, when the object is dead.

use std::{
    sync::{Arc, Mutex, PoisonError},
    task::Poll,
    time::Duration,
};

use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::{
        TcpListener, TcpStream, UdpSocket,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::watch,
};

use super::{
    Big, Bytes, NativeBamlNetBamlIoReadForTcpStream, NativeBamlNetBamlIoWriteForTcpStream,
    NativeBamlNetTcpListener, NativeBamlNetTcpStream, NativeBamlNetUdpSocket, Natives, Obj, Opaque,
    Root, Scope, Str, Thrown,
    failure::{Failure, limit},
    fs::MAX_READ_CHUNK,
};
use crate::layout::{
    NAMED_NET_DATAGRAM, NAMED_NET_TCP_LISTENER, NAMED_NET_TCP_STREAM, NAMED_NET_UDP_SOCKET,
};

/// The most one datagram holds.
const MOST_DATAGRAM: usize = 65_536;

/// A connection. Its two directions are held apart, so that a task that
/// waits to read does not keep another from writing.
struct Stream {
    reader: tokio::sync::Mutex<Option<OwnedReadHalf>>,
    writer: tokio::sync::Mutex<Option<OwnedWriteHalf>>,
    /// Told when the stream is closed: a read or a write that waits ends
    /// then, with an error.
    closed: watch::Sender<bool>,
}

/// A listener or a UDP socket, until it is closed. What waits on one
/// holds it for itself, so closing does not end a wait.
struct Shared<T>(Mutex<Option<Arc<T>>>);

impl<T> Shared<T> {
    fn new(socket: T) -> Arc<Self> {
        Arc::new(Self(Mutex::new(Some(Arc::new(socket)))))
    }

    /// The socket, or the error of using `what` once it is closed.
    fn open(&self, what: &str) -> Result<Arc<T>, Failure> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| Failure::Io(format!("{what} is closed")))
    }

    fn close(&self) {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).take();
    }
}

fn stream_closed() -> Failure {
    Failure::Io("TcpStream is closed".to_owned())
}

/// What the one field of `object` owns, for a job to use.
fn owned<T: Send + Sync + 'static>(scope: &Scope<'_>, object: Root<'_, Obj>) -> Arc<T> {
    // SAFETY: each class of `baml.net` has its handle in its one field.
    unsafe { object.get(scope).field::<Opaque>(0) }.share::<T>()
}

/// An instance of the class at `named`, whose one field owns `handle`.
fn wrap<'s, T: Send + Sync + 'static>(
    scope: &mut Scope<'s>,
    named: usize,
    handle: Arc<T>,
) -> Root<'s, Obj> {
    let handle = scope.alloc_owned(handle);
    let mut built = scope.build(scope.named_class(named));
    built.set(scope, 0, handle);
    built.finish()
}

fn stream_of(stream: TcpStream) -> Arc<Stream> {
    let (reader, writer) = stream.into_split();
    Arc::new(Stream {
        reader: tokio::sync::Mutex::new(Some(reader)),
        writer: tokio::sync::Mutex::new(Some(writer)),
        closed: watch::channel(false).0,
    })
}

/// Run `work` under `limit`, when there is one: what it made, or the
/// `Timeout` whose message is `late`.
async fn within<T>(
    limit: Option<Duration>,
    late: impl FnOnce() -> String,
    work: impl Future<Output = T>,
) -> Result<T, Failure> {
    let Some(limit) = limit else {
        return Ok(work.await);
    };
    tokio::time::timeout(limit, work)
        .await
        .map_err(|_| Failure::Timeout {
            message: late(),
            limit: Some(limit),
            which: "timeout",
        })
}

/// Run `work` on one direction of a stream, which `half` holds, until the
/// stream is closed.
async fn on_half<H, T>(
    stream: &Stream,
    half: &tokio::sync::Mutex<Option<H>>,
    work: impl AsyncFnOnce(&mut H) -> Result<T, Failure>,
) -> Result<T, Failure> {
    let mut closed = stream.closed.subscribe();
    let mut half = half.lock().await;
    if *closed.borrow() {
        return Err(stream_closed());
    }
    let half = half.as_mut().ok_or_else(stream_closed)?;
    let mut work = std::pin::pin!(work(half));
    let mut told = std::pin::pin!(closed.changed());
    // Whichever comes first, and the close when both have.
    std::future::poll_fn(|context| {
        if told.as_mut().poll(context).is_ready() {
            return Poll::Ready(Err(stream_closed()));
        }
        work.as_mut().poll(context)
    })
    .await
}

impl NativeBamlNetTcpStream for Natives {
    async fn _connect<'s>(
        scope: &mut Scope<'s>,
        addr: Root<'s, Str>,
        timeout_nanos: Root<'s, Big>,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let addr = addr.get(scope).as_str().to_owned();
        let limit = limit(&timeout_nanos.get(scope).value());
        let connected = scope
            .io(async move {
                within(
                    limit,
                    || format!("Connecting to '{addr}' timed out"),
                    TcpStream::connect(&addr),
                )
                .await?
                .map_err(|error| Failure::io(format_args!("Failed to connect to '{addr}'"), error))
            })
            .await;
        let stream = connected.map_err(|failure| failure.throw(scope))?;
        Ok(wrap(scope, NAMED_NET_TCP_STREAM, stream_of(stream)))
    }

    async fn close<'s>(scope: &mut Scope<'s>, stream: Root<'s, Obj>) -> Result<(), Thrown<'s>> {
        let stream = owned::<Stream>(scope, stream);
        let closed = scope
            .io(async move {
                stream.closed.send_replace(true);
                stream.reader.lock().await.take();
                let Some(mut writer) = stream.writer.lock().await.take() else {
                    return Ok(());
                };
                match writer.shutdown().await {
                    Err(error) if error.kind() != std::io::ErrorKind::NotConnected => {
                        Err(Failure::io("Failed to close socket", error))
                    }
                    _ => Ok(()),
                }
            })
            .await;
        closed.map_err(|failure| failure.throw(scope))
    }
}

impl NativeBamlNetBamlIoReadForTcpStream for Natives {
    async fn read<'s>(
        scope: &mut Scope<'s>,
        stream: Root<'s, Obj>,
        limit: i64,
    ) -> Result<Option<Root<'s, Bytes>>, Thrown<'s>> {
        if limit <= 0 {
            return Ok(Some(scope.alloc_bytes(&[])));
        }
        let limit = usize::try_from(limit)
            .unwrap_or(MAX_READ_CHUNK)
            .min(MAX_READ_CHUNK);
        let stream = owned::<Stream>(scope, stream);
        let read = scope
            .io(async move {
                on_half(&stream, &stream.reader, async |reader| {
                    let mut chunk = vec![0; limit];
                    let length = reader
                        .read(&mut chunk)
                        .await
                        .map_err(|error| Failure::io("Failed to read from socket", error))?;
                    chunk.truncate(length);
                    Ok(chunk)
                })
                .await
            })
            .await;
        let chunk = read.map_err(|failure| failure.throw(scope))?;
        Ok((!chunk.is_empty()).then(|| scope.alloc_bytes(&chunk)))
    }
}

impl NativeBamlNetBamlIoWriteForTcpStream for Natives {
    async fn write_some<'s>(
        scope: &mut Scope<'s>,
        stream: Root<'s, Obj>,
        data: Root<'s, Bytes>,
    ) -> Result<i64, Thrown<'s>> {
        let stream = owned::<Stream>(scope, stream);
        let data = data.get(scope).hold();
        let written = scope
            .io(async move {
                on_half(&stream, &stream.writer, async |writer| {
                    let length = writer
                        .write(&data)
                        .await
                        .map_err(|error| Failure::io("Failed to write to socket", error))?;
                    i64::try_from(length)
                        .map_err(|_| Failure::Io("socket write count exceeds int range".to_owned()))
                })
                .await
            })
            .await;
        written.map_err(|failure| failure.throw(scope))
    }
}

impl NativeBamlNetTcpListener for Natives {
    async fn bind<'s>(
        scope: &mut Scope<'s>,
        addr: Root<'s, Str>,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let addr = addr.get(scope).as_str().to_owned();
        let bound = scope
            .io(async move {
                TcpListener::bind(&addr)
                    .await
                    .map_err(|error| Failure::io(format_args!("Failed to bind '{addr}'"), error))
            })
            .await;
        let listener = bound.map_err(|failure| failure.throw(scope))?;
        Ok(wrap(scope, NAMED_NET_TCP_LISTENER, Shared::new(listener)))
    }

    async fn accept<'s>(
        scope: &mut Scope<'s>,
        listener: Root<'s, Obj>,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let listener = owned::<Shared<TcpListener>>(scope, listener);
        let accepted = scope
            .io(async move {
                let listener = listener.open("TcpListener")?;
                let (stream, _) = listener
                    .accept()
                    .await
                    .map_err(|error| Failure::io("Failed to accept connection", error))?;
                Ok(stream)
            })
            .await;
        let stream: TcpStream = accepted.map_err(|failure: Failure| failure.throw(scope))?;
        Ok(wrap(scope, NAMED_NET_TCP_STREAM, stream_of(stream)))
    }

    async fn close<'s>(scope: &mut Scope<'s>, listener: Root<'s, Obj>) -> Result<(), Thrown<'s>> {
        let listener = owned::<Shared<TcpListener>>(scope, listener);
        // The socket closes on a thread of the I/O runtime, which made it.
        scope.io(async move { listener.close() }).await;
        Ok(())
    }
}

impl NativeBamlNetUdpSocket for Natives {
    async fn bind<'s>(
        scope: &mut Scope<'s>,
        addr: Root<'s, Str>,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let addr = addr.get(scope).as_str().to_owned();
        let bound = scope
            .io(async move {
                UdpSocket::bind(&addr).await.map_err(|error| {
                    Failure::io(format_args!("Failed to bind UDP '{addr}'"), error)
                })
            })
            .await;
        let socket = bound.map_err(|failure| failure.throw(scope))?;
        Ok(wrap(scope, NAMED_NET_UDP_SOCKET, Shared::new(socket)))
    }

    async fn _send_to<'s>(
        scope: &mut Scope<'s>,
        socket: Root<'s, Obj>,
        data: Root<'s, Bytes>,
        addr: Root<'s, Str>,
        timeout_nanos: Root<'s, Big>,
    ) -> Result<i64, Thrown<'s>> {
        let socket = owned::<Shared<UdpSocket>>(scope, socket);
        let data = data.get(scope).hold();
        let addr = addr.get(scope).as_str().to_owned();
        let limit = limit(&timeout_nanos.get(scope).value());
        let sent = scope
            .io(async move {
                let socket = socket.open("UdpSocket")?;
                let length = within(
                    limit,
                    || format!("Sending to '{addr}' timed out"),
                    socket.send_to(&data, &addr),
                )
                .await?
                .map_err(|error| Failure::io(format_args!("Failed to send to '{addr}'"), error))?;
                Ok(i64::try_from(length).unwrap_or(i64::MAX))
            })
            .await;
        sent.map_err(|failure: Failure| failure.throw(scope))
    }

    async fn _recv_from<'s>(
        scope: &mut Scope<'s>,
        socket: Root<'s, Obj>,
        timeout_nanos: Root<'s, Big>,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let socket = owned::<Shared<UdpSocket>>(scope, socket);
        let limit = limit(&timeout_nanos.get(scope).value());
        let received = scope
            .io(async move {
                let socket = socket.open("UdpSocket")?;
                let mut datagram = vec![0; MOST_DATAGRAM];
                let (length, peer) = within(
                    limit,
                    || "Receiving datagram timed out".to_owned(),
                    socket.recv_from(&mut datagram),
                )
                .await?
                .map_err(|error| Failure::io("Failed to receive datagram", error))?;
                datagram.truncate(length);
                Ok((datagram, peer.to_string()))
            })
            .await;
        let (datagram, peer) = received.map_err(|failure: Failure| failure.throw(scope))?;
        let data = scope.alloc_bytes(&datagram);
        let addr = scope.alloc_str(&peer);
        let mut built = scope.build(scope.named_class(NAMED_NET_DATAGRAM));
        built.set(scope, 0, data);
        built.set(scope, 1, addr);
        Ok(built.finish())
    }

    async fn close<'s>(scope: &mut Scope<'s>, socket: Root<'s, Obj>) -> Result<(), Thrown<'s>> {
        let socket = owned::<Shared<UdpSocket>>(scope, socket);
        // As for a listener.
        scope.io(async move { socket.close() }).await;
        Ok(())
    }
}
