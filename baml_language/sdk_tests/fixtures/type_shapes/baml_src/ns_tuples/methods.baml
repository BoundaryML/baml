
class TuplePairBox {
  a int
  b string

  function as_pair(self) -> (int, string) {
    (self.a, self.b)
  }

  function with_pair(self, p: (int, string)) -> TuplePairBox {
    TuplePairBox { a: p.0, b: p.1 }
  }

  function from_pair(p: (int, string)) -> TuplePairBox {
    TuplePairBox { a: p.0, b: p.1 }
  }
}

function make_tuple_swapper() -> (t: (int, string)) -> (string, int) throws never {
  return (t: (int, string)) -> (string, int) { (t.1, t.0) }
}
