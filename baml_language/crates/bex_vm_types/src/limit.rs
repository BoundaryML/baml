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
//! Fairness: a free slot goes to the oldest queued launch that can take it.
//! Every event that can make a launch admissible runs promotion — a release,
//! and an arrival that finds others already queued, which joins the back of
//! the queue rather than taking the slot — so an arrival racing a release
//! never takes the slot the release was about to hand on. A launch whose
//! other limits are full is passed over, not waited for (BEP-040 promises no
//! fairness across admission sets).
//!
//! Locking: each limit has its own mutex, and whenever more than one is held
//! they are taken in address order — the order a [`LimitSet`] keeps — so
//! releases and admissions racing in different tasks cannot deadlock. A launch
//! needing several limits is queued in each and admitted by whichever event
//! finds every slot free; while queued it counts in none of them, so a lone
//! limit's `active_count` is exact and a multi-limit launch's is consistent
//! only once admitted.
//!
//! Cost: launches needing the same limits queue together, oldest first, and
//! if the oldest of such a group cannot be admitted, none of the group can.
//! So on each limit it promotes, an arrival or a release examines the oldest
//! launch of each group queued there, plus one more per launch it admits,
//! however many launches each group holds; a fan-out whose launches share
//! their limits is one group. Joining or leaving a queue is logarithmic in its
//! length.

use std::{
    collections::{BTreeMap, HashMap},
    hash::{Hash, Hasher},
    num::NonZeroUsize,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
    task::Poll,
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
    /// Launches waiting on this limit, grouped by the limits they need, each
    /// group keyed by waiter id, which is arrival order. One needing other
    /// limits too is queued in each of them, in the same group, and leaves all
    /// of them at once. A group is removed when it empties.
    queues: HashMap<LimitSet, BTreeMap<u64, Arc<Waiter>>>,
}

impl LimitState {
    fn queued(&self) -> usize {
        self.queues.values().map(BTreeMap::len).sum()
    }

    fn enqueue(&mut self, waiter: &Arc<Waiter>) {
        self.queues
            .entry(waiter.limits.clone())
            .or_default()
            .insert(waiter.id, Arc::clone(waiter));
    }

    /// Take `waiter` out of this limit's queue; `false` if it was not queued.
    fn dequeue(&mut self, waiter: &Waiter) -> bool {
        let Some(queue) = self.queues.get_mut(&waiter.limits) else {
            return false;
        };
        let dequeued = queue.remove(&waiter.id).is_some();
        if queue.is_empty() {
            self.queues.remove(&waiter.limits);
        }
        dequeued
    }

    /// The oldest launch waiting here for exactly `limits`.
    fn oldest_of(&self, limits: &LimitSet) -> Option<&Arc<Waiter>> {
        self.queues
            .get(limits)?
            .first_key_value()
            .map(|(_, waiter)| waiter)
    }
}

/// Waiter ids are process-global: two waiters queued in one limit never
/// collide, whichever limits minted them. An id is minted under the lock of
/// every limit its waiter is queued in, so each queue's id order is its
/// arrival order.
static NEXT_WAITER_ID: AtomicU64 = AtomicU64::new(0);

/// One launch waiting for admission, queued in every limit it needs.
struct Waiter {
    id: u64,
    limits: LimitSet,
    /// Present exactly while the waiter is queued: taken, under every limit's
    /// lock, by the promotion that admits it or by its withdrawal.
    grant: Mutex<Option<oneshot::Sender<Admission>>>,
}

/// What a promotion found when it tried to admit a queued waiter.
enum TryAdmit {
    /// Every slot was free: taken, and the waiter was told.
    Admitted,
    /// A limit the waiter needs is full, and so it is for every waiter needing
    /// the same limits.
    Blocked,
    /// The waiter had already left the queues: admitted by another
    /// promotion, or withdrawn.
    Gone,
}

/// The limits a launch is admitted through: deduplicated by identity and kept
/// in address order, which is the lock order. Two sets are equal when they
/// hold the same limits, by identity.
#[derive(Clone, Debug, Default)]
pub struct LimitSet(Box<[Arc<LimitInner>]>);

impl PartialEq for LimitSet {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len()
            && self
                .iter()
                .zip(other.iter())
                .all(|(ours, theirs)| Arc::ptr_eq(ours, theirs))
    }
}

impl Eq for LimitSet {}

