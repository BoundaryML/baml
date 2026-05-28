// `Bar` #1 of 3 — see ../README.md.

class Bar {
  label string
  count int
}

function make_foo_bar(label: string, count: int) -> Bar {
  Bar { label: label, count: count }
}

function round_trip_foo_bar(b: Bar) -> Bar {
  b
}
