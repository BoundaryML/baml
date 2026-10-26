//! `baml.sync.atomic`: per-object, task-reentrant locks, taken one at a time
//! or several together.
//!
//! A lock is identified by the address of the object it protects. The table
//! holds an entry for a key only while some task holds it or waits for it, so
//! an object nobody has locked costs nothing, and an uncontended acquisition
//! allocates nothing.
//!
//! Ownership is a task id (`BexVm::thread_id`), so re-entry follows the BAML
//! task across helper calls and worker threads, and a spawned child, being a
//! different task, waits like any other.
//!
//! Several keys: [`MonitorTable::enter_all`] re-enters the keys its task
//! already holds and takes the rest together. While it waits it holds none of
//! the rest, so blocks naming the same keys in different orders take turns
//! instead of deadlocking. The keys it already held stay held while it waits:
//! an outer block cannot let go of them.
//!
//! Waiting: a contended entry queues one [`Waiter`] on every key it still
//! needs and hands it back; the task then parks on [`Waiter::granted`].
//! Fairness follows `limit.rs`: whenever a key may have become takeable (a
//! release, or an arrival that finds others already queued, which joins the
//! back of the queue rather than taking the key) it goes to the oldest waiter
//! on it that can take every key it needs, switching it from `WAITING` to
//! `GRANTED` under the shard locks. A waiter whose other keys are held is
//! passed over, not waited for, so a free key can stay queued for while every
//! waiter on it is blocked elsewhere.
//!
//! A waiter that is cancelled switches itself to `CANCELLED`; its task's exit,
//! which always follows, takes it out of every queue, and a hand-off skips it
//! if it gets there first.
//!
//! Release: [`MonitorTable::exit`] undoes one acquisition, and is a no-op on a
//! key its task does not own. That lets `atomic` arm its release before it
//! waits: a task cancelled while waiting owns nothing it was waiting for and
//! releases nothing of it, and one whose grant raced its cancellation owns the
//! keys and releases them.
//!
//! Locking: one key takes one shard mutex. Anything that can touch keys in
//! several shards (entering or leaving several keys, or handing a key to a
//! waiter that needs others) locks every shard, in order, so only contended
//! use of several keys pays for it.
//!
//! The garbage collector moves objects, so the keys, and the keys each waiter
//! still needs, are forwarded on every collection
//! ([`MonitorTable::visit_keys`]). The table holds them weakly: a held key's
//! object is on its holder's stack, and a waited-for one's on its waiter's.

use std::{
    collections::{HashMap, HashSet, VecDeque, hash_map::Entry},
    hash::{BuildHasher, BuildHasherDefault},
    num::NonZeroU64,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU8, Ordering},
    },
};

use rustc_hash::FxHasher;
use tokio::sync::Notify;

use crate::{HeapPtr, RootVisitor};

type FxHashMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;
type Keys = FxHashMap<HeapPtr, Monitor>;

/// Shards spread unrelated locks across mutexes so tasks on different worker
/// threads locking different objects rarely meet.
const SHARDS: usize = 16;

const WAITING: u8 = 0;
const GRANTED: u8 = 1;
const CANCELLED: u8 = 2;

/// Every lock currently held or waited for, by the address of the object it
/// protects.
pub struct MonitorTable {
    shards: [Shard; SHARDS],
}

/// One shard, on its own cache line so neighbouring shards don't share one.
#[repr(align(128))]
struct Shard(Mutex<Keys>);

/// One lock that is held or waited for.
struct Monitor {
    /// The holder's task id. `None` while the key is free but still queued
    /// for, by waiters that are all blocked on other keys.
    owner: Option<NonZeroU64>,
    /// How many times the holder has entered without exiting.
    depth: u32,
    /// Tasks waiting for it, oldest first. A cancelled waiter leaves when its
    /// task's exit runs. Empty (and unallocated) while uncontended.
    waiters: VecDeque<Arc<Waiter>>,
}

/// A task waiting for one or more locks, to be taken together.
pub struct Waiter {
    task: NonZeroU64,
    state: AtomicU8,
    notify: Notify,
    /// The keys it needs: queued on each. Read and forwarded only under the
    /// shard locks of all of them.
    keys: Mutex<Vec<HeapPtr>>,
}

