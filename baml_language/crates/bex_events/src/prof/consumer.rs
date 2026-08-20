//! Process-wide direct profiling consumer.
//!
//! Producers retain the compact bounded rings; this consumer decodes them
//! directly into the segmented backend without a transcode or invocation
//! reconstruction stage.

#![allow(unsafe_code)]

use std::{
    collections::HashSet,
    sync::{OnceLock, mpsc},
    time::Duration,
};

const WAKE_INTERVAL: Duration = Duration::from_millis(50);

use crate::{
    ids::{EngineId, ProcessEuid},
    prof::{metadata, registry::Registry, ring::RingCtx},
};

pub(crate) enum ControlMsg {
    Flush(mpsc::SyncSender<()>),
    EngineClosed(u64),
}

pub(crate) struct ConsumerEnv {
    pub(crate) registry: &'static Registry,
    pub(crate) ctx: &'static RingCtx,
    pub(crate) wake_interval: Duration,
}

static CONTROL_TX: OnceLock<mpsc::Sender<ControlMsg>> = OnceLock::new();

pub(crate) fn ensure_started() {
    CONTROL_TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        let env = ConsumerEnv {
            registry: crate::prof::registry::global_registry(),
            ctx: crate::prof::registry::global_ctx(),
            wake_interval: WAKE_INTERVAL,
        };
        std::thread::Builder::new()
            .name("bex-prof-consumer".into())
            .spawn(move || consumer_main(&rx, &env))
            .expect("failed to spawn direct profiling consumer");
        tx
    });
}

pub(crate) fn wake_for_backend_terminal() {
    ensure_started();
    crate::prof::registry::global_ctx().wake().force_wake();
}

pub fn flush_and_join(timeout: Duration) -> bool {
    let Some(tx) = CONTROL_TX.get() else {
        return true;
    };
    let (ack_tx, ack_rx) = mpsc::sync_channel(1);
    if tx.send(ControlMsg::Flush(ack_tx)).is_err() {
        return false;
    }
    crate::prof::registry::global_ctx().wake().force_wake();
    ack_rx.recv_timeout(timeout).is_ok()
}

pub fn engine_closed(engine_id: u64) {
    let Some(tx) = CONTROL_TX.get() else {
        let _ = metadata::remove_engine_metadata(engine_id);
        crate::prof::backend::unregister_engine_session(EngineId(engine_id));
        return;
    };
    if tx.send(ControlMsg::EngineClosed(engine_id)).is_ok() {
        crate::prof::registry::global_ctx().wake().force_wake();
    } else {
        let _ = metadata::remove_engine_metadata(engine_id);
        crate::prof::backend::unregister_engine_session(EngineId(engine_id));
    }
}

pub(crate) fn consumer_main(control: &mpsc::Receiver<ControlMsg>, env: &ConsumerEnv) {
    env.ctx.wake().register_consumer();
    let mut state = ConsumerState::default();
    loop {
        while let Ok(message) = control.try_recv() {
            match message {
                ControlMsg::Flush(ack) => {
                    state.drain_to_idle(env);
                    crate::prof::backend::maintain_sessions();
                    let _ = ack.send(());
                }
                ControlMsg::EngineClosed(engine_id) => {
                    state.drain_to_idle(env);
                    state.close_engine(engine_id);
                }
            }
        }
        state.sweep_once(env);
        crate::prof::backend::maintain_sessions();
        let wake = env.ctx.wake();
        wake.pre_park();
        // Recheck after advertising the parked state, then sleep even when
        // that recheck made progress. Producers wake on segment rollover and
        // terminal/control paths wake unconditionally, so polling an open
        // segment only bounces its commit cache line against the producer.
        // The timeout remains the bounded-latency path for low-volume streams.
        state.sweep_once(env);
        wake.park(env.wake_interval);
        wake.post_park();
    }
}

#[derive(Default)]
struct ConsumerState {
    closed_engines: HashSet<u64>,
}

impl ConsumerState {
    fn drain_to_idle(&mut self, env: &ConsumerEnv) {
        for _ in 0..1024 {
            if !self.sweep_once(env) {
                break;
            }
        }
    }

    fn sweep_once(&mut self, env: &ConsumerEnv) -> bool {
        // SAFETY: `consumer_main` is the registry's sole consumer.
        unsafe {
            env.registry.sweep(&mut |ring, bytes| {
                let engine_id = ring.engine_id();
                if !self.closed_engines.contains(&engine_id) {
                    crate::prof::backend::consume_engine_bytes(
                        ProcessEuid::current(),
                        EngineId(engine_id),
                        bytes,
                    );
                }
            })
        }
    }

    fn close_engine(&mut self, engine_id: u64) {
        let _ = metadata::remove_engine_metadata(engine_id);
        crate::prof::backend::unregister_engine_session(EngineId(engine_id));
        self.closed_engines.insert(engine_id);
    }
}
