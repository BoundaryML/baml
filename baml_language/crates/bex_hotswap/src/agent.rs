//! A process's side of hot swapping: register with BCS, install what it
//! sends, report each outcome, and hold a connection open for more.
//!
//! The agent runs on its own thread, so the program serves while it works.
//! Every request lists the installed swaps, and every response may carry
//! notifications: `active` offers a revision the process lacks, or renews
//! one it has; `disabled` and `deleted` remove one. A swap BCS has not renewed
//! for an hour is removed here, without BCS, and reported when BCS is back.

use std::{
    collections::BTreeMap,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use bcs_api::{
    Client,
    credentials::RequestAuthorization,
    hotswaps::{Delivery, RegisterProcess, SwapOutcome},
};
use bex_engine::BexEngine;

use crate::{HotswapBuild, Swapper};

/// How long BCS holds an idle connection when it does not say.
const DEFAULT_HOLD: Duration = Duration::from_secs(300);
/// The wait after a request to BCS fails, before the next.
const RETRY: Duration = Duration::from_secs(5);
/// The longest the agent holds a connection, whatever BCS asks, so it checks
/// for lapsed swaps at least this often.
const MAX_HOLD: Duration = Duration::from_secs(300);
/// Outcomes kept for BCS while it cannot take them; older ones are dropped.
const MAX_PENDING: usize = 1000;

/// Where and as whom a process talks to BCS: the endpoint and credential its
/// telemetry uses.
pub struct CloudLink {
    pub client: Client,
    pub authorization: RequestAuthorization,
}

/// What the agent reports about itself.
pub struct AgentOptions {
    pub process_id: String,
    pub baml_version: String,
    pub host: String,
    /// See `Swapper::with_lapse`; an hour unless a test shortens it.
    pub lapse: Option<Duration>,
    /// Retry and hold faster, for tests.
    pub retry: Option<Duration>,
}

/// The running agent. [`Agent::stop`], or dropping it, ends it at its next
/// request and tells BCS the process is stopping, so no rollout waits on it.
pub struct Agent {
    ready: Arc<(Mutex<bool>, Condvar)>,
    stop: Arc<AtomicBool>,
    link: Arc<CloudLink>,
    process_id: String,
}

impl Agent {
    /// Wait up to `wait` for the swaps BCS returns at registration to
    /// install, as `startup_wait` asks. Returns whether they did.
    pub fn wait_ready(&self, wait: Duration) -> bool {
        if wait.is_zero() {
            return *self.ready.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        let (lock, ready) = &*self.ready;
        let guard = lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (guard, _) = ready
            .wait_timeout_while(guard, wait, |ready| !*ready)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard
    }

    pub fn stop(&self) {
        if self.stop.swap(true, Ordering::Relaxed) {
            return;
        }
        // On a thread of its own: the blocking client cannot run inside an
        // async runtime, which a host may be in.
        let (link, process_id) = (Arc::clone(&self.link), self.process_id.clone());
        let told = std::thread::spawn(move || {
            link.client
                .process_stopped(&link.authorization, &process_id)
        })
        .join();
        if let Ok(Err(error)) = told {
            tracing::debug!(%error, "BCS did not hear that the process stopped");
        }
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Start the agent for `engine`, built from `build`.
pub fn start(
    engine: Arc<BexEngine>,
    build: Arc<HotswapBuild>,
    link: CloudLink,
    options: AgentOptions,
) -> std::io::Result<Agent> {
    let ready = Arc::new((Mutex::new(false), Condvar::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let link = Arc::new(link);
    let agent = Agent {
        ready: Arc::clone(&ready),
        stop: Arc::clone(&stop),
        link: Arc::clone(&link),
        process_id: options.process_id.clone(),
    };
    std::thread::Builder::new()
        .name("baml-hotswap".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    tracing::warn!(%error, "hot swapping is off: no runtime");
                    signal(&ready);
                    return;
                }
            };
            let mut swapper = Swapper::new(Arc::clone(&engine), Arc::clone(&build));
            if let Some(lapse) = options.lapse {
                swapper = swapper.with_lapse(lapse);
            }
            let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                Loop {
                    swapper,
                    link,
                    options,
                    runtime,
                    ready: Arc::clone(&ready),
                    stop,
                    pending: Vec::new(),
                    rejected: BTreeMap::new(),
                }
                .run();
            }));
            if ran.is_err() {
                // Nothing will renew or remove the swaps now: return every
                // function to the code as built rather than run unvouched code.
                tracing::error!("the hot swap agent panicked; removing every swap");
                let files = build
                    .sources
                    .iter()
                    .map(|(path, text)| (path.clone(), text.clone()))
                    .collect();
                if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    let _ = runtime.block_on(engine.hotswap_patch_named(files, &BTreeMap::new()));
                }
                signal(&ready);
            }
        })?;
    Ok(agent)
}

