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

class SyncShared {
  count int
  most int
  order int[]
}
function sync_nap(milliseconds: int) -> void {
  baml.sys.sleep(baml.time.Duration.from_milliseconds(milliseconds));
}
// Wait until `waiting` tasks wait for a hold of `gate`, and not for ever:
// a case that goes on without them ends otherwise, and is not stuck.
function sync_until_queued(gate: baml.sync.Semaphore, waiting: int) -> void {
  for (let naps = 0; naps < 2000 && gate.queued_count() < waiting; naps += 1) { sync_nap(1); };
}
// Holds are handed on in the order they were asked for: each task here
// asks only once the one before it waits, and each gives its hold up only
// once it has said it had it.
function sync_waiters_wake_in_order() -> string {
  let gate = baml.sync.Semaphore.new(1);
  let shared = SyncShared { count: 0, most: 0, order: [] };
  let held = gate.acquire();
  let waiters: baml.future.Future<int, never>[] = [];
  for (let i = 0; i < 6; i += 1) {
    let id = task_count(i);
    waiters.push(spawn { let mine = gate.acquire(); shared.order.push(id); sync_nap(1); mine.release(); id });
    sync_until_queued(gate, i + 1);
  };
  let queued = gate.queued_count();
  held.release();
  let sum = 0;
  for (let waiter in waiters) { sum += await waiter; };
  let order = "";
  for (let id in shared.order) { order = `${order}${id}`; };
  `${queued} ${order} ${sum} ${gate.active_count()} ${gate.queued_count()}`
}
// A task that gives its hold up and then throws leaves nothing held, and
// the task that waited for it goes on.
function sync_released_then_thrown() -> string {
  let gate = baml.sync.Semaphore.new(1);
  let held = gate.acquire();
  let thrower = spawn { let mine = gate.acquire(); let seen = gate.active_count(); mine.release(); task_fail(seen + 6) };
  sync_until_queued(gate, 1);
  let next = spawn { let mine = gate.acquire(); let seen = gate.active_count(); mine.release(); seen };
  sync_until_queued(gate, 2);
  held.release();
  let thrown = await thrower catch (e) { TaskError => 0 - e.code };
  let after = await next;
  `${thrown} ${after} ${gate.active_count()} ${gate.queued_count()}`
}
// `with` gives its hold up when what it runs throws, whether the hold was
// free or waited for.
function sync_with_releases_when_it_throws() -> string {
  let free = baml.sync.Semaphore.new(1);
  let first = free.with(() -> int throws TaskError { task_fail(free.active_count() + 2) }) catch (e) { TaskError => 0 - e.code };
  let gate = baml.sync.Semaphore.new(1);
  let held = gate.acquire();
  let waited = spawn { gate.with(() -> int throws TaskError { task_fail(gate.active_count() + 4) }) };
  sync_until_queued(gate, 1);
  held.release();
  let second = await waited catch (e) { TaskError => 0 - e.code };
  `${first} ${free.active_count()} ${second} ${gate.active_count()} ${gate.queued_count()}`
}
// A task under a hold of `gate` adds to a count across a wait, which
// loses an addition when two tasks do it at once.
function sync_alone(gate: baml.sync.Semaphore, shared: SyncShared) -> int {
  let mine = gate.acquire();
  let seen = shared.count;
  sync_nap(1);
  let kept = task_ints(8);
  shared.count = seen + 1;
  shared.most = if (gate.active_count() > shared.most) { gate.active_count() } else { shared.most };
  mine.release();
  kept[7] - 6
}
// Many tasks and one hold: no two are ever under it together.
function sync_many_tasks_one_hold() -> string {
  let gate = baml.sync.Semaphore.new(1);
  let shared = SyncShared { count: 0, most: 0, order: [] };
  let tasks: baml.future.Future<int, never>[] = [];
  for (let i = 0; i < 40; i += 1) { tasks.push(spawn { sync_alone(gate, shared) }); };
  let sum = 0;
  for (let task in tasks) { sum += await task; };
  `${sum} ${shared.count} ${shared.most} ${gate.active_count()} ${gate.queued_count()}`
}
// A task under one of the holds of `gate` notes, under a hold of its own
// kind, how many tasks are under them with it.
function sync_among(gate: baml.sync.Semaphore, tally: baml.sync.Semaphore, shared: SyncShared) -> int {
  let mine = gate.acquire();
  sync_nap(2);
  let kept = task_ints(8);
  tally.with(() -> void {
    shared.count += 1;
    shared.most = if (gate.active_count() > shared.most) { gate.active_count() } else { shared.most };
  });
  mine.release();
  kept[7] - 6
}
// Many tasks and three holds: three tasks are under them together, and
// never four.
function sync_many_tasks_three_holds() -> string {
  let gate = baml.sync.Semaphore.new(3);
  let tally = baml.sync.Semaphore.new(1);
  let shared = SyncShared { count: 0, most: 0, order: [] };
  let tasks: baml.future.Future<int, never>[] = [];
  for (let i = 0; i < 60; i += 1) { tasks.push(spawn { sync_among(gate, tally, shared) }); };
  let sum = 0;
  for (let task in tasks) { sum += await task; };
  `${sum} ${shared.count} ${shared.most} ${gate.active_count()} ${gate.queued_count()} ${tally.active_count()}`
}
