// Enum `==` the way a state machine uses it: each step compares a task's
// status against variants to pick the next one. 1,000 tasks, 40 rounds,
// about 110,000 comparisons.
enum Status {
  Todo
  Doing
  Review
  Done
}

class Task {
  id: int
  status: Status
}

function advance(s: Status, n: int) -> Status {
  if (s == Status.Todo) { return Status.Doing; }
  if (s == Status.Doing) {
    if (n % 3 == 0) { return Status.Review; }
    return Status.Doing;
  }
  if (s == Status.Review) {
    if (n % 5 == 0) { return Status.Doing; }
    return Status.Done;
  }
  Status.Todo
}

function main() -> int {
  let tasks: Task[] = [];
  for (let i = 0; i < 1000; i += 1) {
    tasks.push(Task { id: i, status: Status.Todo });
  }
  let done = 0;
  for (let round = 0; round < 40; round += 1) {
    for (let t in tasks) {
      t.status = advance(t.status, t.id + round);
      if (t.status == Status.Done) { done += t.id % 7 + 1; }
    }
  }
  done
}
