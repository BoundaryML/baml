//! BEP-040 `Limit`: shared admission for spawned tasks.
//!
//! A `baml.spawn.Limit` value is one admission domain with a positive
//! capacity. A launch under a set of limits is admitted only when a slot in
//! every one of them can be taken together — it never holds one while waiting
//! for another, so there is no AB/BA hold-and-wait — and holds them until its
//! attempt finishes or unwinds. Two launches share a domain iff they hold the
//! same `Arc<LimitInner>`; there is no registry. A limit is a pure admission
//! counter: cancellation is a token's job.
//!
//! Admission is the engine's: it registers a launch synchronously at the spawn
//! site ([`LimitSet::admit`]), so a lone limit's counts observe it at once, and
//! the launched task awaits the ticket ([`AdmissionTicket::acquire`]) before
//! its first attempt — without the heap permit, so a parked task never blocks
//! GC. The [`Admission`] releases every slot on drop and admits whoever it can.
//!
//! Locking: each limit has its own mutex, and whenever more than one is held
//! they are taken in address order — the order a [`LimitSet`] keeps — so
//! releases and admissions racing in different tasks cannot deadlock. A launch
//! needing several limits is queued in each and admitted by whichever release
//! finds every slot free; while queued it counts in none of them, so a lone
//! limit's `active_count` is exact and a multi-limit launch's is consistent
//! only once admitted.

use std::{
    collections::VecDeque,
    num::NonZeroUsize,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
};

use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

/// Shared admission state behind a `baml.spawn.Limit` value.
pub struct LimitInner {
    capacity: NonZeroUsize,
    state: Mutex<LimitState>,
}

impl std::fmt::Debug for LimitInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never lock `state` here: `Debug` may run while it is held.
        f.debug_struct("LimitInner")
            .field("capacity", &self.capacity)
            .finish_non_exhaustive()
    }
}

struct LimitState {
    /// Slots taken: launches admitted through this limit and not yet released.
    active: usize,
    /// Launches waiting on this limit, oldest first. One needing other limits
    /// too is queued in each of them and leaves all of them at once.
    waiters: VecDeque<Arc<Waiter>>,
}

/// Waiter ids are process-global: two waiters queued in one limit never
/// collide, whichever limits minted them.
static NEXT_WAITER_ID: AtomicU64 = AtomicU64::new(0);

/// One launch waiting for admission, queued in every limit it needs.
struct Waiter {
    id: u64,
    limits: LimitSet,
    /// Taken, under every limit's lock, by the release that admits this
    /// waiter; gone once admitted.
    grant: Mutex<Option<oneshot::Sender<Admission>>>,
}

/// What a release found when it tried to admit a queued waiter.
enum TryAdmit {
    /// Every slot was free: taken, and the waiter was told.
    Admitted,
    /// A slot was taken elsewhere, or the waiter had already left the queues.
    Skipped,
    /// Admitted, but nobody is waiting for the grant any more — the caller
    /// drops it, outside the locks, which releases the slots again.
    Abandoned(Admission),
}

/// The limits a launch is admitted through: deduplicated by identity and kept
/// in address order, which is the lock order.
#[derive(Clone, Debug, Default)]
pub struct LimitSet(Box<[Arc<LimitInner>]>);

/// Slots held in every limit of a set; released together on drop.
#[must_use = "dropping an admission releases its slots"]
pub struct Admission {
    limits: LimitSet,
}

/// The outcome of registering a launch: admitted at once, or parked until a
/// release can take every slot.
pub struct AdmissionTicket {
    state: TicketState,
}

enum TicketState {
    Admitted(Admission),
    Parked(ParkedWaiter),
}

/// A launch queued in its limits. Dropping it — cancelled, admitted, or
/// abandoned — leaves every queue holding nothing.
struct ParkedWaiter {
    waiter: Arc<Waiter>,
    granted: oneshot::Receiver<Admission>,
}

impl Drop for ParkedWaiter {
    fn drop(&mut self) {
        self.waiter.withdraw(&mut self.granted);
    }
}

fn address(limit: &Arc<LimitInner>) -> usize {
    Arc::as_ptr(limit).addr()
}

/// Lock every limit of a set, in the set's (address) order.
fn lock_all(limits: &[Arc<LimitInner>]) -> Vec<MutexGuard<'_, LimitState>> {
    debug_assert!(
        limits
            .windows(2)
            .all(|pair| address(&pair[0]) < address(&pair[1])),
        "a limit set is address-ordered"
    );
    limits.iter().map(|limit| limit.lock()).collect()
}

