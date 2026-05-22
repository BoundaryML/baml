// `Bar` #2 of 3 — see ../../README.md.

class Bar {
  tag string
  ratio float
}

function MakeFizzFooBar(tag: string, ratio: float) -> Bar {
  Bar { tag: tag, ratio: ratio }
}

function RoundTripFizzFooBar(b: Bar) -> Bar {
  b
}