fn signal(ready: &(Mutex<bool>, Condvar)) {
    *ready.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = true;
    ready.1.notify_all();
}

struct Loop {
    swapper: Swapper,
    link: Arc<CloudLink>,
    options: AgentOptions,
    runtime: tokio::runtime::Runtime,
    ready: Arc<(Mutex<bool>, Condvar)>,
    stop: Arc<AtomicBool>,
    /// Outcomes BCS has not acknowledged, oldest first.
    pending: Vec<(String, SwapOutcome)>,
    /// The revision of each swap this process rejected: offered again, it is
    /// not fetched again.
    rejected: BTreeMap<String, u64>,
}

impl Loop {
    fn retry(&self) -> Duration {
        self.options.retry.unwrap_or(RETRY)
    }

    fn run(mut self) {
        // A process registers again whenever BCS no longer knows it, such as after its
        // database was reset.
        while !self.stop.load(Ordering::Relaxed) {
            let Some(delivery) = self.register() else {
                break;
            };
            self.serve(delivery);
        }
        signal(&self.ready);
    }

    /// Register until BCS answers; a process serves as built meanwhile. `None`
    /// once the agent is stopped.
    fn register(&mut self) -> Option<Delivery> {
        loop {
            if self.stop.load(Ordering::Relaxed) {
                return None;
            }
            let request = RegisterProcess {
                process_id: self.options.process_id.clone(),
                commit: self.swapper_build_commit(),
                baml_version: self.options.baml_version.clone(),
                host: self.options.host.clone(),
                installed: self.swapper.installed(),
            };
            match self
                .link
                .client
                .register_process(&self.link.authorization, &request)
            {
                Ok(delivery) => return Some(delivery),
                Err(error) => {
                    tracing::debug!(%error, "hot swap registration failed; retrying");
                    signal(&self.ready);
                    // Unreachable BCS renews nothing: swaps still lapse.
                    self.lapse();
                    std::thread::sleep(self.retry());
                }
            }
        }
    }

    /// Act on deliveries and hold the connection for more, until stopped or
    /// until BCS answers that it does not know the process.
    fn serve(&mut self, mut delivery: Delivery) {
        let mut hold = DEFAULT_HOLD;
        let mut first = true;
        while !self.stop.load(Ordering::Relaxed) {
            if let Some(seconds) = delivery.hold_seconds {
                hold = Duration::from_secs(seconds).min(MAX_HOLD);
            }
            self.handle(delivery);
            if first {
                signal(&self.ready);
                first = false;
            }
            self.lapse();
            self.flush();
            if self.stop.load(Ordering::Relaxed) {
                return;
            }
            delivery = match self.link.client.wait_for_notifications(
                &self.link.authorization,
                &self.options.process_id,
                &self.swapper.installed(),
                hold,
            ) {
                Ok(delivery) => delivery,
                Err(error) if unknown_process(&error) => return,
                Err(error) => {
                    tracing::debug!(%error, "hot swap notifications failed; retrying");
                    std::thread::sleep(self.retry());
                    Delivery::default()
                }
            };
        }
    }

    fn swapper_build_commit(&self) -> Option<String> {
        self.swapper_build().commit.clone()
    }

    fn swapper_build(&self) -> &HotswapBuild {
        self.swapper.build()
    }

