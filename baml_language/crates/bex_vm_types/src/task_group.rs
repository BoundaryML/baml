//! BEP-034 `TaskGroup`: value-based rate limiting for spawns.
//!
//! A `TaskGroup` caps how many spawns referencing it run concurrently. Excess
//! spawns queue FIFO and start as earlier ones settle; the `spawn` still
//! returns a `Future` immediately, so queueing is invisible to the caller.
//! Two spawns share a limit iff they hold the same `Arc<TaskGroupInner>` (the
//! Rust handle behind a `baml.spawn.TaskGroup` value) — there is no global
//! registry.
//!
//! Admission is the engine's responsibility: a spawned body task calls
//! [`TaskGroupTicket::acquire`] *before* running its body (and without holding
//! the heap permit, so a parked task doesn't block GC). The returned
//! [`TaskGroupPermit`] releases the slot on drop and wakes the next waiter.

use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
};

use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

/// Shared admission state behind a `baml.spawn.TaskGroup` value.
pub struct TaskGroupInner {
    name: Option<String>,
    state: Mutex<State>,
}

impl std::fmt::Debug for TaskGroupInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Don't lock `state` in `Debug` — it may be called from contexts that
        // already hold the lock, and a deadlock there would be nasty.
        f.debug_struct("TaskGroupInner")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

struct State {
    /// Max concurrently-active members. `0` pauses the group (admits nothing).
    limit: usize,
    /// Currently-active (running) members.
    active: usize,
    /// FIFO order of parked member ids waiting for a slot.
    waiters: VecDeque<u64>,
    /// All current members (queued + active), keyed by a monotonic id.
    members: BTreeMap<u64, Member>,
    next_id: u64,
}

struct Member {
    /// The member's effective cancel token (fired by `group.cancel(...)`).
    cancel: CancellationToken,
    /// `true` once admitted (running), `false` while parked.
    active: bool,
    /// Grant channel, present only while parked; the releaser takes it to
    /// hand the slot to this waiter.
    grant: Option<oneshot::Sender<()>>,
}

/// RAII slot held by a running member. Releasing it (on drop) frees the slot
/// and wakes the next FIFO waiter.
pub struct TaskGroupPermit {
    group: Arc<TaskGroupInner>,
    id: u64,
}

/// A member registered with a group (synchronously, at spawn time) that has not
/// yet been admitted. The spawned body task [`acquire`](TaskGroupTicket::acquire)s
/// it to await a concurrency slot.
pub struct TaskGroupTicket {
    group: Arc<TaskGroupInner>,
    id: u64,
    /// `None` when admitted at registration; `Some` while parked.
    rx: Option<oneshot::Receiver<()>>,
    /// The member's effective token (only consulted on the parked path).
    cancel: CancellationToken,
}

impl TaskGroupTicket {
    /// Await admission. Returns the permit (releasing the slot on drop), or
    /// `None` if the member was cancelled while parked — the caller then
    /// settles the future `Cancelled` and never runs the body.
    pub async fn acquire(self) -> Option<TaskGroupPermit> {
        let TaskGroupTicket {
            group,
            id,
            rx,
            cancel,
        } = self;
        let Some(rx) = rx else {
            // Admitted at registration — no parking.
            return Some(TaskGroupPermit { group, id });
        };
        tokio::select! {
            // Cancel-biased: if the member was cancelled (e.g. `group.cancel()`)
            // it must NOT run, even if a slot was granted in the same window —
            // `cancel_parked` returns any granted slot and promotes the next
            // waiter. Without this bias a cancelled-then-promoted member would
            // run its body anyway.
            biased;
            () = cancel.cancelled() => {
                group.cancel_parked(id);
                None
            }
            res = rx => match res {
                // Granted: the releaser already incremented `active`, marked
                // this member active, and removed it from `waiters`.
                Ok(()) => Some(TaskGroupPermit { group, id }),
                // Sender dropped (group torn down) — treat as cancellation.
                Err(_) => { group.cancel_parked(id); None }
            },
        }
    }
}

