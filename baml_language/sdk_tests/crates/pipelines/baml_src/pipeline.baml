// Orchestration layer of the simulated call graph. Each function
// makes its own `baml.http.fetch` (more 1-second delays), then
// `match`es on the union returned by a leaf stage to decide what to
// call next. Edges:
//
//   HandleOrder(id)
//     └─ FetchOrder(id) -> Pending | Completed | Cancelled
//         ├─ Pending   → ProcessPending(p)   → fetch
//         ├─ Completed → ProcessCompleted(c) → fetch → ChargePayment(c.total)
//         │                                              -> Charged | Declined
//         └─ Cancelled → no further calls

function ProcessPending(p: Pending) -> string throws baml.errors.Io | baml.errors.Timeout {
  let resp = baml.http.fetch("https://httpbin.org/delay/1");
  if (resp.ok()) {
    "pending: queued"
  } else {
    "pending: enqueue failed"
  }
}

function ProcessCompleted(c: Completed) -> string throws baml.errors.Io | baml.errors.Timeout {
  let resp = baml.http.fetch("https://httpbin.org/delay/1");
  let payment = ChargePayment(c.total);
  match (payment) {
    let charged: Charged => "completed: charged",
    let declined: Declined => "completed: " + declined.reason
  }
}

function HandleOrder(id: int) -> string throws baml.errors.Io | baml.errors.Timeout {
  let resp = baml.http.fetch("https://httpbin.org/delay/3");
  let order = FetchOrder(id);
  let resp2 = baml.http.fetch("https://httpbin.org/delay/3");
  match (order) {
    let p: Pending => ProcessPending(p),
    let c: Completed => ProcessCompleted(c),
    let x: Cancelled => "cancelled: " + x.reason
  }
}