impl Hash for LimitSet {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for limit in self.iter() {
            address(limit).hash(state);
        }
    }
}

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
                queues: HashMap::new(),
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
        self.lock().queued()
    }

    /// A slot was released, or a launch arrived behind others: admit queued
    /// waiters, oldest first, while a slot is free. A waiter whose other
    /// limits are full is passed over, not waited for.
    ///
    /// Only the oldest waiter of each limit set is a candidate: when it is
    /// blocked, the rest of its set is blocked by the same full limit, whose
    /// own release promotes them. When it leaves the queue, admitted here or
    /// gone already, the next of its set takes its place.
    fn promote(&self) {
        let mut candidates: BTreeMap<u64, Arc<Waiter>> = {
            let state = self.lock();
            if state.active >= self.capacity.get() {
                return;
            }
            state
                .queues
                .values()
                .filter_map(BTreeMap::first_key_value)
                .map(|(&id, waiter)| (id, Arc::clone(waiter)))
                .collect()
        };
        while let Some((_, waiter)) = candidates.pop_first() {
            let outcome = waiter.try_admit();
            let state = self.lock();
            if state.active >= self.capacity.get() {
                return;
            }
            match outcome {
                TryAdmit::Blocked => {}
                TryAdmit::Admitted | TryAdmit::Gone => {
                    if let Some(next) = state.oldest_of(&waiter.limits) {
                        candidates.insert(next.id, Arc::clone(next));
                    }
                }
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

    /// Register a launch: take a slot in every limit if all are free and
    /// nobody is queued for them, else queue it in every limit. Synchronous,
    /// so a lone limit's counts observe the launch before its task has been
    /// polled.
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
        let free: Vec<bool> = guards
            .iter()
            .zip(self.iter())
            .map(|(state, limit)| state.active < limit.capacity.get())
            .collect();
        if free.iter().all(|&free| free) && guards.iter().all(|state| state.queues.is_empty()) {
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
            state.enqueue(&waiter);
        }
        drop(guards);
        // An arrival is an admission event, like a release: a slot free while
        // others are queued goes to the oldest one that can take it — this
        // launch only if that is this launch. Taking the slot directly would
        // let an arrival racing a release take the slot that release is about
        // to hand on. A full limit admits nobody, so it needs no walk.
        for (limit, free) in self.iter().zip(free) {
            if free {
                limit.promote();
            }
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
        #[cfg(test)]
        tests::note_examined();
        let mut guards = lock_all(&self.limits.0);
        let mut grant = self
            .grant
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if grant.is_none() {
            return TryAdmit::Gone;
        }
        let free = guards
            .iter()
            .zip(self.limits.iter())
            .all(|(state, limit)| state.active < limit.capacity.get());
        if !free {
            return TryAdmit::Blocked;
        }
        for state in &mut guards {
            let dequeued = state.dequeue(self);
            debug_assert!(
                dequeued,
                "a queued waiter is queued in every limit of its set"
            );
            state.active += 1;
        }
        let sent = grant
            .take()
            .unwrap_or_else(|| unreachable!("checked under the same lock"))
            .send(Admission {
                limits: self.limits.clone(),
            });
        drop((grant, guards));
        match sent {
            Ok(()) => TryAdmit::Admitted,
            // Dropped after the locks, since releasing it takes them.
            Err(_admission) => unreachable!(
                "a parked waiter withdraws, taking its grant, before its receiver goes"
            ),
        }
    }

    /// Leave every queue. If a promotion admitted this waiter first, its
    /// admission is in the channel unless it was received: take it and drop
    /// it, which releases the slots, so a launch that never runs holds
    /// nothing. Idempotent.
    fn withdraw(&self, granted: &mut oneshot::Receiver<Admission>) {
        // A waiter without its grant is in no queue, and needs no locks. That
        // is the common case: every admitted launch withdraws once it has run.
        let queued = self
            .grant
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some();
        if queued {
            let mut guards = lock_all(&self.limits.0);
            // Checked again under the locks: a promotion may have admitted it.
            let still_queued = self
                .grant
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
                .is_some();
            if still_queued {
                for state in &mut guards {
                    let dequeued = state.dequeue(self);
                    debug_assert!(
                        dequeued,
                        "a queued waiter is queued in every limit of its set"
                    );
                }
            }
        }
        if let Ok(admission) = granted.try_recv() {
            drop(admission);
        }
    }
}

impl AdmissionTicket {
    /// Wait for admission. `None` when any of `cancelled_by` fires first: the
    /// launch leaves every queue holding nothing, and its body never runs.
    ///
    /// Takes every token that cancels the launch, not just its own: a token
    /// linked into the launch never fires its own token, and a slot released
    /// by that same token's other launches can be granted here first.
    pub async fn acquire(self, cancelled_by: &[CancellationToken]) -> Option<Admission> {
        match self.state {
            TicketState::Admitted(admission) => Some(admission),
            TicketState::Parked(mut parked) => {
                let mut waits: Vec<_> = cancelled_by
                    .iter()
                    .map(|token| Box::pin(token.cancelled()))
                    .collect();
                let any_cancelled = std::future::poll_fn(|cx| {
                    if waits
                        .iter_mut()
                        .any(|wait| wait.as_mut().poll(cx).is_ready())
                    {
                        Poll::Ready(())
                    } else {
                        Poll::Pending
                    }
                });
                tokio::select! {
                    // Cancel-biased: a launch cancelled while parked must not
                    // run even if a slot was granted in the same instant —
                    // dropping `parked` gives such a grant back.
                    biased;
                    () = any_cancelled => None,
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

    thread_local! {
        /// Waiters examined for admission on this thread.
        static EXAMINED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    pub(super) fn note_examined() {
        EXAMINED.with(|examined| examined.set(examined.get() + 1));
    }

    fn examined() -> usize {
        EXAMINED.with(std::cell::Cell::get)
    }

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
        let mut future = pin!(ticket.acquire(std::slice::from_ref(&token)));
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
        let mut third = pin!(third.acquire(std::slice::from_ref(&token)));
        let mut fourth = pin!(fourth.acquire(std::slice::from_ref(&token)));
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
    fn an_arrival_never_takes_a_slot_a_queued_waiter_can_take() {
        let a = limit(1);
        let hold = admitted(set(&[&a]).admit());
        let token = CancellationToken::new();
        let mut older = pin!(set(&[&a]).admit().acquire(std::slice::from_ref(&token)));
        assert!(poll_once(&mut older).is_pending());

        // A release preempted between freeing its slot and promoting: the
        // state `Admission::drop` passes through between its two loops. The
        // slot is free, and `older` is still queued for it.
        std::mem::forget(hold);
        a.lock().active -= 1;

        let newer = set(&[&a]).admit();
        let Poll::Ready(Some(older_admission)) = poll_once(&mut older) else {
            panic!("the freed slot goes to the waiter queued for it, not the arrival")
        };
        let mut newer = pin!(newer.acquire(std::slice::from_ref(&token)));
        assert!(
            poll_once(&mut newer).is_pending(),
            "the arrival queues behind it"
        );
        assert_eq!((a.active_count(), a.queued_count()), (1, 1));

        drop(older_admission);
        let Poll::Ready(Some(newer_admission)) = poll_once(&mut newer) else {
            panic!("and is admitted next")
        };
        drop(newer_admission);
        assert_eq!((a.active_count(), a.queued_count()), (0, 0));
    }

    #[test]
    fn an_arrival_still_bypasses_a_waiter_blocked_on_another_limit() {
        let a = limit(1);
        let b = limit(1);
        let hold_b = admitted(set(&[&b]).admit());
        let token = CancellationToken::new();
        // Blocked on B, while A has its slot free.
        let mut both = pin!(set(&[&a, &b]).admit().acquire(std::slice::from_ref(&token)));
        assert!(poll_once(&mut both).is_pending());

        // A slot nobody queued can take is not held back for `both`.
        let only_a = admitted(set(&[&a]).admit());
        assert_eq!((a.active_count(), a.queued_count()), (1, 1));

        drop(only_a);
        drop(hold_b);
        assert!(matches!(poll_once(&mut both), Poll::Ready(Some(_))));
    }

    #[test]
    fn several_limits_are_taken_together_or_not_at_all() {
        let a = limit(1);
        let b = limit(1);
        let hold_a = admitted(set(&[&a]).admit());

        let both = set(&[&a, &b]).admit();
        let token = CancellationToken::new();
        let mut both = pin!(both.acquire(std::slice::from_ref(&token)));
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
        let mut first = pin!(ab.admit().acquire(std::slice::from_ref(&token)));
        let mut second = pin!(ba.admit().acquire(std::slice::from_ref(&token)));
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
        let mut parked = pin!(set(&[&a]).admit().acquire(std::slice::from_ref(&token)));
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
        let mut parked = pin!(set(&[&a]).admit().acquire(std::slice::from_ref(&token)));
        assert!(poll_once(&mut parked).is_pending());

        // The release admits the waiter before it observes its cancellation.
        drop(hold);
        assert_eq!(a.active_count(), 1, "granted");
        token.cancel();
        assert!(matches!(poll_once(&mut parked), Poll::Ready(None)));
        assert_eq!(a.active_count(), 0, "the granted slot was released");
    }

    #[test]
    fn a_waiter_observes_every_token_that_cancels_it() {
        let a = limit(1);
        let hold = admitted(set(&[&a]).admit());
        let own = CancellationToken::new();
        let linked = CancellationToken::new();
        let cancelled_by = [own.clone(), linked.clone()];
        let mut parked = pin!(set(&[&a]).admit().acquire(&cancelled_by));
        assert!(poll_once(&mut parked).is_pending());

        // The linked token fires, which leaves the waiter's own token alone,
        // and frees the slot through its other launch.
        linked.cancel();
        drop(hold);
        assert!(!own.is_cancelled());
        assert!(matches!(poll_once(&mut parked), Poll::Ready(None)));
        assert_eq!(
            (a.active_count(), a.queued_count()),
            (0, 0),
            "the grant was given back"
        );
    }

    #[test]
    fn overlapping_multi_limit_waiters_keep_their_own_wakeups() {
        let a = limit(1);
        let b = limit(1);
        let hold = admitted(set(&[&a, &b]).admit());
        let token = CancellationToken::new();
        let mut first = pin!(set(&[&a, &b]).admit().acquire(std::slice::from_ref(&token)));
        let mut second = pin!(set(&[&a, &b]).admit().acquire(std::slice::from_ref(&token)));
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
    fn a_free_slot_goes_to_the_oldest_waiter_whatever_limits_it_needs() {
        let a = limit(1);
        let b = limit(1);
        let hold = admitted(set(&[&a]).admit());
        let token = CancellationToken::new();
        let mut first = pin!(set(&[&a]).admit().acquire(std::slice::from_ref(&token)));
        let mut second = pin!(set(&[&a, &b]).admit().acquire(std::slice::from_ref(&token)));
        let mut third = pin!(set(&[&a]).admit().acquire(std::slice::from_ref(&token)));
        assert!(poll_once(&mut first).is_pending());
        assert!(poll_once(&mut second).is_pending());
        assert!(poll_once(&mut third).is_pending());

        drop(hold);
        let Poll::Ready(Some(first_admission)) = poll_once(&mut first) else {
            panic!("the oldest waiter is admitted")
        };
        drop(first_admission);
        let Poll::Ready(Some(second_admission)) = poll_once(&mut second) else {
            panic!("then the next oldest, though it needs another limit too")
        };
        assert!(poll_once(&mut third).is_pending());
        drop(second_admission);
        assert!(matches!(poll_once(&mut third), Poll::Ready(Some(_))));
    }

    #[test]
    fn a_blocked_limit_set_does_not_hold_back_a_younger_one() {
        let a = limit(1);
        let b = limit(1);
        let hold_a = admitted(set(&[&a]).admit());
        let hold_b = admitted(set(&[&b]).admit());
        let token = CancellationToken::new();
        let mut both = pin!(set(&[&a, &b]).admit().acquire(std::slice::from_ref(&token)));
        let mut only_a = pin!(set(&[&a]).admit().acquire(std::slice::from_ref(&token)));
        assert!(poll_once(&mut both).is_pending());
        assert!(poll_once(&mut only_a).is_pending());

        drop(hold_a);
        let Poll::Ready(Some(only_a_admission)) = poll_once(&mut only_a) else {
            panic!("A's slot goes to the younger waiter, since the older one still needs B")
        };
        assert!(poll_once(&mut both).is_pending());

        drop(hold_b);
        assert!(poll_once(&mut both).is_pending(), "A is taken now");
        drop(only_a_admission);
        assert!(matches!(poll_once(&mut both), Poll::Ready(Some(_))));
        assert_eq!((a.queued_count(), b.queued_count()), (0, 0));
    }

    #[test]
    fn admission_examines_only_the_oldest_launch_of_each_limit_set() {
        // `n` launches needing a roomy limit and a one-slot one, queued behind
        // a holder of the one-slot limit: every arrival and every release
        // finds the roomy limit free, while at most one launch can be admitted.
        let n = 200;
        let roomy = limit(n);
        let one = limit(1);
        let hold = admitted(set(&[&one]).admit());
        let before = examined();
        let tickets: Vec<_> = (0..n).map(|_| set(&[&roomy, &one]).admit()).collect();
        drop(hold);
        for ticket in tickets {
            drop(admitted(ticket));
        }
        let examined = examined() - before;
        assert!(
            examined <= 3 * n,
            "admitting {n} launches examined {examined} waiters"
        );
        assert_eq!((roomy.active_count(), one.active_count()), (0, 0));
        assert_eq!((roomy.queued_count(), one.queued_count()), (0, 0));
    }

    #[test]
    fn no_limits_admit_at_once() {
        let admission = admitted(LimitSet::new().admit());
        drop(admission);
    }
}
