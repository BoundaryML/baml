// === Heterogeneous stored callback collection ===
// Array literal should widen element types via union.

class Task<E> {
  name: string
  run: () -> null throws E
}

function run_tasks(tasks: Task<string | int>[]) -> null {
  for (let task in tasks) {
    task.run()
  }
  null
}

function run_tasks_alias(tasks: Task<string | int>[]) -> null {
  let alias = tasks
  for (let task in alias) {
    task.run()
  }
  null
}

function make_mixed_tasks() -> Task<string | int>[] {
  [
    Task { name: "a", run: () -> null { throw "error" } },
    Task { name: "b", run: () -> null { throw 42 } },
  ]
}

function run_local_tasks() -> null {
  let tasks = make_mixed_tasks()
  for (let task in tasks) {
    task.run()
  }
  null
}

function run_inline_tasks() -> null {
  let tasks = [
    Task { name: "a", run: () -> null { throw "error" } },
    Task { name: "b", run: () -> null { throw 42 } },
  ]
  for (let task in tasks) {
    task.run()
  }
  null
}

function test_run_mixed_tasks() -> null {
  run_tasks(make_mixed_tasks())
}

function test_run_tasks_alias() -> null {
  run_tasks_alias(make_mixed_tasks())
}

function test_run_local_tasks() -> null {
  run_local_tasks()
}

function test_run_inline_tasks() -> null {
  run_inline_tasks()
}
