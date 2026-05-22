// `Bar` #1 of 3 — see ../README.md.

class Bar {
  label string
  count int
}

function MakeFooBar(label: string, count: int) -> Bar {
  Bar { label: label, count: count }
}

function RoundTripFooBar(b: Bar) -> Bar {
  b
}
