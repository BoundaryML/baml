// Literal Ty variants — int (positive + negative), string (plain + escaped),
// bool true/false. Float literals are deliberately skipped (18a §"Ignore
// coverage": Python `Literal` rejects floats, so they degrade to `Any`).
//
// `Literals` is the field-bearing class that pyright will see; the
// `Return*` functions exercise the BAML frontend but are not emitted
// into Python — see 18b.

class Literals {
  literal_42 42
  // literal_neg_one -1 — parser rejects bare `-1` in field-type position;
  // see 18b §"negative literal in field-type position".
  literal_draft "draft"
  literal_escaped "has \"quotes\""
  literal_true true
  literal_false false
}

function return_literal42() -> 42 {
  42
}

function return_literal_neg_one() -> -1 {
  -1
}

function return_literal_draft() -> "draft" {
  "draft"
}

function return_literal_escaped() -> "has \"quotes\"" {
  "has \"quotes\""
}

function return_literal_true() -> true {
  true
}

function return_literal_false() -> false {
  false
}

function round_trip_literal42(x: 42) -> 42 {
  x
}

function round_trip_literal_draft(x: "draft") -> "draft" {
  x
}

function round_trip_literal_escaped(x: "has \"quotes\"") -> "has \"quotes\"" {
  x
}

function round_trip_literal_true(x: true) -> true {
  x
}

function round_trip_literal_false(x: false) -> false {
  x
}

function round_trip_literals(l: Literals) -> Literals {
  l
}

// Mixed-base literal union: every alternative is a literal of a different
// base type, so hosts with typed literals must dispatch by value.
type Flag = "active" | 1 | true

function round_trip_flag(f: Flag) -> Flag {
  f
}
