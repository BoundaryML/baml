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

function ReturnLiteral42() -> 42 {
  42
}

function ReturnLiteralNegOne() -> -1 {
  -1
}

function ReturnLiteralDraft() -> "draft" {
  "draft"
}

function ReturnLiteralEscaped() -> "has \"quotes\"" {
  "has \"quotes\""
}

function ReturnLiteralTrue() -> true {
  true
}

function ReturnLiteralFalse() -> false {
  false
}

function RoundTripLiteral42(x: 42) -> 42 {
  x
}

function RoundTripLiteralDraft(x: "draft") -> "draft" {
  x
}

function RoundTripLiteralEscaped(x: "has \"quotes\"") -> "has \"quotes\"" {
  x
}

function RoundTripLiteralTrue(x: true) -> true {
  x
}

function RoundTripLiteralFalse(x: false) -> false {
  x
}

function RoundTripLiterals(l: Literals) -> Literals {
  l
}
