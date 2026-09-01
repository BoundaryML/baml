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
//! same way — see `BamlWasmRuntime::pump`.
//!
//! The playground half (engine, runs, tests) is a separate surface still
//! being rebuilt; this file is the analysis half only.

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Once},
};

use baml_lsp::{GlobalState, LspError, SessionKey, executor::Executors};
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
mod runs;
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
    let git_sha = env!("BRIDGE_WASM_GIT_SHA");
    if git_sha.is_empty() {
        String::new()
    } else {
        git_sha.to_owned()
    }
}

/// Returns the build timestamp (unix seconds) for hot-reload / build-identity checks.
#[wasm_bindgen(js_name = getBuildTime)]
pub fn get_build_time() -> String {
    env!("BRIDGE_WASM_BUILD_TS").to_string()
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
    ///
    /// One case escapes that: `wasm32-unknown-unknown` cannot unwind, so a
    /// panic inside a borrow never runs the guard's destructor and the cell
    /// stays borrowed for the life of the tab. Entry points test for it with
    /// [`Self::is_unavailable`] rather than tripping `already borrowed`.
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
    /// Live runs, and everything the host reads back about them.
    pub(crate) run_store: Arc<bex_events::run::InMemoryRunStore>,
    /// Terminal runs, retained in memory: the browser has no disk to spill to,
    /// so "history" is the same read API over the same process.
    pub(crate) history_store: runs::WasmHistoryStore,
    pub(crate) value_store: runs::WasmLiveValueStore,
    /// The raw playground callback, for the run paths that were salvaged
    /// around it. [`Self::playground_sender`] wraps the same function.
    pub(crate) playground_callback: send_wrapper::SendWrapper<Function>,
    /// Set by the owner's source observer whenever a batch lands, so `pump`
    /// knows a rebuild is owed. The observer runs *inside* `apply`, where the
    /// state is already mutably borrowed, so it can only record the fact.
    build_owed: Arc<std::sync::atomic::AtomicBool>,
    /// Whether the user has already been told the server is unavailable, so
    /// a wedged tab reports once instead of on every keystroke.
    unavailable_reported: std::cell::Cell<bool>,
}

/// What the user is told when an internal error has left the tab's language
/// server unusable. It names the fault as ours, not their file, and asks for
/// the report that would let us fix it.
const UNAVAILABLE_MESSAGE: &str = "BAML internal error: the language server for this tab hit a bug \
     and can no longer analyze your code. Reload the page to restart it. Please report this at \
     https://github.com/BoundaryML/baml/issues — including what you were editing — so we can fix it.";

/// The browser session's key. One runtime, one client, one session.
const BROWSER_SESSION: SessionKey = SessionKey(1);

