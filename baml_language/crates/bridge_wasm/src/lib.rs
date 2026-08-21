//! `bridge_wasm` - WASM bindings for BAML.
//!
//! This crate only supports the `wasm32-unknown-unknown` target. Use
//! `--target wasm32-unknown-unknown` when building.
//!
//! The browser language server. `BamlWasmRuntime` is the JS-facing object:
//! it owns the [`baml_lsp::GlobalState`] for the tab and pumps LSP traffic
//! through it.
//!
//! ## The owner, single-threaded
//!
//! The native host runs the owner on its own thread and reads on a pool. Here
//! the JS thread *is* the owner, so the same state machine runs with
//! [`baml_lsp::executor::Executors::inline`]: a snapshot read executes
//! synchronously inside `spawn` and the snapshot is dropped before it
//! returns, which is exactly the invariant that keeps Salsa's `set_*` from
//! ever seeing a live clone (a mutation with one outstanding would hang the
//! tab). Owner events are drained after each inbound message rather than by a
//! blocking `select!` loop, and the debounced diagnostics tail is flushed the
//! same way — see [`BamlWasmRuntime::pump`].
//!
//! The playground half (engine, runs, tests) is a separate surface still
//! being rebuilt; this file is the analysis half only.

use std::{cell::RefCell, rc::Rc, sync::Arc, sync::Once};

use baml_lsp::{GlobalState, SessionKey, executor::Executors};
use js_sys::Function;
use wasm_bindgen::prelude::*;

use crate::{
    lsp_wire::WasmClientSender,
    wasm_vfs::{WasmProjectFs, WasmVfs},
};

mod handle;
mod host_value {
    pub(crate) use sys_wasm::WasmHost;
}
mod lsp_wire;
mod playground;
mod playground_notify;
mod registry;
mod send_wrapper {
    pub(crate) use sys_wasm::{SendFuture, SendWrapper};
}
mod wasm_env;
mod wasm_http;
mod wasm_io;
mod wasm_io_fs;
mod wasm_io_glob;
mod wasm_random;
mod wasm_sys;
mod wasm_time;
mod wasm_vfs;

pub use lsp_wire::{LspNotification, LspRequest, LspResponse, LspResponseError};

static LOGGER_INIT: Once = Once::new();

#[wasm_bindgen(start)]
pub fn start() {
    bex_project::register_inbound_union_ambiguity_policy(
        bex_project::InboundUnionAmbiguityPolicy::SelectDefault,
    )
    .expect("the browser TypeScript bridge must own the process-wide inbound policy");
    #[cfg(feature = "console_error_panic")]
    console_error_panic_hook::set_once();
    LOGGER_INIT.call_once(|| {
        let level = if cfg!(debug_assertions) {
            log::Level::Debug
        } else {
            log::Level::Info
        };
        wasm_logger::init(wasm_logger::Config::new(level));
    });
}

/// Get the version of the `bridge_wasm` crate.
#[wasm_bindgen]
pub fn version() -> String {
    baml_version::CANONICAL_VERSION.to_string()
}

/// Get the Git commit used to build the `bridge_wasm` crate.
#[wasm_bindgen(js_name = commitHash)]
pub fn commit_hash() -> String {
    env!("BRIDGE_WASM_GIT_SHA").to_string()
}

/// Returns the build timestamp (unix seconds) for hot-reload / build-identity checks.
#[wasm_bindgen(js_name = getBuildTime)]
pub fn get_build_time() -> String {
    env!("BRIDGE_WASM_BUILD_TS").to_string()
}

/// The `HostCallId` an in-flight wasm call reports under. Ids beyond `u32`
/// cannot be represented on the wire, and a run that cannot be addressed is
/// better left unattached than mislabelled.
pub(crate) fn wasm_host_call_id(call_id: sys_types::CallId) -> Option<bex_events::run::HostCallId> {
    u32::try_from(call_id.0)
        .ok()
        .map(bex_events::run::HostCallId::Wasm)
}