    /// Install what `delivery` carries and act on its notifications, fetching
    /// every revision the process lacks in one request.
    fn handle(&mut self, delivery: Delivery) {
        for swap in delivery.swaps {
            let outcome = match self.runtime.block_on(self.swapper.install(&swap)) {
                Ok(_) => SwapOutcome {
                    revision: swap.revision,
                    status: "installed".into(),
                    reason: None,
                },
                Err(error) => {
                    self.rejected.insert(swap.id.clone(), swap.revision);
                    SwapOutcome {
                        revision: swap.revision,
                        status: "rejected".into(),
                        reason: Some(error.0),
                    }
                }
            };
            self.report(swap.id.clone(), outcome);
            for retired in self.swapper.take_retired() {
                self.report(
                    retired.id,
                    SwapOutcome {
                        revision: retired.revision,
                        status: "removed".into(),
                        reason: Some(format!("replaced by {}", swap.id)),
                    },
                );
            }
        }
        let mut fetch = Vec::new();
        for notification in delivery.notifications {
            if notification.kind != "hotswap" {
                continue;
            }
            match notification.status.as_str() {
                // Retired here by its replacement; BCS retires it once every
                // process has the replacement.
                _ if self.swapper.is_retired(&notification.id) => {}
                "active"
                    if self
                        .swapper
                        .is_installed(&notification.id, notification.revision) =>
                {
                    self.swapper.renew(&notification.id);
                }
                // Rejected at this revision already: the answer would not change.
                "active"
                    if self
                        .rejected
                        .get(&notification.id)
                        .is_some_and(|&revision| revision >= notification.revision) => {}
                "active" => fetch.push(notification.id),
                // A notice older than the installed revision is stale.
                "disabled" | "deleted"
                    if self
                        .swapper
                        .revision(&notification.id)
                        .is_some_and(|revision| revision <= notification.revision) =>
                {
                    match self
                        .runtime
                        .block_on(self.swapper.remove(&notification.id, &notification.status))
                    {
                        Ok(()) => self.report(
                            notification.id,
                            SwapOutcome {
                                revision: notification.revision,
                                status: "removed".into(),
                                reason: None,
                            },
                        ),
                        // Still running: the notice repeats, and the swap lapses
                        // without renewal if removal keeps failing.
                        Err(error) => tracing::warn!(
                            %error,
                            hotswap = notification.id,
                            "a removed swap was not removed"
                        ),
                    }
                }
                _ => {}
            }
        }
        if fetch.is_empty() {
            return;
        }
        match self.link.client.fetch_swaps(
            &self.link.authorization,
            &self.options.process_id,
            &self.swapper.installed(),
            &fetch,
        ) {
            // Only the swaps: notifications on a fetch would fetch again.
            Ok(fetched) => self.handle(Delivery {
                swaps: fetched.swaps,
                ..Delivery::default()
            }),
            Err(error) => tracing::debug!(%error, "hot swap fetch failed; the next notice retries"),
        }
    }

    /// Remove every swap BCS has not renewed within the lapse.
    fn lapse(&mut self) {
        for id in self.swapper.lapsed() {
            let revision = self
                .swapper
                .installed()
                .into_iter()
                .find(|installed| installed.id == id)
                .map_or(0, |installed| installed.revision);
            // A failed removal leaves the swap running; the next pass tries again.
            match self.runtime.block_on(self.swapper.remove(&id, "lapsed")) {
                Ok(()) => self.report(
                    id,
                    SwapOutcome {
                        revision,
                        status: "lapsed".into(),
                        reason: None,
                    },
                ),
                Err(error) => tracing::warn!(%error, hotswap = id, "a lapsed swap was not removed"),
            }
        }
    }

    /// Queue an outcome for BCS, keeping the newest when too many wait.
    fn report(&mut self, id: String, outcome: SwapOutcome) {
        if self.pending.len() >= MAX_PENDING {
            self.pending.remove(0);
        }
        self.pending.push((id, outcome));
    }

    /// Report outcomes in order, each in its own request, until BCS cannot be
    /// reached. One BCS refuses for good is dropped, so it blocks no other.
    fn flush(&mut self) {
        while let Some((id, outcome)) = self.pending.first().cloned() {
            match self.link.client.report_swap(
                &self.link.authorization,
                &self.options.process_id,
                &id,
                &self.swapper.installed(),
                &outcome,
            ) {
                Ok(delivery) => {
                    self.pending.remove(0);
                    // Renewals and notices ride on this response too.
                    if !delivery.notifications.is_empty() || !delivery.swaps.is_empty() {
                        self.handle(delivery);
                    }
                }
                Err(error) if refused(&error) => {
                    tracing::warn!(%error, hotswap = id, "BCS refused a hot swap outcome");
                    self.pending.remove(0);
                }
                Err(error) => {
                    tracing::debug!(%error, "hot swap outcome not reported; retrying later");
                    return;
                }
            }
        }
    }
}

/// Whether BCS refused a request for good: a client error other than an
/// expired credential or too many requests, or a request never sent because
/// it was malformed.
fn refused(error: &bcs_api::Error) -> bool {
    match error.http_failure() {
        Some(failure) => {
            let status = failure.status.as_u16();
            (400..500).contains(&status) && !matches!(status, 401 | 408 | 429)
        }
        None => matches!(error, bcs_api::Error::Protocol(_)),
    }
}

/// Whether BCS answered that it does not know the process.
fn unknown_process(error: &bcs_api::Error) -> bool {
    error
        .http_failure()
        .is_some_and(|failure| failure.status.as_u16() == 404)
}
