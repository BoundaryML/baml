//! `baml.sync.Lock`: a lock held by one BAML task at a time, which that task
//! can take again (re-entry).
//!
//! Ownership is a task id (`BexVm::thread_id`), so re-entry follows the task
//! across helper calls and worker threads, and a spawned child, being another
//! task, waits like anyone else.
//!
//! State is one word, `LockInner::state`: the holder's task id (0 while
//! free), plus `HAS_WAITERS` while the queue may be non-empty. Taking a free
//! lock is one compare-and-swap, re-entry one load and an increment, and a
//! release with nobody waiting one compare-and-swap; only contention touches
//! the queue's mutex.
//!
//! Waiting: a contended [`LockInner::enter`] queues a [`Waiter`] and hands it
//! back, and the task parks on [`Waiter::granted`]. A release hands the lock
//! straight to the oldest waiter still waiting (no barging: a free lock never
//! has waiters, so a newcomer's compare-and-swap can only win when nobody is
//! queued). `HAS_WAITERS` is set and cleared only under the queue's mutex,
//! which is what keeps a release from missing a waiter that is just queueing.
//!
//! Release: [`LockInner::exit`] undoes one `enter`. For a task that does not
//! hold the lock it withdraws the task from the queue instead, so `with` can
//! arm its release before it waits: a task cancelled while waiting holds
//! nothing and leaves the queue, and one whose grant raced its cancellation
//! holds the lock and releases it.

use std::{
    collections::VecDeque,
    num::NonZeroU64,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering},
    },
};

use tokio::sync::Notify;

/// Set in `LockInner::state` while any task may be queued. Only ever set on
/// a held lock: a free lock has nobody waiting.
const HAS_WAITERS: u64 = 1 << 63;

/// The largest task id a lock can record; the top bit marks a lock with
/// waiters.
pub const MAX_TASK_ID: u64 = HAS_WAITERS - 1;

const WAITING: u8 = 0;
const GRANTED: u8 = 1;
const CANCELLED: u8 = 2;

/// What a `baml.sync.Lock` value holds.
pub struct LockInner {
    /// The holder's task id, or 0 while free; with [`HAS_WAITERS`] while the
    /// queue may be non-empty.
    state: AtomicU64,
    /// How many times the holder has entered without exiting. Written by the
    /// holder, or by the release that hands it the lock (before the new holder
    /// can see that it holds it).
    depth: AtomicU32,
    /// Bumped when a hold ends because its task ended, not by its releases:
    /// the guards from that hold are spent.
    epoch: AtomicU64,
    /// Tasks waiting, oldest first.
    waiters: Mutex<VecDeque<Arc<Waiter>>>,
}

/// A task waiting for a lock.
pub struct Waiter {
    task: NonZeroU64,
    state: AtomicU8,
    notify: Notify,
}

/// What a `baml.sync.Guard` holds: one acquisition of a lock.
pub struct GuardState {
    lock: Arc<LockInner>,
    task: NonZeroU64,
    epoch: u64,
    released: AtomicBool,
}

/// A guard released by a task that does not hold its lock.
#[derive(Debug)]
pub struct NotHeld;

impl std::fmt::Debug for LockInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LockInner")
            .field("holder", &self.holder())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Waiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Waiter")
            .field("task", &self.task)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for GuardState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuardState")
            .field("task", &self.task)
            .finish_non_exhaustive()
    }
}