/// Push one run patch to the host. The interposing sys-ops (fetch logs, env
/// prompts, `baml.io`) report progress this way.
pub(crate) fn send_run_patch(
    callback: &send_wrapper::SendWrapper<js_sys::Function>,
    patch: &bex_events::run::RunPatch,
) {
    playground_notify::send_wasm_playground_notification(
        callback.inner(),
        &playground_notify::PlaygroundNotification::RunPatch {
            patch: bex_events::run::patch_to_wire(patch),
        },
    );
}

/// The callbacks the host installs on the runtime: the LSP's two outbound
/// channels, the playground's one, and the platform operations the browser
/// has to perform on the runtime's behalf.
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = r#"{
        fetch: (request: any) => Promise<any>;
        env: (name: string) => Promise<string | undefined>;
        input: (prompt: string) => Promise<string>;
        exec: (command: string, args: string[]) => Promise<any>;
        shell: (command: string) => Promise<any>;
        host_dispatch: (call: any) => void;
        lsp_send_notification: (notification: LspNotification) => void;
        lsp_send_response: (response: LspResponse) => void;
        playground_send_notification: (notification: PlaygroundNotification) => void;
    }"#)]
    pub type WasmCallbacks;

    #[wasm_bindgen(method, getter, structural)]
    fn fetch(this: &WasmCallbacks) -> Function;

    #[wasm_bindgen(method, getter, structural, js_name = "env")]
    fn env(this: &WasmCallbacks) -> Function;

    #[wasm_bindgen(method, getter, structural, js_name = "input")]
    fn input(this: &WasmCallbacks) -> Function;

    #[wasm_bindgen(method, getter, structural, js_name = "exec")]
    fn exec(this: &WasmCallbacks) -> Function;

    #[wasm_bindgen(method, getter, structural, js_name = "shell")]
    fn shell(this: &WasmCallbacks) -> Function;

    #[wasm_bindgen(method, getter, structural, js_name = "host_dispatch")]
    fn host_dispatch(this: &WasmCallbacks) -> Function;

    #[wasm_bindgen(method, getter, structural, js_name = "lsp_send_notification")]
    fn lsp_send_notification(this: &WasmCallbacks) -> Function;

    #[wasm_bindgen(method, getter, structural, js_name = "lsp_send_response")]
    fn lsp_send_response(this: &WasmCallbacks) -> Function;

    #[wasm_bindgen(method, getter, structural, js_name = "playground_send_notification")]
    fn playground_send_notification(this: &WasmCallbacks) -> Function;
}

/// The browser's platform: every namespace the standard library can reach,
/// with the six the host has to perform for us routed through its callbacks.
///
/// Built by enumeration, and it has to be: there is no native table to start
/// from the way [`sys_native::SysOps::native`] serves the desktop host. An
/// omission here is not a missing feature but a `baml.errors.Unsupported`
/// thrown from the middle of a user's program — `testing.run_test` calls
/// `baml.time.Instant.now` on every test, so dropping one namespace silently
/// costs the whole test surface.
fn build_wasm_sys_ops(
    callbacks: &WasmCallbacks,
    run_store: &Arc<bex_events::run::InMemoryRunStore>,
    playground: &send_wrapper::SendWrapper<Function>,
    vfs: &Arc<wasm_vfs::WasmVfs>,
) -> sys_ops::SysOps {
    sys_ops::SysOpsBuilder::new()
        .with_http_instance(Arc::new(wasm_http::WasmHttp::new(
            callbacks.fetch(),
            run_store.clone(),
            playground.clone(),
        )))
        .with_env_instance(Arc::new(wasm_env::WasmEnv::new(
            callbacks.env(),
            run_store.clone(),
            playground.clone(),
        )))
        .with_io_instance(Arc::new(wasm_io::WasmIo::new(
            callbacks.input(),
            run_store.clone(),
            playground.clone(),
        )))
        .with_sys_instance(Arc::new(wasm_sys::WasmSys::new(
            callbacks.exec(),
            callbacks.shell(),
        )))
        .with_fs_instance(Arc::new(wasm_io_fs::WasmIoFs::new(Arc::clone(vfs))))
        .with_glob_instance(Arc::new(wasm_io_glob::WasmIoGlob::new(Arc::clone(vfs))))
        .with_time_instance(Arc::new(wasm_time::WasmTime))
        .with_random_instance(Arc::new(wasm_random::WasmRandom))
        // One `WasmHost` per runtime, holding *this* runtime's `host_dispatch`
        // so a BAML→host call reaches the right wrapper; a process-global one
        // would let a second runtime clobber the first's.
        .with_host_instance(Arc::new(host_value::WasmHost::new(
            callbacks.host_dispatch(),
            false,
        )))
        .build()
}

