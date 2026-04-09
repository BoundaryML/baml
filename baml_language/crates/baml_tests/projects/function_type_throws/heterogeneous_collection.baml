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

function make_mixed_tasks() -> Task<string | int>[] {
  [
    Task { name: "a", run: () -> null { throw "error" } },
    Task { name: "b", run: () -> null { throw 42 } },
  ]
}

function test_run_mixed_tasks() -> null {
  run_tasks(make_mixed_tasks())
}
