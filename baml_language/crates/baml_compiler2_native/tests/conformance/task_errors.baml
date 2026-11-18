// Tasks beside errors and panics that are class instances, maps and slices:
// what a task throws is held by its future and thrown again by an `await`
// as a handler in the task would have had it, and what a task returns or a
// spawner holds stays good across the collections of either. As in
// `tasks.baml`, no case depends on whether a task starts before its spawner
// goes on.

class TaskFault {
  code: int
  detail: string
}

function task_fault(code: int) -> int throws TaskFault {
  if code > 0 { throw TaskFault { code: code, detail: rep("d", code) } }
  0
}
// The throw is in the function the frames are counted of: a function
// inlined into its caller has no frame of its own in native code.
function task_fault_dive(depth: int, code: int) -> int throws TaskFault {
  if depth > 0 {
    return task_fault_dive(depth - 1, code) + 1;
  }
  if code > 0 { throw TaskFault { code: code, detail: rep("d", code) } }
  0
}
function task_ints(n: int) -> int[] {
  let items = [0];
  for (let i = 1; i < n; i += 1) { items.push(i * 3); };
  items
}
function task_item(items: int[], i: int) -> int { items[i] }
function task_churn(n: int) -> int {
  let t = "";
  for (let i = 0; i < n; i += 1) { t = rep("z", i % 9); };
  t.length()
}

// A class instance a task throws is caught where its future is awaited, by
// its class, with its fields.
function task_error_by_class() -> int {
  let f = spawn { task_fault(task_count(3)) };
  (await f) catch (e) {
    TaskFault => {
      let detail = e.detail;
      e.code * 10 + detail.length()
    },
  }
}
function task_error_other_class() -> int {
  let f = spawn { task_fault(task_count(2)) };
  (await f) catch (e) {
    TaskError => -1,
    TaskFault => e.code,
  }
}
function task_error_catch_all() -> int {
  let f = spawn { task_fault(task_count(2)) };
  (await f) catch (e) {
    _ => 8,
  }
}
// The error is the future's while both tasks allocate, and until the second
// `await` of it.
function task_error_kept() -> string {
  let f = spawn { task_fault(task_count(5)) };
  let made = task_churn(40);
  let g = spawn { task_churn(30) + made };
  let n = await g;
  let first = (await f) catch (e) {
    TaskFault => e.detail,
  };
  let again = task_churn(20);
  let second = (await f) catch (e) {
    TaskFault => `${e.code}${e.detail}`,
  };
  `${n}|${first}|${again}|${second}`
}
// A task that awaits a failed task and catches nothing fails with the same
// error.
function task_error_through_a_task() -> int {
  let f = spawn { task_fault(task_count(4)) };
  let g = spawn { (await f) + 1 };
  (await g) catch (e) {
    TaskFault => e.code,
  }
}

// A panic in a task is caught where its future is awaited: by its class, and
// by the name of every panic.
function task_panic_index() -> int {
  let items = task_ints(3);
  let f = spawn { task_item(items, task_count(5)) };
  (await f) catch (e) {
    baml.panics.IndexOutOfBounds => e.index * 100 + e.length,
  }
}
function task_panic_map_key() -> int {
  let m = map_squares(4);
  let f = spawn { m[map_key(task_count(9))] };
  (await f) catch (e) {
    baml.panics.MapKeyNotFound => 7,
  }
}
function task_panic_alias_index() -> int {
  let items = task_ints(3);
  let f = spawn { task_item(items, task_count(5)) };
  (await f) catch (e) {
    baml.panics.Panic => 9,
  }
}
function task_panic_alias_map_key() -> int {
  let m = map_squares(4);
  let f = spawn { m[map_key(task_count(9))] };
  (await f) catch (e) {
    baml.panics.Panic => 9,
  }
}
function task_panic_other_class() -> int {
  let items = task_ints(3);
  let f = spawn { task_item(items, task_count(5)) };
  (await f) catch (e) {
    baml.panics.DivisionByZero => 1,
    baml.panics.IndexOutOfBounds => 2,
  }
}
// A handler that takes every error lets a task's panic by, in the function
// that awaits and in the one that calls it.
function task_await_taking_all(f: baml.future.Future<int, TaskFault>) -> int {
  (await f) catch (e) {
    _ => -1,
  }
}
function task_panic_passes_catch_all() -> int {
  let items = task_ints(3);
  let panics = spawn { task_item(items, task_count(5)) + task_fault(0) };
  let fails = spawn { task_fault(task_count(1)) };
  let passed = task_await_taking_all(panics) catch (e) {
    baml.panics.Panic => 5,
  };
  passed * 10 + task_await_taking_all(fails)
}
function task_panic_passes_local_catch_all() -> int {
  let m = map_squares(4);
  let f = spawn { m[map_key(task_count(9))] + task_fault(0) };
  {
    (await f) catch (e) {
      _ => -1,
    }
  } catch (e) {
    baml.panics.MapKeyNotFound => 6,
  }
}
// Nothing catches the panic: it ends the program as it would have ended the
// task.
function task_panic_uncaught_index() -> int {
  let items = task_ints(3);
  let f = spawn { task_item(items, task_count(5)) };
  await f
}
function task_panic_uncaught_map_key() -> int {
  let m = map_squares(4);
  let f = spawn { m[map_key(task_count(9))] };
  await f
}
function task_error_uncaught() -> int {
  let f = spawn { task_fault(task_count(2)) };
  await f
}
// A task that recursed past its stack panics, and its spawner goes on.
// native prints: 3
function task_overflow_caught() -> int {
  let f = spawn { task_dive(task_count(20000)) };
  let g = spawn { task_count(2) };
  let r = (await f) catch (e) {
    baml.panics.StackOverflow => {
      let message = e.message;
      message.length() - 13
    },
  };
  r + (await g)
}

