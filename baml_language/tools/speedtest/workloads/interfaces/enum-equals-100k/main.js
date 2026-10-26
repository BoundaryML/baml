const Status = Object.freeze({ Todo: "Todo", Doing: "Doing", Review: "Review", Done: "Done" });
class Task { constructor(id, status) { this.id = id; this.status = status; } }
function advance(s, n) {
  if (s === Status.Todo) return Status.Doing;
  if (s === Status.Doing) { if (n % 3 === 0) return Status.Review; return Status.Doing; }
  if (s === Status.Review) { if (n % 5 === 0) return Status.Doing; return Status.Done; }
  return Status.Todo;
}
function main() {
  const tasks = [];
  for (let i = 0; i < 1000; i++) tasks.push(new Task(i, Status.Todo));
  let done = 0;
  for (let round = 0; round < 40; round++) {
    for (const t of tasks) {
      t.status = advance(t.status, t.id + round);
      if (t.status === Status.Done) done += t.id % 7 + 1;
    }
  }
  return done;
}
