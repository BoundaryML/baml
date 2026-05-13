// Union variants returned by the leaf pipeline stages. The pipeline
// functions in `pipeline.baml` `match` on these to decide which
// downstream call to make, so the union arms are the edges of the
// simulated call graph.

// Possible outcomes of `FetchOrder`.
class Pending {
  id int
}

class Completed {
  id int
  total int
}

class Cancelled {
  id int
  reason string
}

// Possible outcomes of `ChargePayment`.
class Charged {
  amount int
}

class Declined {
  reason string
}
