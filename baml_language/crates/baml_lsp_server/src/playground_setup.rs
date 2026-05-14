//! Reusable wiring for the playground HTTP/WS server and SysOps interception.
//!
//! Used both by the native LSP server (`run_server`) and by bridge_cffi when
//! it wants to expose a playground from inside a host process (Python, etc.).

use std::sync::Arc;

use bex_events::{EventSink, FanOutEventSink};
use tokio::sync::broadcast;

use crate::{
    playground_env::{PlaygroundEnv, PlaygroundEnvState},
    playground_event_sink::PlaygroundEventSink,
    playground_http::{PlaygroundHttp, PlaygroundHttpState},
    playground_io::{PlaygroundIo, PlaygroundIoState},
    playground_ws::WsOutMessage,
};

#[allow(clippy::type_complexity)]
pub type SysOpFactory =
    Arc<dyn Fn(&vfs::VfsPath) -> Arc<sys_ops::SysOps> + Send + Sync>;

/// All the shared state the playground HTTP server and the SysOps shims need.
///
/// `event_sink` is the composed `FanOutEventSink` (playground always-on, file
/// sink optional based on `BAML_TRACE_FILE`).
pub struct PlaygroundWiring {
    pub broadcast_tx: broadcast::Sender<WsOutMessage>,
    pub env_state: Arc<PlaygroundEnvState>,
    pub io_state: Arc<PlaygroundIoState>,
    pub sys_op_factory: SysOpFactory,
    pub event_sink: Arc<dyn EventSink>,
}

impl PlaygroundWiring {
    /// Build playground wiring. Reads `BAML_TRACE_FILE` to decide whether to
    /// attach a file sink alongside the playground event sink.
    pub fn build() -> Self {
        let (broadcast_tx, _) = broadcast::channel::<WsOutMessage>(64);
        let env_state = Arc::new(PlaygroundEnvState::new(broadcast_tx.clone()));
        let io_state = Arc::new(PlaygroundIoState::new(broadcast_tx.clone()));

        let bcast_for_factory = broadcast_tx.clone();
        let env_for_factory = env_state.clone();
        let io_for_factory = io_state.clone();
        let sys_op_factory: SysOpFactory = Arc::new(move |_path: &vfs::VfsPath| {
            let http_state = Arc::new(PlaygroundHttpState::new(bcast_for_factory.clone()));
            Arc::new(
                sys_ops::SysOpsBuilder::new()
                    .with_fs::<sys_native::NativeSysOps>()
                    .with_sys::<sys_native::NativeSysOps>()
                    .with_net::<sys_native::NativeSysOps>()
                    .with_http_instance(Arc::new(PlaygroundHttp(http_state)))
                    .with_env_instance(Arc::new(PlaygroundEnv(env_for_factory.clone())))
                    .with_io_instance(Arc::new(PlaygroundIo(io_for_factory.clone())))
                    .build(),
            )
        });

        let playground_sink: Arc<dyn EventSink> =
            Arc::new(PlaygroundEventSink::new(broadcast_tx.clone()));
        let file_sink: Option<Arc<dyn EventSink>> = std::env::var("BAML_TRACE_FILE")
            .ok()
            .map(|trace_file| bex_events_native::start(trace_file.into()));

        let event_sink: Arc<dyn EventSink> = {
            let mut sinks: Vec<Arc<dyn EventSink>> = vec![playground_sink];
            if let Some(file_sink) = file_sink {
                sinks.push(file_sink);
            }
            Arc::new(FanOutEventSink::new(sinks))
        };

        Self {
            broadcast_tx,
            env_state,
            io_state,
            sys_op_factory,
            event_sink,
        }
    }
}