impl std::fmt::Debug for MonitorTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MonitorTable").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Waiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Waiter")
            .field("task", &self.task)
            .finish_non_exhaustive()
    }
}

impl Default for MonitorTable {
    fn default() -> Self {
        Self::new()
    }
}

impl MonitorTable {
    pub fn new() -> Self {
        Self {
            shards: std::array::from_fn(|_| Shard(Mutex::new(FxHashMap::default()))),
        }
    }

    fn one(&self, key: HeapPtr) -> OneShard<'_> {
        let index = shard_index(key);
        OneShard {
            index,
            keys: lock(&self.shards[index].0),
        }
    }

    fn all(&self) -> AllShards<'_> {
        AllShards(self.shards.iter().map(|shard| lock(&shard.0)).collect())
    }

    /// Acquire `key`'s lock for `task`, or queue for it.
    ///
    /// `None` when `task` now holds it: it was free, or `task` already held it
    /// and has entered once more. Otherwise the [`Waiter`] to park on; the
    /// lock is `task`'s once [`Waiter::granted`] returns.
    pub fn enter(&self, key: HeapPtr, task: NonZeroU64) -> Option<Arc<Waiter>> {
        let mut shard = self.one(key);
        let waiter = match shard.keys.entry(key) {
            Entry::Vacant(slot) => {
                slot.insert(Monitor::held(task));
                return None;
            }
            Entry::Occupied(mut slot) => {
                let monitor = slot.get_mut();
                if monitor.owner == Some(task) {
                    monitor.depth += 1;
                    return None;
                }
                let waiter = Waiter::new(task, vec![key]);
                monitor.waiters.push_back(Arc::clone(&waiter));
                if monitor.owner.is_some() {
                    return Some(waiter);
                }
                waiter
            }
        };
        // Free, but queued for: it goes to the oldest waiter that can take
        // everything it needs, which may be this one.
        if !promote(&mut shard, key) {
            drop(shard);
            promote(&mut self.all(), key);
        }
        waiter.pending()
    }

    /// Acquire every lock in `keys` for `task` together, or queue for them.
    /// `keys` holds no duplicates.
    ///
    /// Keys `task` already holds are entered once more at once, and stay held
    /// while it waits for the rest. `None` when `task` now holds them all;
    /// otherwise the [`Waiter`] to park on, as for [`Self::enter`].
    pub fn enter_all(&self, keys: &[HeapPtr], task: NonZeroU64) -> Option<Arc<Waiter>> {
        let mut all = self.all();
        let mut needed = Vec::new();
        for &key in keys {
            match all.keys(key).get_mut(&key) {
                Some(monitor) if monitor.owner == Some(task) => monitor.depth += 1,
                _ => needed.push(key),
            }
        }
        if needed.iter().all(|key| !all.keys(*key).contains_key(key)) {
            // Nobody holds or waits for any of them.
            for &key in &needed {
                all.keys(key).insert(key, Monitor::held(task));
            }
            return None;
        }
        let waiter = Waiter::new(task, needed.clone());
        for &key in &needed {
            all.keys(key)
                .entry(key)
                .or_insert_with(Monitor::free)
                .waiters
                .push_back(Arc::clone(&waiter));
        }
        for &key in &needed {
            promote(&mut all, key);
        }
        waiter.pending()
    }

    /// Undo one acquisition of `key`'s lock by `task`, handing the lock on
    /// when that was the outermost.
    ///
    /// A no-op on the lock for a task that does not own it: one whose wait
    /// ended in cancellation. If that task is somehow still queued (it never
    /// reached its wait), it is withdrawn so the lock is never handed to it.
    pub fn exit(&self, key: HeapPtr, task: NonZeroU64) {
        let mut shard = self.one(key);
        if release(&mut shard.keys, key, task) && !promote(&mut shard, key) {
            drop(shard);
            promote(&mut self.all(), key);
        }
    }

    /// Undo one [`Self::enter_all`] of `keys` by `task`: [`Self::exit`] for
    /// each, handing on together.
    pub fn exit_all(&self, keys: &[HeapPtr], task: NonZeroU64) {
        let mut all = self.all();
        let freed: Vec<_> = keys
            .iter()
            .copied()
            .filter(|&key| release(all.keys(key), key, task))
            .collect();
        for key in freed {
            promote(&mut all, key);
        }
    }

    /// Release everything `task` still holds and withdraw it from every
    /// queue, for a task that ended without running its releases.
    pub fn release_all(&self, task: NonZeroU64) {
        let mut all = self.all();
        let mut freed = Vec::new();
        for shard in &mut all.0 {
            shard.retain(|&key, monitor| {
                withdraw(monitor, task);
                if monitor.owner == Some(task) {
                    monitor.owner = None;
                    monitor.depth = 0;
                    freed.push(key);
                }
                monitor.owner.is_some() || !monitor.waiters.is_empty()
            });
        }
        for key in freed {
            promote(&mut all, key);
        }
    }

    /// Hand every key, and every key a waiter still needs, to `visitor` as a
    /// weak reference: a moved key is repointed, and a dead one's entry is
    /// dropped.
    pub fn visit_keys(&self, visitor: &mut dyn RootVisitor) {
        let mut all = self.all();
        // A waiter is queued on each key it needs: forward its list once.
        let mut seen = HashSet::new();
        for shard in &all.0 {
            for waiter in shard.values().flat_map(|monitor| &monitor.waiters) {
                if seen.insert(Arc::as_ptr(waiter)) {
                    for key in lock(&waiter.keys).iter_mut() {
                        let live = visitor.visit_weak(key);
                        debug_assert!(live, "a waited-for key outlived by its waiter");
                    }
                }
            }
        }
        // Take every entry out before reinserting any, so a new address can
        // never land on an entry not yet looked at. Entries exist only while
        // a key is held or waited for, so there are few.
        let mut live = Vec::new();
        for shard in &mut all.0 {
            live.extend(shard.drain().filter_map(|(mut key, monitor)| {
                visitor.visit_weak(&mut key).then_some((key, monitor))
            }));
        }
        for (key, monitor) in live {
            let previous = all.keys(key).insert(key, monitor);
            debug_assert!(previous.is_none(), "two locks forwarded to one object");
        }
    }

    /// How many tasks wait for `key`'s lock right now.
    pub fn waiting_count(&self, key: HeapPtr) -> usize {
        self.one(key)
            .keys
            .get(&key)
            .map_or(0, |monitor| monitor.waiters.len())
    }

    /// How many locks are held right now.
    pub fn held_count(&self) -> usize {
        self.shards
            .iter()
            .map(|shard| {
                lock(&shard.0)
                    .values()
                    .filter(|monitor| monitor.owner.is_some())
                    .count()
            })
            .sum()
    }
}

