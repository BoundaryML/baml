// Group E: field attrs in non-field type positions.
// Field attrs should produce diagnostics; type attrs are valid.

class EFoo { x int }
class EA { x int }
class EB { x int }
class EC { x int }

// E1: type alias with field attr — ERROR
type E1 = EFoo @alias("e1_alias")

// E2: type alias with type attr — valid
type E2 = EFoo @stream.done

// E3: type alias with both — ERROR for @alias, @stream.done is fine
type E3 = EFoo @alias("e3_alias") @stream.done

// E4: type alias union with field attr — ERROR
type E4 = EA | EB | EC @alias("e4_alias")

// E5: function param with field attr — ERROR
function E5(x: EFoo @alias("e5_alias")) -> string {
  "hello"
}

// E6: function param with type attr — valid
function E6(x: EFoo @stream.done) -> string {
  "hello"
}

// E7: function return type with field attr — ERROR
function E7() -> EFoo @alias("e7_alias") {
  EFoo { x: 1 }
}

// E8: function return type with type attr — valid
function E8() -> EFoo @stream.done {
  EFoo { x: 1 }
}
