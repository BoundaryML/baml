// `Bar` #2 of 3 — see ../../README.md.

class Bar {
  tag string
  ratio float
}

function make_fizz_foo_bar(tag: string, ratio: float) -> Bar {
  Bar { tag: tag, ratio: ratio }
}

function round_trip_fizz_foo_bar(b: Bar) -> Bar {
  b
}