impl TaskGroupInner {
    /// Create a group with concurrency cap `limit` and an optional debug name.
    pub fn new(limit: usize, name: Option<String>) -> Arc<Self> {
        Arc::new(Self {
            name,
            state: Mutex::new(State {
                limit,
                active: 0,
                waiters: VecDeque::new(),
                members: BTreeMap::new(),
                next_id: 0,
            }),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("TaskGroup state poisoned")
    }

    /// Optional debug name (no semantic meaning).
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Current concurrency cap.
    pub fn limit(&self) -> usize {
        self.lock().limit
    }

    /// Instantaneous count of running members.
    pub fn active_count(&self) -> usize {
        self.lock().active
    }

    /// Instantaneous count of parked (queued) members.
    pub fn queued_count(&self) -> usize {
        self.lock().waiters.len()
    }

    /// Change the concurrency cap. Increasing admits parked waiters up to the
    /// new cap immediately; decreasing never preempts running members (new
    /// admissions simply wait); `0` pauses the group.
    pub fn set_limit(&self, limit: usize) {
        let mut st = self.lock();
        st.limit = limit;
        Self::promote(&mut st);
    }

    /// Fire the cancel tokens of the selected members. `pending` selects parked
    /// members, `active` selects running ones. Returns the number signalled.
    /// The group stays usable afterward (new spawns get fresh members). Tokens
    /// are collected under the lock and fired after releasing it, because
    /// firing a parked member's token re-enters the group (via `acquire`'s
    /// cancel path) to unqueue it.
    pub fn cancel(&self, pending: bool, active: bool) -> usize {
        let to_fire: Vec<CancellationToken> = {
            let st = self.lock();
            st.members
                .values()
                .filter(|m| if m.active { active } else { pending })
                .map(|m| m.cancel.clone())
                .collect()
        };
        let count = to_fire.len();
        for token in to_fire {
            token.cancel();
        }
        count
    }

    /// Register a member with the group **synchronously**, at spawn time, so
    /// that `cancel(...)`, `active_count()`, and `queued_count()` observe it
    /// immediately — before the spawned body task has been polled. Returns a
    /// [`TaskGroupTicket`] the body task then `acquire`s (awaits admission on).
    ///
    /// `cancel` is the spawn's effective token; `group.cancel(...)` fires it.
    pub fn register(self: &Arc<Self>, cancel: CancellationToken) -> TaskGroupTicket {
        let mut st = self.lock();
        let id = st.next_id;
        st.next_id += 1;
        if st.active < st.limit {
            // A slot is free — admit immediately (no parking needed).
            st.active += 1;
            st.members.insert(
                id,
                Member {
                    cancel,
                    active: true,
                    grant: None,
                },
            );
            TaskGroupTicket {
                group: Arc::clone(self),
                id,
                rx: None,
                cancel: CancellationToken::new(), // unused on the granted path
            }
        } else {
            // At capacity — park FIFO until a slot frees.
            let (tx, rx) = oneshot::channel();
            st.members.insert(
                id,
                Member {
                    cancel: cancel.clone(),
                    active: false,
                    grant: Some(tx),
                },
            );
            st.waiters.push_back(id);
            TaskGroupTicket {
                group: Arc::clone(self),
                id,
                rx: Some(rx),
                cancel,
            }
        }
    }

    /// Cancel path for a member that observed cancellation. If it had already
    /// been granted a slot (a grant raced the cancel), release the slot and
    /// promote the next waiter; otherwise just unqueue it.
    fn cancel_parked(&self, id: u64) {
        let mut st = self.lock();
        if let Some(member) = st.members.remove(&id) {
            if member.active {
                st.active -= 1;
                Self::promote(&mut st);
            } else {
                st.waiters.retain(|w| *w != id);
            }
        }
    }

    /// Wake parked waiters while there is spare capacity. Called under lock.
    fn promote(st: &mut State) {
        while st.active < st.limit {
            let Some(id) = st.waiters.pop_front() else {
                break;
            };
            let grant = match st.members.get_mut(&id) {
                Some(member) => {
                    member.active = true;
                    member.grant.take()
                }
                // Member already removed (cancelled); skip it.
                None => continue,
            };
            let Some(tx) = grant else { continue };
            st.active += 1;
            if tx.send(()).is_err() {
                // The waiter dropped its receiver (cancelled) between being
                // popped and the grant. Undo and try the next waiter.
                st.active -= 1;
                st.members.remove(&id);
            }
            // Keep promoting while there is spare capacity: a `set_limit`
            // raising the cap by N must admit up to N parked waiters
            // immediately (per its docstring), not just the first; the
            // `while` condition already stops a single-slot release (permit
            // drop / cancel) after one admission.
        }
    }
}

impl Drop for TaskGroupPermit {
    fn drop(&mut self) {
        let mut st = self.group.lock();
        if let Some(member) = st.members.remove(&self.id) {
            if member.active {
                st.active -= 1;
            }
        }
        TaskGroupInner::promote(&mut st);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn limit_three_grants_three_immediately() {
        let g = TaskGroupInner::new(3, None);
        let t1 = g.register(CancellationToken::new());
        let t2 = g.register(CancellationToken::new());
        let t3 = g.register(CancellationToken::new());
        assert_eq!(g.active_count(), 3, "all three admitted at register");
        assert_eq!(g.queued_count(), 0);
        let (p1, p2, p3) = (t1.acquire().await, t2.acquire().await, t3.acquire().await);
        assert!(p1.is_some() && p2.is_some() && p3.is_some());
        assert_eq!(g.active_count(), 3);
    }

    #[tokio::test]
    async fn limit_one_queues_then_promotes() {
        let g = TaskGroupInner::new(1, None);
        let t1 = g.register(CancellationToken::new());
        let _t2 = g.register(CancellationToken::new());
        let _t3 = g.register(CancellationToken::new());
        assert_eq!(g.active_count(), 1);
        assert_eq!(g.queued_count(), 2);
        let p1 = t1.acquire().await.expect("first admitted");
        drop(p1); // releasing promotes the next FIFO waiter
        assert_eq!(g.active_count(), 1, "next waiter promoted");
        assert_eq!(g.queued_count(), 1);
    }
}
