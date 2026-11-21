// `baml.sync.Semaphore`: its counts, a hold released more than once, and
// a task that waits for a hold another task has.

function sync_counts() -> string {
  let gate = baml.sync.Semaphore.new(task_count(2));
  let a = gate.acquire();
  let b = gate.acquire();
  let full = `${gate.capacity()} ${gate.active_count()} ${gate.queued_count()}`;
  a.release();
  a.release();
  let one = gate.active_count();
  b.release();
  `${full} ${one} ${gate.active_count()}`
}
function sync_capacity_refused(capacity: int) -> string {
  let gate = baml.sync.Semaphore.new(capacity) catch (e) {
    baml.errors.InvalidArgument => { return e.message; },
  };
  `${gate.capacity()}`
}
function sync_no_capacity() -> string {
  `${sync_capacity_refused(task_count(0))} | ${sync_capacity_refused(0 - task_count(3))}`
}
// The second task waits until the first gives its hold up, and the third
// until the second does.
function sync_holds_in_turn() -> string {
  let gate = baml.sync.Semaphore.new(1);
  let held = gate.acquire();
  let second = spawn { let mine = gate.acquire(); let seen = gate.active_count(); let kept = task_ints(20); mine.release(); seen + kept[19] };
  let third = spawn { let mine = gate.acquire(); let seen = gate.active_count(); mine.release(); seen * 10 };
  let busy = spawn { task_churn(40) };
  let n = await busy;
  held.release();
  let a = await second;
  let b = await third;
  `${a} ${b} ${n} ${gate.active_count()} ${gate.queued_count()}`
}
function sync_with_runs_under_a_hold() -> string {
  let gate = baml.sync.Semaphore.new(1);
  let inside = gate.with(() -> int { gate.active_count() * 10 + gate.queued_count() });
  `${inside} ${gate.active_count()}`
}
