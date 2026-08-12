// `Bar` #3 of 3 — see ../../../README.md.

class Bar {
  flavor string
  weight float
  active bool
}

function make_fizz_buzz_foo_bar(flavor: string, weight: float, active: bool) -> Bar {
  Bar { flavor: flavor, weight: weight, active: active }
}

function round_trip_fizz_buzz_foo_bar(b: Bar) -> Bar {
  b
}