/// One BAML language server for the tab.
#[wasm_bindgen]
pub struct BamlWasmRuntime {
    /// `RefCell` is the single-threaded spelling of the owner's exclusive
    /// access. Every borrow is confined to one JS callback, and no borrow is
    /// ever held across a call back into JS, so re-entrancy cannot observe a
    /// half-applied state.
    state: Rc<RefCell<GlobalState>>,
    /// `Arc` because [`baml_lsp::ClientSender`] is a `Send + Sync` trait
    /// (the native host shares one across threads); the sender's own JS
    /// handles carry that through [`sys_wasm::SendWrapper`].
    sender: Arc<WasmClientSender>,
    session: SessionKey,
    /// The engine and what it was built from. `RefCell` for the same reason
    /// `state` is one: exclusive access on a single thread.
    playground: Rc<RefCell<playground::PlaygroundState>>,
    playground_sender: playground_notify::WasmPlaygroundSender,
    sys_ops: Arc<sys_ops::SysOps>,
    /// Set by the owner's source observer whenever a batch lands, so `pump`
    /// knows a rebuild is owed. The observer runs *inside* `apply`, where the
    /// state is already mutably borrowed, so it can only record the fact.
    build_owed: Arc<std::sync::atomic::AtomicBool>,
}

/// The browser session's key. One runtime, one client, one session.
const BROWSER_SESSION: SessionKey = SessionKey(1);

#[wasm_bindgen]
impl BamlWasmRuntime {
    /// Build the runtime over the host's filesystem and callbacks.
    #[wasm_bindgen]
    pub fn create(callbacks: &WasmCallbacks, vfs: WasmVfs) -> Self {
        bex_events::prof::enable_wasm_cooperative_profile();
        let sender = Arc::new(WasmClientSender::new(
            callbacks.lsp_send_notification(),
            callbacks.lsp_send_response(),
        ));
        let playground_sender =
            playground_notify::WasmPlaygroundSender::new(callbacks.playground_send_notification());
        let run_store = Arc::new(bex_events::run::InMemoryRunStore::default());
        // The JS filesystem is shared by the language server's discovery and
        // by `baml.fs`/`baml.glob` at run time; one handle, not two views.
        // `js_sys` values are `!Send`; this target is single-threaded and the
        // sys-ops table's signatures ask for `Arc`, so that is what they get.
        let vfs = Arc::new(vfs);
        let sys_ops = Arc::new(build_wasm_sys_ops(
            callbacks,
            &run_store,
            &send_wrapper::SendWrapper::new(callbacks.playground_send_notification()),
            &vfs,
        ));
        // No materialized stdlib on the web: goto-definition into the stdlib
        // has no real file to open, so the protocol layer declines those
        // targets rather than inventing a path.
        let mut state = GlobalState::with_fs(
            Executors::inline(),
            None,
            Arc::new(WasmProjectFs::new(Arc::clone(&vfs))),
        );
        state.open_session(
            BROWSER_SESSION,
            Arc::clone(&sender) as Arc<dyn baml_lsp::ClientSender>,
        );
        let build_owed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observer = Arc::clone(&build_owed);
        state.set_source_observer(Arc::new(move |_applied| {
            observer.store(true, std::sync::atomic::Ordering::Relaxed);
        }));
        Self {
            state: Rc::new(RefCell::new(state)),
            sender,
            session: BROWSER_SESSION,
            playground: Rc::new(RefCell::new(playground::PlaygroundState::default())),
            playground_sender,
            sys_ops,
            build_owed,
        }
    }