impl Monitor {
    fn held(task: NonZeroU64) -> Self {
        Self {
            owner: Some(task),
            depth: 1,
            waiters: VecDeque::new(),
        }
    }

    fn free() -> Self {
        Self {
            owner: None,
            depth: 0,
            waiters: VecDeque::new(),
        }
    }
}

/// The shards a lock-table operation has locked.
trait Locked {
    /// The map `key` lives in, or `None` when its shard is not locked.
    fn get(&mut self, key: HeapPtr) -> Option<&mut Keys>;
}

/// One shard, for an operation on one key.
struct OneShard<'a> {
    index: usize,
    keys: MutexGuard<'a, Keys>,
}

impl Locked for OneShard<'_> {
    fn get(&mut self, key: HeapPtr) -> Option<&mut Keys> {
        (shard_index(key) == self.index).then_some(&mut *self.keys)
    }
}

/// Every shard, locked in order.
struct AllShards<'a>(Vec<MutexGuard<'a, Keys>>);

impl AllShards<'_> {
    fn keys(&mut self, key: HeapPtr) -> &mut Keys {
        &mut self.0[shard_index(key)]
    }
}

impl Locked for AllShards<'_> {
    fn get(&mut self, key: HeapPtr) -> Option<&mut Keys> {
        Some(self.keys(key))
    }
}