impl LimitInner {
    /// A limit admitting `capacity` launches at a time.
    pub fn new(capacity: NonZeroUsize) -> Arc<Self> {
        Arc::new(Self {
            capacity,
            state: Mutex::new(LimitState {
                active: 0,
                waiters: VecDeque::new(),
            }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, LimitState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// How many launches this limit admits at a time.
    pub fn capacity(&self) -> NonZeroUsize {
        self.capacity
    }

    /// Slots taken right now. Exact for launches admitted through this limit
    /// alone; a launch queued on several limits counts in none until admitted.
    pub fn active_count(&self) -> usize {
        self.lock().active
    }

    /// Launches queued on this limit right now.
    pub fn queued_count(&self) -> usize {
        self.lock().waiters.len()
    }

    /// A slot was released: admit queued waiters, oldest first, while a slot
    /// is free. A waiter whose other limits are full is bypassed, not waited
    /// for (BEP-040 promises no fairness across admission sets).
    fn promote(self: &Arc<Self>) {
        let candidates: Vec<Arc<Waiter>> = self.lock().waiters.iter().cloned().collect();
        for waiter in candidates {
            if self.lock().active >= self.capacity.get() {
                break;
            }
            match waiter.try_admit() {
                TryAdmit::Admitted | TryAdmit::Skipped => {}
                TryAdmit::Abandoned(admission) => drop(admission),
            }
        }
    }
}

impl LimitSet {
    /// No limits: every launch is admitted at once.
    pub fn new() -> Self {
        Self::default()
    }

    /// This set plus `limit`; the same limit again is the same set.
    #[must_use]
    pub fn with(&self, limit: &Arc<LimitInner>) -> Self {
        if self.contains(limit) {
            return self.clone();
        }
        let mut limits = self.0.to_vec();
        let at = limits.partition_point(|present| address(present) < address(limit));
        limits.insert(at, Arc::clone(limit));
        Self(limits.into_boxed_slice())
    }

    /// Whether `limit` (by identity) is in the set.
    pub fn contains(&self, limit: &Arc<LimitInner>) -> bool {
        self.0.iter().any(|present| Arc::ptr_eq(present, limit))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Arc<LimitInner>> {
        self.0.iter()
    }

    /// Register a launch: take a slot in every limit if all are free, else
    /// queue it in every limit. Synchronous, so a lone limit's counts observe
    /// the launch before its task has been polled.
    pub fn admit(&self) -> AdmissionTicket {
        let admitted = || AdmissionTicket {
            state: TicketState::Admitted(Admission {
                limits: self.clone(),
            }),
        };
        if self.is_empty() {
            return admitted();
        }
        let mut guards = lock_all(&self.0);
        if guards
            .iter()
            .zip(self.iter())
            .all(|(state, limit)| state.active < limit.capacity.get())
        {
            for state in &mut guards {
                state.active += 1;
            }
            return admitted();
        }
        let (grant, granted) = oneshot::channel();
        let waiter = Arc::new(Waiter {
            id: NEXT_WAITER_ID.fetch_add(1, Ordering::Relaxed),
            limits: self.clone(),
            grant: Mutex::new(Some(grant)),
        });
        for state in &mut guards {
            state.waiters.push_back(Arc::clone(&waiter));
        }
        AdmissionTicket {
            state: TicketState::Parked(ParkedWaiter { waiter, granted }),
        }
    }
}

impl Waiter {
    /// Admit this waiter if every slot it needs is free, under every limit's
    /// lock: the slots are taken, the waiter leaves every queue, and the grant
    /// is sent, all at once — so a waiter absent from the queues has its
    /// admission in the channel.
    fn try_admit(&self) -> TryAdmit {
        let mut guards = lock_all(&self.limits.0);
        let queued = guards[0].waiters.iter().any(|waiter| waiter.id == self.id);
        let free = guards
            .iter()
            .zip(self.limits.iter())
            .all(|(state, limit)| state.active < limit.capacity.get());
        if !queued || !free {
            return TryAdmit::Skipped;
        }
        for state in &mut guards {
            state.active += 1;
            state.waiters.retain(|waiter| waiter.id != self.id);
        }
        let grant = self
            .grant
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .unwrap_or_else(|| unreachable!("a queued waiter holds its grant"));
        match grant.send(Admission {
            limits: self.limits.clone(),
        }) {
            Ok(()) => TryAdmit::Admitted,
            Err(admission) => TryAdmit::Abandoned(admission),
        }
    }

    /// Leave every queue. If a release admitted this waiter first, its
    /// admission is in the channel unless it was received: take it and drop
    /// it, which releases the slots, so a launch that never runs holds
    /// nothing. Idempotent.
    fn withdraw(&self, granted: &mut oneshot::Receiver<Admission>) {
        {
            let mut guards = lock_all(&self.limits.0);
            for state in &mut guards {
                state.waiters.retain(|waiter| waiter.id != self.id);
            }
        }
        if let Ok(admission) = granted.try_recv() {
            drop(admission);
        }
    }
}

impl AdmissionTicket {
    /// Wait for admission. `None` when `cancel` fires first: the launch leaves
    /// every queue holding nothing, and its body never runs.
    pub async fn acquire(self, cancel: &CancellationToken) -> Option<Admission> {
        match self.state {
            TicketState::Admitted(admission) => Some(admission),
            TicketState::Parked(mut parked) => {
                tokio::select! {
                    // Cancel-biased: a launch cancelled while parked must not
                    // run even if a slot was granted in the same instant —
                    // dropping `parked` gives such a grant back.
                    biased;
                    () = cancel.cancelled() => None,
                    granted = &mut parked.granted => Some(granted.unwrap_or_else(|_| {
                        unreachable!("a queued waiter's grant is sent, never dropped")
                    })),
                }
            }
        }
    }
}

impl Drop for Admission {
    fn drop(&mut self) {
        // Release every slot first, then admit: a waiter needing two of these
        // limits sees both slots free.
        for limit in self.limits.iter() {
            limit.lock().active -= 1;
        }
        for limit in self.limits.iter() {
            limit.promote();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        pin::pin,
        task::{Context, Poll, Waker},
    };

    use super::*;

    fn limit(capacity: usize) -> Arc<LimitInner> {
        LimitInner::new(NonZeroUsize::new(capacity).unwrap())
    }

    fn set(limits: &[&Arc<LimitInner>]) -> LimitSet {
        limits
            .iter()
            .fold(LimitSet::new(), |set, limit| set.with(limit))
    }

    /// Poll once with a no-op waker: admission never needs a runtime, so a
    /// test observes "still parked" without a timer.
    fn poll_once<F: Future>(future: &mut std::pin::Pin<&mut F>) -> Poll<F::Output> {
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
    }

    fn admitted(ticket: AdmissionTicket) -> Admission {
        let token = CancellationToken::new();
        let mut future = pin!(ticket.acquire(&token));
        match poll_once(&mut future) {
            Poll::Ready(Some(admission)) => admission,
            Poll::Ready(None) => panic!("cancelled"),
            Poll::Pending => panic!("parked"),
        }
    }

    #[test]
    fn a_lone_limit_admits_to_capacity_then_parks_in_order() {
        let a = limit(2);
        let first = admitted(set(&[&a]).admit());
        let second = admitted(set(&[&a]).admit());
        let third = set(&[&a]).admit();
        let fourth = set(&[&a]).admit();
        assert_eq!((a.active_count(), a.queued_count()), (2, 2));

        let token = CancellationToken::new();
        let mut third = pin!(third.acquire(&token));
        let mut fourth = pin!(fourth.acquire(&token));
        assert!(poll_once(&mut third).is_pending());
        assert!(poll_once(&mut fourth).is_pending());

        drop(first);
        let Poll::Ready(Some(third_admission)) = poll_once(&mut third) else {
            panic!("the freed slot goes to the oldest waiter")
        };
        assert!(poll_once(&mut fourth).is_pending(), "oldest first");
        assert_eq!((a.active_count(), a.queued_count()), (2, 1));

        drop(second);
        let Poll::Ready(Some(fourth_admission)) = poll_once(&mut fourth) else {
            panic!("the next freed slot goes to the next waiter")
        };
        assert_eq!((a.active_count(), a.queued_count()), (2, 0));
        drop((third_admission, fourth_admission));
        assert_eq!(a.active_count(), 0);
    }

    #[test]
    fn several_limits_are_taken_together_or_not_at_all() {
        let a = limit(1);
        let b = limit(1);
        let hold_a = admitted(set(&[&a]).admit());

        let both = set(&[&a, &b]).admit();
        let token = CancellationToken::new();
        let mut both = pin!(both.acquire(&token));
        assert!(poll_once(&mut both).is_pending());
        // Parked on both, holding neither: B is still free for a B-only launch.
        assert_eq!((a.active_count(), a.queued_count()), (1, 1));
        assert_eq!((b.active_count(), b.queued_count()), (0, 1));
        let hold_b = admitted(set(&[&b]).admit());

        drop(hold_a);
        assert!(poll_once(&mut both).is_pending(), "B is still taken");
        assert_eq!(a.active_count(), 0, "A is not held while waiting for B");

        drop(hold_b);
        let Poll::Ready(Some(admission)) = poll_once(&mut both) else {
            panic!("admitted once both slots are free")
        };
        assert_eq!((a.active_count(), b.active_count()), (1, 1));
        assert_eq!((a.queued_count(), b.queued_count()), (0, 0));
        drop(admission);
        assert_eq!((a.active_count(), b.active_count()), (0, 0));
    }

    #[test]
    fn opposite_orders_cannot_deadlock() {
        let a = limit(1);
        let b = limit(1);
        let hold_a = admitted(set(&[&a]).admit());
        let hold_b = admitted(set(&[&b]).admit());

        let ab = set(&[&a, &b]);
        let ba = set(&[&b, &a]);
        assert!(
            ab.iter().zip(ba.iter()).all(|(x, y)| Arc::ptr_eq(x, y)),
            "one canonical order, whichever the user wrote"
        );
        let token = CancellationToken::new();
        let mut first = pin!(ab.admit().acquire(&token));
        let mut second = pin!(ba.admit().acquire(&token));
        assert!(poll_once(&mut first).is_pending());
        assert!(poll_once(&mut second).is_pending());

        drop(hold_a);
        assert!(poll_once(&mut first).is_pending());
        assert!(poll_once(&mut second).is_pending());
        drop(hold_b);
        let Poll::Ready(Some(first_admission)) = poll_once(&mut first) else {
            panic!("the older waiter is admitted first")
        };
        assert!(poll_once(&mut second).is_pending());
        drop(first_admission);
        assert!(matches!(poll_once(&mut second), Poll::Ready(Some(_))));
    }

    #[test]
    fn the_same_limit_twice_is_one_slot() {
        let a = limit(1);
        let twice = set(&[&a, &a]);
        assert_eq!(twice.len(), 1);
        let admission = admitted(twice.admit());
        assert_eq!(a.active_count(), 1);
        drop(admission);
        assert_eq!(a.active_count(), 0);
    }

    #[test]
    fn a_cancelled_waiter_leaves_holding_nothing() {
        let a = limit(1);
        let hold = admitted(set(&[&a]).admit());
        let token = CancellationToken::new();
        let mut parked = pin!(set(&[&a]).admit().acquire(&token));
        assert!(poll_once(&mut parked).is_pending());
        assert_eq!(a.queued_count(), 1);

        token.cancel();
        assert!(matches!(poll_once(&mut parked), Poll::Ready(None)));
        assert_eq!((a.active_count(), a.queued_count()), (1, 0));
        drop(hold);
        assert_eq!(a.active_count(), 0, "nothing phantom was admitted");
    }

    #[test]
    fn a_grant_racing_a_cancellation_is_given_back() {
        let a = limit(1);
        let hold = admitted(set(&[&a]).admit());
        let token = CancellationToken::new();
        let mut parked = pin!(set(&[&a]).admit().acquire(&token));
        assert!(poll_once(&mut parked).is_pending());

        // The release admits the waiter before it observes its cancellation.
        drop(hold);
        assert_eq!(a.active_count(), 1, "granted");
        token.cancel();
        assert!(matches!(poll_once(&mut parked), Poll::Ready(None)));
        assert_eq!(a.active_count(), 0, "the granted slot was released");
    }

    #[test]
    fn overlapping_multi_limit_waiters_keep_their_own_wakeups() {
        let a = limit(1);
        let b = limit(1);
        let hold = admitted(set(&[&a, &b]).admit());
        let token = CancellationToken::new();
        let mut first = pin!(set(&[&a, &b]).admit().acquire(&token));
        let mut second = pin!(set(&[&a, &b]).admit().acquire(&token));
        assert!(poll_once(&mut first).is_pending());
        assert!(poll_once(&mut second).is_pending());
        assert_eq!((a.queued_count(), b.queued_count()), (2, 2));

        drop(hold);
        let Poll::Ready(Some(first_admission)) = poll_once(&mut first) else {
            panic!("the older waiter is admitted")
        };
        assert!(poll_once(&mut second).is_pending());
        assert_eq!((a.queued_count(), b.queued_count()), (1, 1));

        drop(first_admission);
        assert!(
            matches!(poll_once(&mut second), Poll::Ready(Some(_))),
            "the second waiter's wakeup was not lost to the first's"
        );
        assert_eq!((a.queued_count(), b.queued_count()), (0, 0));
    }

    #[test]
    fn a_parked_ticket_dropped_unpolled_leaves_the_queue() {
        let a = limit(1);
        let hold = admitted(set(&[&a]).admit());
        let parked = set(&[&a]).admit();
        assert_eq!(a.queued_count(), 1);
        drop(parked);
        assert_eq!(a.queued_count(), 0);

        // Granted, then dropped without ever being polled: the slot comes back.
        let parked = set(&[&a]).admit();
        drop(hold);
        assert_eq!(a.active_count(), 1, "granted to the parked launch");
        drop(parked);
        assert_eq!(a.active_count(), 0);
    }

    #[test]
    fn no_limits_admit_at_once() {
        let admission = admitted(LimitSet::new().admit());
        drop(admission);
    }
}
