// Tasks: `spawn` and `await`. A spawned task may start before the task that
// spawned it goes on, or after, so no case here depends on which.

class TaskError { code: int }

function task_count(n: int) -> int {
  let x = 0;
  for (let i = 0; i < n; i += 1) { x += 1; };
  x
}

// A task of its own for each call, capturing nothing that changes later.
function task_square(i: int) -> baml.future.Future<int, never> {
  spawn { task_count(i) * i }
}

function task_fail(code: int) -> int throws TaskError {
  if code > 0 { throw TaskError { code: code } }
  0
}

function task_value() -> int {
  let f = spawn { task_count(41) + 1 };
  await f
}

function task_string() -> string {
  let f = spawn { rep("ab", 3) };
  let g = spawn { rep("c", 20) };
  (await g) + (await f)
}

function task_float() -> float {
  let f = spawn { halves(5) };
  await f
}

function task_bool() -> bool {
  let f = spawn { task_count(2) == 2 };
  await f
}

function task_in_order() -> int {
  let futures = [task_square(1)];
  for (let i = 2; i <= 50; i += 1) { futures.push(task_square(i)); };
  let sum = 0;
  for (let i = 0; i < futures.length(); i += 1) {
    sum = (sum * 3 + (await futures[i])) % 1000003;
  };
  sum
}

function task_in_reverse() -> int {
  let futures = [task_square(1)];
  for (let i = 2; i <= 50; i += 1) { futures.push(task_square(i)); };
  let sum = 0;
  let i = futures.length() - 1;
  while i >= 0 {
    sum = (sum * 3 + (await futures[i])) % 1000003;
    i -= 1;
  };
  sum
}

function task_tree(depth: int) -> int {
  if (depth == 0) { return 1; }
  let left = spawn { task_tree(depth - 1) };
  let right = spawn { task_tree(depth - 1) };
  (await left) + (await right) + 1
}

function task_tree_of_tasks() -> int {
  task_tree(9)
}

// A task awaits the future of another, which it was handed.
function task_sibling() -> int {
  let first = spawn { task_count(20) };
  let second = spawn { (await first) + 1 };
  (await second) * 100 + (await first)
}

function task_finished_twice() -> int {
  let f = spawn { task_count(5) };
  let a = await f;
  let b = await f;
  a * 10 + b
}

function task_throws() -> int {
  let f = spawn { task_fail(task_count(7)) };
  (await f) catch (e) {
    TaskError => e.code,
  }
}

function task_await_in(f: baml.future.Future<int, TaskError>) -> int throws TaskError {
  (await f) + 1
}

// The error arrives through a frame that does not catch it, and again at
// every later `await`.
function task_throws_through() -> int {
  let f = spawn { task_fail(task_count(3)) };
  let first = task_await_in(f) catch (e) {
    TaskError => e.code * 2,
  };
  let second = (await f) catch (e) {
    TaskError => e.code * 5,
  };
  first * 100 + second
}

function task_does_not_throw() -> int {
  let f = spawn { task_fail(task_count(0)) };
  (await f) catch (e) {
    TaskError => -1,
  }
}

// A panic in a task is thrown by the `await` of its future.
function task_panics() -> int {
  let f = spawn { 10 / task_count(0) };
  await f
}

// The task changes a variable its spawner reads only once it has ended.
function task_mutates() -> int {
  let x = 1;
  let f = spawn { x = x + task_count(4); 0 };
  let r = await f;
  x * 10 + r
}

// What the spawner holds stays good while its tasks allocate.
function task_keeps_references() -> string {
  let before = rep("x", 5);
  let items = [rep("a", 3), rep("b", 2)];
  let f = spawn {
    let t = "";
    for (let i = 0; i < 40; i += 1) { t = rep("z", i % 7); };
    items.push(rep("q", 2));
    t
  };
  let g = spawn { (await f) + rep("y", 2) };
  let made = await g;
  `${before}${items[0]}${items[1]}${items[2]}${made}${await f}`
}

function task_dive(depth: int) -> int {
  if depth == 0 { return 0; }
  task_dive(depth - 1) + 1
}

function task_recursion() -> int {
  let f = spawn { task_dive(task_count(300)) };
  await f
}

// A task's stack does not grow yet.
// native throws: baml.panics.StackOverflow {message: "stack overflow"}
function task_recursion_too_deep() -> int {
  let f = spawn { task_dive(task_count(20000)) };
  await f
}

// The future is never named: it lives in a temporary.
function task_awaited_at_once() -> int {
  let s = 0;
  for (let i = 0; i < 100; i += 1) {
    s += await spawn { task_count(i) };
  };
  s
}

// Sleeping tasks wake in the order of their timers, whatever order they
// were spawned in.
function task_nap(order: int[], id: int, milliseconds: int) -> int {
  baml.sys.sleep(baml.time.Duration.from_milliseconds(milliseconds));
  order.push(id);
  id
}
function task_sleep_order() -> string {
  let order: int[] = [];
  let naps = [90, 30, 60, 150, 120, 10];
  let futures: baml.future.Future<int, never>[] = [];
  for (let i = 0; i < naps.length(); i += 1) {
    let id = task_count(i);
    let nap = naps[i];
    futures.push(spawn { task_nap(order, id, nap) });
  }
  let sum = 0;
  for (let f in futures) { sum += await f; }
  let out = `${sum}:`;
  for (let id in order) { out = out + `${id}`; }
  out
}
// A sleep for no time, or for less, returns.
function task_sleep_no_wait() -> string {
  let order: int[] = [];
  let sum = task_nap(order, 1, task_count(0)) + task_nap(order, 2, 0 - task_count(5));
  `${sum}:${order.length()}`
}
// A sleep is a place other tasks run: the count goes on while the timer
// waits, and the sleeper wakes while a long run of tasks hand over to one
// another, not only once they are all done.
function task_spin(order: int[], rounds: int) -> int {
  let total = 0;
  for (let i = 0; i < rounds; i += 1) {
    let f = spawn { task_count(200) };
    total += await f;
    if (order.length() > 0 && order[0] == 7) { return total - total + i - i + 1; }
  }
  0
}
function task_timer_during_handoffs() -> int {
  let order: int[] = [];
  let sleeper = spawn { task_nap(order, 7, 2) };
  let woke_during = task_spin(order, 5000000);
  (await sleeper) * 10 + woke_during
}