#[wasm_bindgen]
impl BamlWasmRuntime {
    /// Build the runtime over the host's filesystem and callbacks.
    #[wasm_bindgen]
    pub fn create(callbacks: &WasmCallbacks, vfs: WasmVfs) -> Self {
        let sender = Arc::new(WasmClientSender::new(
            callbacks.lsp_send_notification(),
            callbacks.lsp_send_response(),
        ));
        let playground_sender =
            playground_notify::WasmPlaygroundSender::new(callbacks.playground_send_notification());
        let run_store = Arc::new(bex_events::run::InMemoryRunStore::default());
        let playground_callback =
            send_wrapper::SendWrapper::new(callbacks.playground_send_notification());
        let history_store = runs::new_history_store();
        let value_store = runs::new_value_store();
        // The JS filesystem is shared by the language server's discovery and
        // by `baml.fs`/`baml.glob` at run time; one handle, not two views.
        // `js_sys` values are `!Send`; this target is single-threaded and the
        // sys-ops table's signatures ask for `Arc`, so that is what they get.
        let vfs = Arc::new(vfs);
        let sys_ops = Arc::new(build_wasm_sys_ops(
            callbacks,
            &run_store,
            &playground_callback,
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
            run_store,
            history_store,
            value_store,
            playground_callback,
            build_owed,
            unavailable_reported: std::cell::Cell::new(false),
        }
    }

    /// Whether an earlier internal error left this runtime unusable, telling
    /// the user once when it has.
    ///
    /// `wasm32-unknown-unknown` has no unwinding, so a panic raised while the
    /// owner state was borrowed never runs the borrow guard's destructor: the
    /// cell stays borrowed for the life of the tab and every later entry point
    /// would trap on `already borrowed`. That trap names the symptom and not
    /// the cause, repeats on every keystroke, and reads like the user's own
    /// file is at fault — so entry points ask here first and refuse honestly.
    ///
    /// Recovery is the host's: the state is half-applied by construction, so
    /// this runtime cannot be trusted again and the page must be reloaded.
    fn is_unavailable(&self) -> bool {
        // A successful test borrow is released immediately; only a leaked
        // guard from a panicked call can keep this `Err`.
        if self.state.try_borrow_mut().is_ok() {
            return false;
        }
        if !self.unavailable_reported.replace(true) {
            log::error!(
                "the BAML language server is unavailable: an earlier internal error left its \
                 state unrecoverable (wasm cannot unwind, so the owner borrow was never released)"
            );
            let params = lsp_types::ShowMessageParams {
                typ: lsp_types::MessageType::ERROR,
                message: UNAVAILABLE_MESSAGE.to_owned(),
            };
            if let Ok(params) = serde_json::to_value(params) {
                let _ = baml_lsp::ClientSender::send_notification(
                    self.sender.as_ref(),
                    "window/showMessage",
                    params,
                );
            }
        }
        true
    }

    /// Handle one client request and answer it through `lsp_send_response`.
    #[wasm_bindgen(js_name = handleLspRequest)]
    pub fn handle_lsp_request(&self, request: LspRequest) {
        let request: lsp_server::Request = request.into();
        let id = request.id.clone();
        if self.is_unavailable() {
            // Answer rather than drop: an unanswered id hangs the client.
            self.sender
                .respond(id, Err(LspError::Internal(UNAVAILABLE_MESSAGE.to_owned())));
            return;
        }
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
        if self.is_unavailable() {
            return;
        }
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
        if self.is_unavailable() {
            return;
        }
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

    /// Build a function's control-flow graph and send it back.
    #[wasm_bindgen(js_name = requestControlFlowGraph)]
    pub fn request_control_flow_graph(
        &self,
        project: &str,
        function_name: &str,
        request_id: Option<u32>,
    ) {
        if self.is_unavailable() {
            return;
        }
        if !self.serves(project) {
            return;
        }
        let graph = playground::control_flow_graph(&self.state.borrow(), function_name);
        self.playground_sender.send(
            &playground_notify::PlaygroundNotification::ControlFlowGraphResult {
                function_name: function_name.to_owned(),
                graph,
                request_id,
            },
        );
    }

    /// Report what the cursor is inside, so the graph view can follow along.
    #[wasm_bindgen(js_name = handleCursorPosition)]
    pub fn handle_cursor_position(&self, file: &str, line: u32, column: u32) {
        if self.is_unavailable() {
            return;
        }
        let Some(context) = playground::cursor_context(&self.state.borrow(), file, line, column)
        else {
            return;
        };
        self.playground_sender
            .send(&playground_notify::PlaygroundNotification::CursorContext { context });
    }

    /// Collect the project's tests from the installed engine and send the tree.
    ///
    /// Fire-and-forget: the tree arrives as a `testCollectionResult`
    /// notification, or not at all if a rebuild overtakes the collection.
    #[wasm_bindgen(js_name = requestCollectTests)]
    pub fn request_collect_tests(&self, project: &str) {
        if self.is_unavailable() {
            return;
        }
        if !self.serves(project) {
            return;
        }
        let Some((project, package)) = playground::workspace_root(&self.state.borrow()) else {
            return;
        };
        let revision = self.state.borrow().revision();
        let ticket = self
            .playground
            .borrow_mut()
            .begin_test_collection(revision, package);
        let Some(ticket) = ticket else {
            log::debug!("collect_tests: no engine current with the sources");
            return;
        };
        let playground = Rc::clone(&self.playground);
        let sender = self.playground_sender.clone();
        wasm_bindgen_futures::spawn_local(async move {
            collect_tests(&playground, &sender, ticket, project).await;
        });
    }

    /// Expand one lazy testset in place and re-send the tree.
    ///
    /// `generation` is a `u32` because `u64` crosses the wasm ABI as a JS
    /// `bigint`, and the wire protocol declares a plain number — a `bigint`
    /// parameter would reject every call the worker makes.
    #[wasm_bindgen(js_name = expandTestSet)]
    pub fn expand_test_set(&self, project: String, generation: u32, testset_name: String) {
        if self.is_unavailable() {
            return;
        }
        if !self.serves(&project) {
            return;
        }
        let generation = u64::from(generation);
        let revision = self.state.borrow().revision();
        let lease = self
            .playground
            .borrow()
            .lease_registry(generation, revision);
        let lease = match lease {
            Ok(lease) => lease,
            Err(error) => {
                log::info!("not expanding '{testset_name}': {error}");
                return;
            }
        };
        let playground = Rc::clone(&self.playground);
        let sender = self.playground_sender.clone();
        wasm_bindgen_futures::spawn_local(async move {
            expand_test_set(
                &playground,
                &sender,
                lease,
                project,
                generation,
                testset_name,
            )
            .await;
        });
    }

    /// Drop the session's state and drain its engine. The JS object itself is
    /// freed by `wasm_bindgen`.
    #[wasm_bindgen(js_name = closeSession)]
    pub fn close_session(&self) {
        if self.is_unavailable() {
            return;
        }
        self.state.borrow_mut().close_session(self.session);
        self.playground.borrow_mut().shutdown();
    }
}

impl BamlWasmRuntime {
    /// Whether `project` names the workspace this runtime hosts.
    ///
    /// One runtime, one workspace: a request naming anything else is a host
    /// bug, and answering it with this workspace's data would be worse than
    /// answering nothing.
    fn serves(&self, project: &str) -> bool {
        let Some((root, _)) = playground::workspace_root(&self.state.borrow()) else {
            return false;
        };
        if root == project {
            return true;
        }
        log::warn!("ignoring a playground request for {project}; this runtime hosts {root}");
        false
    }

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

/// One collection attempt: ask the engine for the registry, install it if it
/// is still the current build's, then serialize the tree.
///
/// Every emission is fenced by the ticket. A rebuild between the engine call
/// and the answer means the tree describes code that no longer exists, and
/// the newer build's own collection owns the UI.
async fn collect_tests(
    playground: &Rc<RefCell<playground::PlaygroundState>>,
    sender: &playground_notify::WasmPlaygroundSender,
    ticket: playground::CollectionTicket,
    project: String,
) {
    let call_id = sys_types::CallId::next();
    let generation = ticket.generation;
    let send_tree = |data: Vec<u8>, expand_error| {
        sender.send(
            &playground_notify::PlaygroundNotification::TestCollectionResult {
                project: project.clone(),
                generation,
                call_id: call_id.0,
                data,
                expand_error,
            },
        );
    };

    let registry = match ticket
        .engine
        .collect_tests(
            &ticket.package,
            call_id,
            sys_types::CancellationToken::new(),
        )
        .await
    {
        Ok(registry) => registry,
        Err(error) => {
            // A failure for the still-current build unblocks the frontend with
            // an empty tree instead of a spinner that never resolves.
            log::error!("collect_tests failed: {error}");
            if playground.borrow().ticket_is_current(&ticket) {
                send_tree(empty_tree(), None);
            }
            return;
        }
    };

    // Null means the project has no tests (`$init_test` absent), which is a
    // different state from "not collected yet".
    let handle = match &registry {
        bex_project::BexExternalValue::Handle(handle) => Some(handle.clone()),
        bex_project::BexExternalValue::Null => None,
        other => {
            log::error!("collect_tests returned an unexpected value: {other:?}");
            return;
        }
    };
    let has_tests = handle.is_some();
    if !playground
        .borrow_mut()
        .install_collected_registry(&ticket, handle)
    {
        log::debug!("collect_tests: discarding a stale result (generation {generation})");
        return;
    }
    if !has_tests {
        send_tree(empty_tree(), None);
        return;
    }

    let data = serialize_registry(&ticket.engine, registry).await;
    // Fenced again: a rebuild during serialization means the newer build owns
    // the tree.
    if playground.borrow().ticket_is_current(&ticket) {
        send_tree(data, None);
    }
}

/// Expand one testset, then re-send the tree either way — on failure the
/// pre-expansion tree is what unblocks the UI.
async fn expand_test_set(
    playground: &Rc<RefCell<playground::PlaygroundState>>,
    sender: &playground_notify::WasmPlaygroundSender,
    lease: playground::RegistryLease,
    project: String,
    generation: u64,
    testset_name: String,
) {
    // Expansions mutate the registry object in place: one owner at a time.
    let _mutation_owner = lease.expansion_gate.lock().await;

    let call_id = sys_types::CallId::next();
    let registry_value = bex_project::BexExternalValue::Handle(lease.handle.clone());
    let context = bex_project::FunctionCallContextBuilder::new(call_id)
        .suppress_internal_profile()
        .build();
    let expand_error = match lease
        .engine
        .call_function(
            "testing.TestRegistry.expand_set",
            vec![registry_value.clone(), testset_name.as_str().into()],
            context,
            true,
        )
        .await
    {
        Ok(_) => None,
        Err(error) => {
            log::error!("expanding testset '{testset_name}' failed: {error}");
            Some(playground_notify::TestExpandError {
                testset_name: testset_name.clone(),
                message: error.to_string(),
            })
        }
    };

    let data = serialize_registry(&lease.engine, registry_value).await;
    if playground.borrow().lease_is_current(&lease) {
        sender.send(
            &playground_notify::PlaygroundNotification::TestCollectionResult {
                project,
                generation,
                call_id: call_id.0,
                data,
                expand_error,
            },
        );
    }
}

/// Serialize a registry handle to the tree the host renders.
async fn serialize_registry(
    engine: &Arc<bex_project::BexEngine>,
    registry: bex_project::BexExternalValue,
) -> Vec<u8> {
    let context = bex_project::FunctionCallContextBuilder::new(sys_types::CallId::next())
        .suppress_internal_profile()
        .build();
    match engine
        .call_function(
            "testing.TestRegistry.serialize",
            vec![registry],
            context,
            true,
        )
        .await
    {
        Ok(serialized) => serde_json::to_vec(&playground::bex_value_to_json(&serialized))
            .unwrap_or_else(|_| empty_tree()),
        Err(error) => {
            log::error!("serializing the test tree failed: {error}");
            empty_tree()
        }
    }
}

fn empty_tree() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!([])).unwrap_or_default()
}
