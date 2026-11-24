//! `baml.sync.Semaphore`: a count of holds and a queue of those waiting
//! for one, as Rust state the `Semaphore` owns. A hold that has to wait
//! waits as I/O does: its task is parked until whoever releases a hold
//! hands it on, first come first served. Only a task can release one, so
//! the wait says so: see `Scope::wait_for_a_task`.
//!
//! The state is behind locks of its own and no part of it is in the heap,
//! so it is as it stands for several workers. `baml.sync.atomic` is not
//! here: its key is an object, which the collector moves, so what holds a
//! lock on one needs a place to be first.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use super::{
    NativeBamlSync, NativeBamlSyncPermit, NativeBamlSyncSemaphore, Natives, Obj, ObjRef, Opaque,
    OpaqueRef, Root, Scope, Thrown,
};
use crate::layout::{NAMED_SYNC_PERMIT, NAMED_SYNC_SEMAPHORE};

/// What a `Semaphore` owns.
struct Limit {
    capacity: usize,
    counts: Mutex<Counts>,
}

struct Counts {
    /// How many holds are admitted and not released.
    active: usize,
    /// The holds that wait, the longest wait first.
    queued: VecDeque<Arc<Hold>>,
}

/// Where a hold is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Queued,
    Admitted,
    Released,
}

/// One hold of a semaphore: what `_admit` returns, and what a `Permit`
/// owns.
struct Hold {
    limit: Arc<Limit>,
    stage: Mutex<Stage>,
    /// Told when the hold is admitted from the queue.
    admitted: tokio::sync::Notify,
}

fn locked<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Hold {
    /// Give the hold up, wherever it is: once, however often it is asked.
    /// A hold that was admitted makes room for the one that waited
    /// longest.
    fn release(self: &Arc<Self>) {
        let mut counts = locked(&self.limit.counts);
        let was = std::mem::replace(&mut *locked(&self.stage), Stage::Released);
        match was {
            Stage::Released => {}
            // What waits for this hold has nothing to wait for any more.
            Stage::Queued => {
                counts.queued.retain(|queued| !Arc::ptr_eq(queued, self));
                self.admitted.notify_one();
            }
            Stage::Admitted => match counts.queued.pop_front() {
                // The count is the next hold's now.
                Some(next) => {
                    *locked(&next.stage) = Stage::Admitted;
                    next.admitted.notify_one();
                }
                None => counts.active -= 1,
            },
        }
    }
}

/// The count `count` as an `int`.
fn int(count: usize) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}

/// What the one field of `object`, a `Semaphore` or a `Permit`, owns.
fn handle<T: Send + Sync + 'static>(object: ObjRef<'_>) -> &T {
    // SAFETY: both classes have their handle in their one field.
    unsafe { object.field::<Opaque>(0) }.owned::<T>()
}

impl NativeBamlSyncSemaphore for Natives {
    fn new<'s>(scope: &mut Scope<'s>, capacity: i64) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let Some(size) = usize::try_from(capacity).ok().filter(|size| *size != 0) else {
            return Err(scope.invalid_argument(&format!(
                "baml.sync.Semaphore.new: capacity must be positive, got {capacity}"
            )));
        };
        let limit = scope.alloc_owned(Arc::new(Limit {
            capacity: size,
            counts: Mutex::new(Counts {
                active: 0,
                queued: VecDeque::new(),
            }),
        }));
        let mut built = scope.build(scope.named_class(NAMED_SYNC_SEMAPHORE));
        built.set(scope, 0, limit);
        Ok(built.finish())
    }

    fn capacity(semaphore: ObjRef<'_>) -> i64 {
        int(handle::<Limit>(semaphore).capacity)
    }

    fn active_count(semaphore: ObjRef<'_>) -> i64 {
        int(locked(&handle::<Limit>(semaphore).counts).active)
    }

    fn queued_count(semaphore: ObjRef<'_>) -> i64 {
        int(locked(&handle::<Limit>(semaphore).counts).queued.len())
    }

    fn _admit<'s>(scope: &mut Scope<'s>, semaphore: Root<'s, Obj>) -> Root<'s, Opaque> {
        // SAFETY: a `Semaphore` has its handle in its one field.
        let limit = unsafe { semaphore.get(scope).field::<Opaque>(0) }.share::<Limit>();
        let hold = {
            let mut counts = locked(&limit.counts);
            // Nothing passes a hold that waits.
            let admitted = counts.active < limit.capacity && counts.queued.is_empty();
            let hold = Arc::new(Hold {
                limit: Arc::clone(&limit),
                stage: Mutex::new(if admitted {
                    Stage::Admitted
                } else {
                    Stage::Queued
                }),
                admitted: tokio::sync::Notify::new(),
            });
            if admitted {
                counts.active += 1;
            } else {
                counts.queued.push_back(Arc::clone(&hold));
            }
            hold
        };
        scope.alloc_owned(hold)
    }
}

impl NativeBamlSyncPermit for Natives {
    fn release(permit: ObjRef<'_>) {
        // SAFETY: a `Permit` has its hold in its one field.
        unsafe { permit.field::<Opaque>(0) }
            .share::<Hold>()
            .release();
    }
}

impl NativeBamlSync for Natives {
    fn _admitted(held: OpaqueRef<'_>) -> bool {
        *locked(&held.owned::<Hold>().stage) == Stage::Admitted
    }

    async fn _wait_permit<'s>(
        scope: &mut Scope<'s>,
        held: Root<'s, Opaque>,
    ) -> Result<(), Thrown<'s>> {
        let hold = held.get(scope).share::<Hold>();
        let waiting = Arc::clone(&hold);
        scope
            .wait_for_a_task(
                async move {
                    // A hold released while it waits has nothing to wait
                    // for.
                    while *locked(&hold.stage) == Stage::Queued {
                        hold.admitted.notified().await;
                    }
                },
                move || *locked(&waiting.stage) == Stage::Queued,
            )
            .await;
        Ok(())
    }

    fn _permit<'s>(scope: &mut Scope<'s>, held: Root<'s, Opaque>) -> Root<'s, Obj> {
        let hold = held.get(scope).share::<Hold>();
        let hold = scope.alloc_owned(hold);
        let mut built = scope.build(scope.named_class(NAMED_SYNC_PERMIT));
        built.set(scope, 0, hold);
        built.finish()
    }

    fn _release_permit(held: OpaqueRef<'_>) {
        held.share::<Hold>().release();
    }
}