impl LockInner {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: AtomicU64::new(0),
            depth: AtomicU32::new(0),
            epoch: AtomicU64::new(0),
            waiters: Mutex::new(VecDeque::new()),
        })
    }

    /// The holder's task id, or 0 while free.
    fn holder(&self) -> u64 {
        self.state.load(Ordering::Acquire) & !HAS_WAITERS
    }

    /// Take the lock for `task`, enter it again, or queue for it.
    ///
    /// `None` when `task` now holds it. Otherwise the [`Waiter`] to park on;
    /// the lock is `task`'s once [`Waiter::granted`] returns.
    pub fn enter(&self, task: NonZeroU64) -> Option<Arc<Waiter>> {
        let me = task.get();
        debug_assert!(me <= MAX_TASK_ID, "task id {me} collides with HAS_WAITERS");
        let state = self.state.load(Ordering::Acquire);
        if state & !HAS_WAITERS == me {
            self.depth.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        if state == 0
            && self
                .state
                .compare_exchange(0, me, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
        {
            self.depth.store(1, Ordering::Relaxed);
            return None;
        }
        self.enter_contended(task)
    }

    #[cold]
    fn enter_contended(&self, task: NonZeroU64) -> Option<Arc<Waiter>> {
        let me = task.get();
        let mut waiters = lock(&self.waiters);
        let mut state = self.state.load(Ordering::Acquire);
        loop {
            if state == 0 {
                // Released while we took the queue's mutex, with nobody queued.
                match self
                    .state
                    .compare_exchange(0, me, Ordering::Acquire, Ordering::Acquire)
                {
                    Ok(_) => {
                        self.depth.store(1, Ordering::Relaxed);
                        return None;
                    }
                    Err(now) => {
                        state = now;
                        continue;
                    }
                }
            }
            if state & HAS_WAITERS != 0 {
                break;
            }
            match self.state.compare_exchange(
                state,
                state | HAS_WAITERS,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(now) => state = now,
            }
        }
        let waiter = Arc::new(Waiter {
            task,
            state: AtomicU8::new(WAITING),
            notify: Notify::new(),
        });
        waiters.push_back(Arc::clone(&waiter));
        Some(waiter)
    }

    /// Undo one `enter` by `task`, handing the lock on when that was the
    /// outermost.
    ///
    /// For a task that does not hold the lock (its wait ended without it, by
    /// cancellation), withdraw it from the queue instead.
    pub fn exit(&self, task: NonZeroU64) {
        let me = task.get();
        // One snapshot: a waiter absent from a queue seen empty cannot be
        // granted the lock afterwards.
        let state = self.state.load(Ordering::Acquire);
        if state & !HAS_WAITERS == me {
            self.exit_held(me);
            return;
        }
        if state & HAS_WAITERS == 0 {
            return;
        }
        let mut waiters = lock(&self.waiters);
        if self.holder() == me {
            // Granted between the snapshot and the mutex: we hold it now.
            drop(waiters);
            self.exit_held(me);
            return;
        }
        self.withdraw(&mut waiters, task);
    }

    fn exit_held(&self, me: u64) {
        if self.depth.fetch_sub(1, Ordering::Relaxed) == 1 {
            self.release(me);
        }
    }

    /// Release everything `task` holds of this lock and withdraw it from the
    /// queue, for a task that ended without its releases. Guards from that
    /// hold are spent.
    pub fn release_all(&self, task: NonZeroU64) {
        let me = task.get();
        let mut waiters = lock(&self.waiters);
        self.withdraw(&mut waiters, task);
        let held = self.holder() == me;
        drop(waiters);
        if held {
            self.epoch.fetch_add(1, Ordering::Release);
            self.depth.store(0, Ordering::Relaxed);
            self.release(me);
        }
    }

    /// Take `task`'s waiters out of the queue, clearing [`HAS_WAITERS`] when
    /// that empties it. Under the queue's mutex.
    fn withdraw(&self, waiters: &mut MutexGuard<'_, VecDeque<Arc<Waiter>>>, task: NonZeroU64) {
        waiters.retain(|waiter| {
            if waiter.task == task {
                waiter.cancel();
                false
            } else {
                true
            }
        });
        if waiters.is_empty() {
            self.state.fetch_and(!HAS_WAITERS, Ordering::AcqRel);
        }
    }

    /// The holder `me` fully exited: free the lock, or hand it to the oldest
    /// task still waiting.
    fn release(&self, me: u64) {
        if self
            .state
            .compare_exchange(me, 0, Ordering::Release, Ordering::Relaxed)
            .is_ok()
        {
            return;
        }
        // HAS_WAITERS is set, and stays set until we hold the queue's mutex.
        let mut waiters = lock(&self.waiters);
        while let Some(waiter) = waiters.pop_front() {
            if waiter
                .state
                .compare_exchange(WAITING, GRANTED, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                self.depth.store(1, Ordering::Relaxed);
                let flag = if waiters.is_empty() { 0 } else { HAS_WAITERS };
                self.state
                    .store(waiter.task.get() | flag, Ordering::Release);
                waiter.notify.notify_one();
                return;
            }
        }
        self.state.store(0, Ordering::Release);
    }

    /// How many tasks are queued right now.
    pub fn waiting_count(&self) -> usize {
        lock(&self.waiters).len()
    }
}

impl Waiter {
    /// Wait until the lock is this task's. Dropping the future first marks the
    /// waiter withdrawn; the task's `exit` takes it out of the queue, or
    /// releases a grant that raced the drop.
    pub async fn granted(self: Arc<Self>) {
        struct Withdraw<'a>(&'a Waiter);
        impl Drop for Withdraw<'_> {
            fn drop(&mut self) {
                self.0.cancel();
            }
        }
        let withdraw = Withdraw(&self);
        // `notify_one` stores a permit when nobody is polling yet, so a grant
        // between the check and the await is not lost.
        while self.state.load(Ordering::Acquire) != GRANTED {
            self.notify.notified().await;
        }
        std::mem::forget(withdraw);
    }

    /// Withdraw a waiter that has not been granted the lock; no effect on one
    /// that has.
    fn cancel(&self) {
        let _ =
            self.state
                .compare_exchange(WAITING, CANCELLED, Ordering::AcqRel, Ordering::Acquire);
    }
}