// A handler at an `await` that binds the context reads the error and the
// functions the task threw it through.
function task_context_error() -> int {
  let f = spawn { task_fault(task_count(4)) };
  (await f) catch (e, context) {
    _ => match (context.error) {
      let thrown: TaskFault => thrown.code,
      _ => -1,
    },
  }
}
function task_context_cause() -> int {
  let f = spawn { task_fault(task_count(4)) };
  (await f) catch (e, context) {
    _ => if context.cause == null { 1 } else { 2 },
  }
}
function task_context_frames() -> int {
  let f = spawn { task_fault_dive(task_count(3), 2) };
  (await f) catch (e, context) {
    TaskFault => context.stack_trace.frames.length() * 10 + e.code,
  }
}
function task_context_innermost() -> string {
  let f = spawn { `${task_fault_dive(task_count(3), 2)}` };
  (await f) catch (e, context) {
    _ => context.stack_trace.frames[0].function_name,
  }
}
function task_context_await_in(f: baml.future.Future<int, TaskFault>) -> int throws TaskFault {
  (await f) + 1
}
function task_context_through_a_frame() -> int {
  let f = spawn { task_fault_dive(task_count(2), 1) };
  task_context_await_in(f) catch (e, context) {
    TaskFault => context.stack_trace.frames.length() * 10 + e.code,
  }
}
function task_context_of_a_panic() -> int {
  let items = task_ints(3);
  let f = spawn { task_item(items, task_count(5)) };
  (await f) catch (e, context) {
    baml.panics.Panic => match (context.error) {
      let thrown: baml.panics.IndexOutOfBounds => thrown.index,
      _ => -1,
    },
  }
}

// A task builds a map, and a string that is a slice of another, and returns
// it to the task that awaits it.
function task_returns_map() -> string {
  let f = spawn { map_squares(task_count(12)) };
  let made = task_churn(30);
  let m = await f;
  m[map_key(12)] = made;
  map_show(m)
}
function task_returns_int_map() -> string {
  let f = spawn {
    let m = map_ints(task_count(20));
    m.delete(-50);
    m
  };
  map_show_ints(await f)
}
function task_returns_slice() -> string {
  let f = spawn { slice_kept(task_count(3)) };
  let g = spawn { slice_kept_wide(task_count(12)) };
  let made = task_churn(30);
  `${await f}|${await g}|${made}`
}
function task_returns_holder() -> string {
  let f = spawn { MapHolder { name: slice_kept(2), counts: map_squares(task_count(5)) } };
  let made = task_churn(30);
  let h = await f;
  `${h.name}|${map_show(h.counts)}|${made}`
}

// What the spawner holds (a map, a slice, a class instance) stays good while
// its tasks and it allocate, across the spawn and the await.
function task_parent_keeps() -> string {
  let m = map_squares(10);
  let part = slice_kept(2);
  let fault = TaskFault { code: task_count(6), detail: rep("x", 3) };
  let f = spawn {
    let made = map_squares(20);
    made.length() + task_churn(60)
  };
  let between = task_churn(40);
  let g = spawn { m[map_key(3)] + task_churn(25) };
  let n = (await f) + (await g);
  `${map_show(m)}|${part}|${fault.code}${fault.detail}|${n}|${between}`
}
// The task writes what the spawner holds, and the spawner reads it after.
function task_parent_shares() -> string {
  let m = map_squares(3);
  let holder = MapHolder { name: slice_kept(2), counts: m };
  let f = spawn {
    for (let i = 3; i < 30; i += 1) { m[map_key(i)] = task_churn(i); };
    holder.name = slice_kept(3);
    0
  };
  let between = task_churn(40);
  let r = await f;
  `${holder.counts.length()}|${holder.name}|${m[map_key(29)]}|${between + r}`
}
// The same with a task that fails after it has written.
function task_parent_keeps_past_a_failure() -> string {
  let m = map_squares(6);
  let part = slice_kept(2);
  let f = spawn {
    m[map_key(40)] = task_churn(30);
    task_fault(m.length())
  };
  let between = task_churn(40);
  let code = (await f) catch (e) {
    TaskFault => {
      let detail = e.detail;
      e.code * 100 + detail.length()
    },
  };
  `${map_show(m)}|${part}|${code}|${between}`
}
