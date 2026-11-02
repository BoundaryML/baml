// Tuples crossing generated class methods and BAML function handles: an
// instance method returning a tuple, instance and static methods taking one,
// and a BAML closure returned to the host whose argument and result are
// tuples.

class TuplePairBox {
  a int
  b string

  function as_pair(self) -> (int, string) {
    (self.a, self.b)
  }

  function with_pair(self, p: (int, string)) -> TuplePairBox {
    TuplePairBox { a: p[0], b: p[1] }
  }

  function from_pair(p: (int, string)) -> TuplePairBox {
    TuplePairBox { a: p[0], b: p[1] }
  }
}

// A BAML closure returned to the host: calling it through its handle sends a
// tuple argument and receives a tuple result.
function make_tuple_swapper() -> (t: (int, string)) -> (string, int) throws never {
  return (t: (int, string)) -> (string, int) { (t[1], t[0]) }
}
