use crate::server::api;
use lsp_types::{DidChangeTextDocumentParams, Url};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use crate::session::Session;

mod task;
mod thread;

pub(super) use task::{BackgroundSchedule, Task};

use self::{
    task::{BackgroundTaskBuilder, SyncTask},
    thread::ThreadPriority,
};

use super::{client::Client, ClientSender};

/// The event loop thread is actually a secondary thread that we spawn from the
/// _actual_ main thread. This secondary thread has a larger stack size
/// than some OS defaults (Windows, for example) and is also designated as
/// high-priority.
pub(crate) fn event_loop_thread(
    func: impl FnOnce() -> anyhow::Result<()> + Send + 'static,
) -> anyhow::Result<thread::JoinHandle<anyhow::Result<()>>> {
    // Override OS defaults to avoid stack overflows on platforms with low stack size defaults.
    const MAIN_THREAD_STACK_SIZE: usize = 2 * 1024 * 1024;
    const MAIN_THREAD_NAME: &str = "baml:main";
    Ok(
        thread::Builder::new(thread::ThreadPriority::LatencySensitive)
            .name(MAIN_THREAD_NAME.into())
            .stack_size(MAIN_THREAD_STACK_SIZE)
            .spawn(func)?,
    )
}

pub(crate) struct Scheduler<'s> {
    session: &'s mut Session,
    client: Client<'s>,
    fmt_pool: thread::Pool,
    background_pool: thread::Pool,
    pending_changes: HashMap<Url, (Instant, DidChangeTextDocumentParams)>,
    debounce_duration: Duration,
}

impl<'s> Scheduler<'s> {
    pub(super) fn new(
        session: &'s mut Session,
        worker_threads: NonZeroUsize,
        sender: ClientSender,
    ) -> Self {
        const FMT_THREADS: usize = 1;
        Self {
            session,
            fmt_pool: thread::Pool::new(NonZeroUsize::try_from(FMT_THREADS).unwrap()),
            background_pool: thread::Pool::new(worker_threads),
            client: Client::new(sender),
            pending_changes: HashMap::new(),
            debounce_duration: Duration::from_millis(50),
        }
    }

    /// Immediately sends a request of kind `R` to the client, with associated parameters.
    /// The task provided by `response_handler` will be dispatched as soon as the response
    /// comes back from the client.
    pub(super) fn request<R>(
        &mut self,
        params: R::Params,
        response_handler: impl Fn(R::Result) -> Task<'s> + 'static,
    ) -> anyhow::Result<()>
    where
        R: lsp_types::request::Request,
    {
        self.client.requester.request::<R>(params, response_handler)
    }

    /// Creates a task to handle a response from the client.
    pub(super) fn response(&mut self, response: lsp_server::Response) -> Task<'s> {
        self.client.requester.pop_response_task(response)
    }

    /// Dispatches a `task` by either running it as a blocking function or
    /// executing it on a background thread pool.
    pub(super) fn dispatch(&mut self, task: task::Task<'s>) {
        match task {
            Task::Sync(SyncTask { func }) => {
                let notifier = self.client.notifier();
                let responder = self.client.responder();
                func(
                    self.session,
                    notifier,
                    &mut self.client.requester,
                    responder,
                );
            }
            Task::Background(BackgroundTaskBuilder {
                schedule,
                builder: func,
            }) => {
                let static_func = func(self.session);
                let notifier = self.client.notifier();
                let responder = self.client.responder();
                let task = move || static_func(notifier, responder);
                match schedule {
                    BackgroundSchedule::Worker => {
                        self.background_pool.spawn(ThreadPriority::Worker, task);
                    }
                    BackgroundSchedule::LatencySensitive => self
                        .background_pool
                        .spawn(ThreadPriority::LatencySensitive, task),
                    BackgroundSchedule::Fmt => {
                        self.fmt_pool.spawn(ThreadPriority::LatencySensitive, task);
                    }
                }
            }
        }
    }

    /// Registers a document change, resetting the debounce timer for the given URL.
    pub(crate) fn register_pending_change(
        &mut self,
        url: Url,
        params: DidChangeTextDocumentParams,
    ) {
        let now = Instant::now();
        self.pending_changes.insert(url.clone(), (now, params));
        tracing::trace!("Registered pending change for {}", url.clone().as_str());
    }

    /// Checks for pending changes whose debounce timers have expired and dispatches tasks for them.
    pub(crate) fn process_ready_changes(&mut self) {
        let now = Instant::now();
        let ready_urls: Vec<Url> = self
            .pending_changes
            .iter()
            .filter_map(|(url, (timestamp, _))| {
                if now.duration_since(*timestamp) > self.debounce_duration {
                    Some(url.clone())
                } else {
                    None
                }
            })
            .collect();

        if !ready_urls.is_empty() {
            tracing::debug!("Processing {} debounced changes", ready_urls.len());
        }

        for url in ready_urls {
            if let Some((_timestamp, params)) = self.pending_changes.remove(&url) {
                tracing::debug!("Dispatching debounced change for {}", url.as_str());
                // Use the function from api.rs to create the necessary tasks
                let tasks = api::create_did_change_tasks(params);
                for task in tasks {
                    // Dispatch the task using the scheduler's existing dispatch logic
                    self.dispatch(task);
                }
            }
        }
    }
}