impl GuardState {
    /// A guard for the hold `task` has on `lock` right now.
    pub fn new(lock: Arc<LockInner>, task: NonZeroU64) -> Self {
        debug_assert_eq!(lock.holder(), task.get(), "a guard is taken by the holder");
        let epoch = lock.epoch.load(Ordering::Acquire);
        Self {
            lock,
            task,
            epoch,
            released: AtomicBool::new(false),
        }
    }

    pub fn lock(&self) -> &Arc<LockInner> {
        &self.lock
    }

    /// Claim this guard's release for `task`: `Ok(true)` when the caller must
    /// now exit the lock once, `Ok(false)` when the guard is spent (released
    /// already, or its task ended and released everything), and [`NotHeld`]
    /// when another task still holds the lock through it.
    pub fn claim(&self, task: NonZeroU64) -> Result<bool, NotHeld> {
        if self.released.load(Ordering::Acquire)
            || self.lock.epoch.load(Ordering::Acquire) != self.epoch
        {
            return Ok(false);
        }
        if task != self.task {
            return Err(NotHeld);
        }
        Ok(!self.released.swap(true, Ordering::AcqRel))
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Nothing panics while the queue is locked; recover rather than cascade.
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl crate::BexRustData for LockInner {
    fn measure(&self, meter: &mut crate::Meter) {
        meter.bytes(size_of::<Self>());
    }
}

impl crate::BexRustData for Waiter {
    fn measure(&self, meter: &mut crate::Meter) {
        meter.bytes(size_of::<Self>());
    }
}

impl crate::BexRustData for GuardState {
    fn measure(&self, meter: &mut crate::Meter) {
        meter.bytes(size_of::<Self>());
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn task(n: u64) -> NonZeroU64 {
        NonZeroU64::new(n).unwrap()
    }

    #[test]
    fn free_reentry_and_release() {
        let lock = LockInner::new();
        assert!(lock.enter(task(7)).is_none());
        assert!(lock.enter(task(7)).is_none());
        lock.exit(task(7));
        assert_eq!(lock.holder(), 7, "the outer entry still holds it");
        lock.exit(task(7));
        assert_eq!(lock.state.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn hands_off_oldest_first_and_cancelled_waiters_leave_the_queue() {
        let lock = LockInner::new();
        assert!(lock.enter(task(1)).is_none());
        let second = lock.enter(task(2)).unwrap();
        let third = lock.enter(task(3)).unwrap();
        let fourth = lock.enter(task(4)).unwrap();
        assert_eq!(lock.waiting_count(), 3);

        // Task 2 is cancelled while waiting; its release takes it out of the
        // queue at once, not when the holder lets go.
        drop(Arc::clone(&second).granted());
        lock.exit(task(2));
        assert_eq!(lock.waiting_count(), 2);

        lock.exit(task(1));
        third.granted().await;
        assert_eq!(lock.holder(), 3);
        assert_eq!(fourth.state.load(Ordering::Acquire), WAITING);
        lock.exit(task(3));
        fourth.granted().await;
        lock.exit(task(4));
        assert_eq!(lock.state.load(Ordering::Acquire), 0, "free, no waiters");
    }

    #[test]
    fn a_grant_that_raced_cancellation_is_released_by_exit() {
        let lock = LockInner::new();
        assert!(lock.enter(task(1)).is_none());
        let waiter = lock.enter(task(2)).unwrap();
        lock.exit(task(1));
        assert_eq!(lock.holder(), 2, "granted before the waiter saw it");
        drop(waiter.granted()); // cancelled: never observed the grant
        lock.exit(task(2));
        assert_eq!(lock.state.load(Ordering::Acquire), 0);
    }

    #[test]
    fn the_last_withdrawal_clears_has_waiters() {
        let lock = LockInner::new();
        assert!(lock.enter(task(1)).is_none());
        let _waiter = lock.enter(task(2)).unwrap();
        lock.exit(task(2));
        assert_eq!(
            lock.state.load(Ordering::Acquire),
            1,
            "held by 1, nobody queued"
        );
        lock.exit(task(1));
        assert_eq!(lock.state.load(Ordering::Acquire), 0);
    }

    #[test]
    fn release_all_hands_on_and_spends_guards() {
        let lock = LockInner::new();
        assert!(lock.enter(task(1)).is_none());
        assert!(lock.enter(task(1)).is_none());
        let guard = GuardState::new(Arc::clone(&lock), task(1));
        let next = lock.enter(task(2)).unwrap();
        lock.release_all(task(1));
        assert_eq!(next.state.load(Ordering::Acquire), GRANTED);
        assert_eq!(lock.holder(), 2);
        // Task 1 ended: its guard is spent, even when called from task 2.
        assert!(matches!(guard.claim(task(2)), Ok(false)));
        lock.exit(task(2));
    }

    #[test]
    fn guards_release_once_and_only_on_their_task() {
        let lock = LockInner::new();
        assert!(lock.enter(task(1)).is_none());
        let guard = GuardState::new(Arc::clone(&lock), task(1));
        assert!(
            guard.claim(task(2)).is_err(),
            "another task, while task 1 holds it"
        );
        assert!(matches!(guard.claim(task(1)), Ok(true)));
        assert!(matches!(guard.claim(task(1)), Ok(false)), "run once");
    }

    /// Many tasks on many worker threads contending for one lock, each doing a
    /// read-then-write the lock must keep whole, with some entering twice.
    /// A lost wakeup hangs (caught by the timeout); a broken exclusion loses an
    /// update.
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn stress_no_lost_wakeups_or_updates() {
        const TASKS: u64 = 64;
        const ROUNDS: u64 = 500;
        let lock = LockInner::new();
        let counter = Arc::new(AtomicU64::new(0));
        let mut handles = Vec::new();
        for id in 1..=TASKS {
            let lock = Arc::clone(&lock);
            let counter = Arc::clone(&counter);
            handles.push(tokio::spawn(async move {
                for round in 0..ROUNDS {
                    if let Some(waiter) = lock.enter(task(id)) {
                        waiter.granted().await;
                    }
                    let reentrant = round % 3 == 0;
                    if reentrant {
                        assert!(lock.enter(task(id)).is_none());
                    }
                    let read = counter.load(Ordering::Relaxed);
                    tokio::task::yield_now().await;
                    counter.store(read + 1, Ordering::Relaxed);
                    if reentrant {
                        lock.exit(task(id));
                    }
                    lock.exit(task(id));
                }
            }));
        }
        let all = async {
            for handle in handles {
                handle.await.unwrap();
            }
        };
        tokio::time::timeout(Duration::from_secs(60), all)
            .await
            .expect("a waiter was never woken");
        assert_eq!(counter.load(Ordering::Relaxed), TASKS * ROUNDS);
        assert_eq!(lock.state.load(Ordering::Acquire), 0);
    }

    /// The same contention with waits abandoned at random: every abandoned
    /// wait exits, and the lock still ends free with nobody queued.
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn stress_with_cancelled_waits() {
        const TASKS: u64 = 32;
        const ROUNDS: u64 = 300;
        let lock = LockInner::new();
        let mut handles = Vec::new();
        for id in 1..=TASKS {
            let lock = Arc::clone(&lock);
            handles.push(tokio::spawn(async move {
                for round in 0..ROUNDS {
                    if let Some(waiter) = lock.enter(task(id)) {
                        if (id + round) % 4 == 0 {
                            // Give up after a moment; the exit cleans up either way.
                            let _ =
                                tokio::time::timeout(Duration::from_micros(50), waiter.granted())
                                    .await;
                            lock.exit(task(id));
                            continue;
                        }
                        waiter.granted().await;
                    }
                    tokio::task::yield_now().await;
                    lock.exit(task(id));
                }
            }));
        }
        let all = async {
            for handle in handles {
                handle.await.unwrap();
            }
        };
        tokio::time::timeout(Duration::from_secs(60), all)
            .await
            .expect("a waiter was never woken");
        assert_eq!(lock.state.load(Ordering::Acquire), 0);
        assert_eq!(lock.waiting_count(), 0);
    }
}
