// `Bar` #3 of 3 — see ../../../README.md.

class Bar {
  flavor string
  weight float
  active bool
}

function MakeFizzBuzzFooBar(flavor: string, weight: float, active: bool) -> Bar {
  Bar { flavor: flavor, weight: weight, active: active }
}

function RoundTripFizzBuzzFooBar(b: Bar) -> Bar {
  b
}
