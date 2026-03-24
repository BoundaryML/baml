// Exhaustive match patterns — no diagnostics expected
enum Direction {
  North
  South
  East
  West
}

// All variants covered
function AllVariants(d: Direction) -> string {
  match (d) {
    Direction.North => "north",
    Direction.South => "south",
    Direction.East => "east",
    Direction.West => "west"
  }
}

// Wildcard catch-all
function WildcardCatchAll(d: Direction) -> string {
  match (d) {
    Direction.North => "north",
    _ => "other"
  }
}

// Named catch-all
function NamedCatchAll(d: Direction) -> string {
  match (d) {
    Direction.North => "north",
    other => "other"
  }
}

// Boolean exhaustive
function BoolExhaustive(b: bool) -> string {
  match (b) {
    true => "yes",
    false => "no"
  }
}

// Optional with null
function OptionalExhaustive(x: int?) -> string {
  match (x) {
    n: int => "has value",
    null => "null"
  }
}

// Type alias union fully covered
class Ok {
  value string
}
class Err {
  message string
}
type MyResult = Ok | Err

function TypeAliasExhaustive(r: MyResult) -> string {
  match (r) {
    o: Ok => o.value,
    e: Err => e.message
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