fn shard_index(key: HeapPtr) -> usize {
    let hash = BuildHasherDefault::<FxHasher>::default().hash_one(key);
    // The high bits: FxHash mixes upward, and heap addresses share their low
    // (alignment) bits.
    #[allow(clippy::cast_possible_truncation, reason = "reduced modulo SHARDS")]
    let index = (hash >> 32) as usize % SHARDS;
    index
}

/// Undo one acquisition of `key` by `task`. `true` when that freed it, and it
/// must be promoted.
fn release(keys: &mut Keys, key: HeapPtr, task: NonZeroU64) -> bool {
    let Some(monitor) = keys.get_mut(&key) else {
        return false;
    };
    if monitor.owner != Some(task) {
        withdraw(monitor, task);
        if monitor.owner.is_none() && monitor.waiters.is_empty() {
            keys.remove(&key);
        }
        return false;
    }
    monitor.depth -= 1;
    if monitor.depth > 0 {
        return false;
    }
    monitor.owner = None;
    true
}

/// Take `task`'s waiters out of the queue, so a holder that keeps the lock
/// a long time doesn't accumulate cancelled waits.
fn withdraw(monitor: &mut Monitor, task: NonZeroU64) {
    monitor.waiters.retain(|waiter| {
        if waiter.task == task {
            waiter.cancel();
            false
        } else {
            true
        }
    });
}

/// Give `key`, if it is free, to the oldest waiter on it that can take every
/// key it needs, and drop its entry if nobody is left waiting. `false` when a
/// waiter needs keys in shards `locked` does not hold: the caller retries with
/// every shard locked.
fn promote(locked: &mut impl Locked, key: HeapPtr) -> bool {
    let mut next = 0;
    loop {
        let keys = locked.get(key).expect("the promoted key's shard is locked");
        let Some(monitor) = keys.get_mut(&key) else {
            return true;
        };
        if monitor.owner.is_some() {
            return true;
        }
        let Some(waiter) = monitor.waiters.get(next).map(Arc::clone) else {
            if monitor.waiters.is_empty() {
                keys.remove(&key);
            }
            return true;
        };
        if waiter.state.load(Ordering::Acquire) == CANCELLED {
            monitor.waiters.remove(next);
            continue;
        }
        let needs = lock(&waiter.keys);
        let mut takeable = true;
        for &need in needs.iter() {
            let Some(keys) = locked.get(need) else {
                return false;
            };
            takeable &= keys
                .get(&need)
                .is_some_and(|monitor| monitor.owner.is_none());
        }
        if !takeable {
            next += 1;
            continue;
        }
        let granted = waiter
            .state
            .compare_exchange(WAITING, GRANTED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok();
        for &need in needs.iter() {
            let keys = locked.get(need).expect("checked above");
            let monitor = keys.get_mut(&need).expect("checked above");
            monitor
                .waiters
                .retain(|queued| !Arc::ptr_eq(queued, &waiter));
            if granted {
                monitor.owner = Some(waiter.task);
                monitor.depth = 1;
            }
        }
        if granted {
            waiter.notify.notify_one();
            return true;
        }
        // Cancelled since the check: it has left every queue; look again.
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Nothing panics while a shard is locked; recover rather than cascade.
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Waiter {
    fn new(task: NonZeroU64, keys: Vec<HeapPtr>) -> Arc<Self> {
        Arc::new(Self {
            task,
            state: AtomicU8::new(WAITING),
            notify: Notify::new(),
            keys: Mutex::new(keys),
        })
    }

    /// `None` when the keys were granted while it was queued; otherwise this
    /// waiter, to park on.
    fn pending(self: Arc<Self>) -> Option<Arc<Self>> {
        (self.state.load(Ordering::Acquire) != GRANTED).then_some(self)
    }

    /// Wait until the locks are this task's. Dropping the future first
    /// withdraws the task, unless the locks were granted in the meantime, in
    /// which case the task owns them and its release must give them back.
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

    /// Withdraw a waiter that has not been granted its locks; no effect on
    /// one that has.
    fn cancel(&self) {
        let _ =
            self.state
                .compare_exchange(WAITING, CANCELLED, Ordering::AcqRel, Ordering::Acquire);
    }
}

impl crate::BexRustData for Waiter {
    fn measure(&self, meter: &mut crate::Meter) {
        meter.bytes(size_of::<Self>());
    }
}
