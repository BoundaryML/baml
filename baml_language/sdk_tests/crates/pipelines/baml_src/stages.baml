// Leaf stages of the simulated call graph. Each one performs a
// `baml.http.fetch` against the httpbin 1-second-delay endpoint to
// stand in for a real network call, then returns one of several union
// arms based on its input. The pipeline functions in `pipeline.baml`
// dispatch on those arms.

function FetchOrder(id: int) -> Pending | Completed | Cancelled throws baml.errors.Io | baml.errors.Timeout {
  let resp = baml.http.fetch("https://httpbin.org/delay/1");
  if (resp.ok()) {
    if (id % 3 == 0) {
      Pending { id: id }
    } else if (id % 3 == 1) {
      Completed { id: id, total: 100 }
    } else {
      Cancelled { id: id, reason: "skipped" }
    }
  } else {
    Cancelled { id: id, reason: "fetch failed" }
  }
}

function ChargePayment(amount: int) -> Charged | Declined throws baml.errors.Io | baml.errors.Timeout {
  let resp = baml.http.fetch("https://httpbin.org/delay/1");
  if (resp.ok() && amount > 0) {
    Charged { amount: amount }
  } else {
    Declined { reason: "non-positive amount" }
  }
}
