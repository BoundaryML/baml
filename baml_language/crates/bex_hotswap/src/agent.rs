//! A process's side of hot swapping: register with BCS, install what it
//! sends, report each outcome, and hold a connection open for more.
//!
//! The agent runs on its own thread, so the program serves while it works.
//! Every request lists the installed swaps, and every response may carry
//! notifications: `active` offers a revision the process lacks, or renews
//! one it has; `disabled` and `deleted` remove one. A swap BCS has not renewed
//! for an hour is removed here, without BCS, and reported when BCS is back.

use std::{
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

/// The running agent. Dropping it leaves the agent running for the life of
/// the process; [`Agent::stop`] ends it at its next request.
pub struct Agent {
    ready: Arc<(Mutex<bool>, Condvar)>,
    stop: Arc<AtomicBool>,
}

impl Agent {
    /// Wait up to `wait` for the swaps BCS returns at registration to
    /// install, as `startup_wait` asks. Returns whether they did.
    pub fn wait_ready(&self, wait: Duration) -> bool {
        if wait.is_zero() {
            return *self.ready.0.lock().unwrap_or_else(|e| e.into_inner());
        }
        let (lock, ready) = &*self.ready;
        let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let (guard, _) = ready
            .wait_timeout_while(guard, wait, |ready| !*ready)
            .unwrap_or_else(|e| e.into_inner());
        *guard
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
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
    let agent = Agent {
        ready: Arc::clone(&ready),
        stop: Arc::clone(&stop),
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
            let mut swapper = Swapper::new(engine, build);
            if let Some(lapse) = options.lapse {
                swapper = swapper.with_lapse(lapse);
            }
            Loop {
                swapper,
                link,
                options,
                runtime,
                ready,
                stop,
                pending: Vec::new(),
            }
            .run();
        })?;
    Ok(agent)
}

fn signal(ready: &(Mutex<bool>, Condvar)) {
    *ready.0.lock().unwrap_or_else(|e| e.into_inner()) = true;
    ready.1.notify_all();
}

struct Loop {
    swapper: Swapper,
    link: CloudLink,
    options: AgentOptions,
    runtime: tokio::runtime::Runtime,
    ready: Arc<(Mutex<bool>, Condvar)>,
    stop: Arc<AtomicBool>,
    /// Outcomes BCS has not acknowledged, oldest first.
    pending: Vec<(String, SwapOutcome)>,
}

impl Loop {
    fn retry(&self) -> Duration {
        self.options.retry.unwrap_or(RETRY)
    }

    fn run(mut self) {
        let mut hold = DEFAULT_HOLD;
        // Register until BCS answers; a process serves meanwhile.
        let mut delivery = loop {
            if self.stop.load(Ordering::Relaxed) {
                signal(&self.ready);
                return;
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
                Ok(delivery) => break delivery,
                Err(error) => {
                    tracing::debug!(%error, "hot swap registration failed; retrying");
                    // A process that cannot reach BCS serves as built.
                    signal(&self.ready);
                    std::thread::sleep(self.retry());
                }
            }
        };
        let mut first = true;
        while !self.stop.load(Ordering::Relaxed) {
            if let Some(seconds) = delivery.hold_seconds {
                hold = Duration::from_secs(seconds);
            }
            self.handle(delivery);
            if first {
                signal(&self.ready);
                first = false;
            }
            self.lapse();
            self.flush();
            if self.stop.load(Ordering::Relaxed) {
                break;
            }
            delivery = match self.link.client.wait_for_notifications(
                &self.link.authorization,
                &self.options.process_id,
                &self.swapper.installed(),
                hold,
            ) {
                Ok(delivery) => delivery,
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
                Err(error) => SwapOutcome {
                    revision: swap.revision,
                    status: "rejected".into(),
                    reason: Some(error.0),
                },
            };
            self.pending.push((swap.id.clone(), outcome));
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
                "active" => fetch.push(notification.id),
                "disabled" | "deleted" => {
                    if self.swapper.is_installed(&notification.id, 0) {
                        let result = self
                            .runtime
                            .block_on(self.swapper.remove(&notification.id, &notification.status));
                        self.pending.push((
                            notification.id,
                            SwapOutcome {
                                revision: notification.revision,
                                status: "removed".into(),
                                reason: result.err().map(|error| error.0),
                            },
                        ));
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
            let result = self.runtime.block_on(self.swapper.remove(&id, "lapsed"));
            self.pending.push((
                id,
                SwapOutcome {
                    revision,
                    status: "lapsed".into(),
                    reason: result.err().map(|error| error.0),
                },
            ));
        }
    }

    /// Report outcomes in order, each in its own request, until one fails.
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
                Err(error) => {
                    tracing::debug!(%error, "hot swap outcome not reported; retrying later");
                    return;
                }
            }
        }
    }
}