    /// Handle one client request and answer it through `lsp_send_response`.
    #[wasm_bindgen(js_name = handleLspRequest)]
    pub fn handle_lsp_request(&self, request: LspRequest) {
        let request: lsp_server::Request = request.into();
        let id = request.id.clone();
        let sender = Arc::clone(&self.sender);
        self.state.borrow_mut().dispatch_request(
            self.session,
            request,
            Box::new(move |result| sender.respond(id, result)),
        );
        self.pump();
    }

    /// Handle one client notification. Nothing is answered; the work it
    /// schedules (discovery, diagnostics) is drained by `pump`.
    #[wasm_bindgen(js_name = handleLspNotification)]
    pub fn handle_lsp_notification(&self, notification: LspNotification) {
        let notification: lsp_server::Notification = notification.into();
        let method = notification.method.clone();
        if let Err(error) = self
            .state
            .borrow_mut()
            .dispatch_notification(self.session, notification)
        {
            log::debug!("notification {method} not applied: {error}");
        }
        self.pump();
    }

    /// Push the project surface: what can be run, and what is wrong with it.
    #[wasm_bindgen(js_name = requestPlaygroundState)]
    pub fn request_playground_state(&self) {
        let state = self.state.borrow();
        let playground = self.playground.borrow();
        let Some((project, update)) = playground::project_update(&state, &playground) else {
            return;
        };
        drop(playground);
        drop(state);
        self.playground_sender
            .send(&playground_notify::PlaygroundNotification::ListProjects {
                projects: vec![project.clone()],
            });
        self.playground_sender
            .send(&playground_notify::PlaygroundNotification::UpdateProject { project, update });
    }

    /// Drop the session's state and drain its engine. The JS object itself is
    /// freed by `wasm_bindgen`.
    #[wasm_bindgen(js_name = closeSession)]
    pub fn close_session(&self) {
        self.state.borrow_mut().close_session(self.session);
        self.playground.borrow_mut().shutdown();
    }
}

impl BamlWasmRuntime {
    /// Run the owner to quiescence.
    ///
    /// Native hosts block in `select!` on the event queue and an armed timer.
    /// Here there is nothing to block on: an inline executor has already
    /// finished every job it was handed by the time `dispatch_*` returns, so
    /// draining the queue and firing any due tail is enough — and jobs
    /// enqueued *by* an event (discovery scheduling a diagnostics pass) are
    /// picked up by the same loop.
    ///
    /// Tails are fired at their deadline rather than after it: the browser
    /// has no timer thread to wake the owner later, so a pass left pending
    /// here would only run on the next keystroke.
    fn pump(&self) {
        loop {
            let event = self.state.borrow().events().try_recv();
            match event {
                Ok(event) => {
                    self.state.borrow_mut().handle_event(event);
                    continue;
                }
                Err(_) => {
                    let Some(deadline) = self.state.borrow().next_deadline() else {
                        break;
                    };
                    self.state.borrow_mut().on_tick(deadline);
                }
            }
        }
        // The engine is rebuilt only once the owner is quiet, so a batch of
        // events that all touch source costs one build rather than one each.
        if self
            .build_owed
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            playground::rebuild(
                &self.state.borrow(),
                &mut self.playground.borrow_mut(),
                &self.sys_ops,
            );
        }
    }
}
