// B-624: throws from calls nested in match arms must contribute to the
// enclosing match expression's catch_all domain.

class NumV {
  n int
}

class BoolV {
  b bool
}

function ev(x: string) -> NumV | BoolV throws baml.errors.ParseError | baml.errors.InvalidArgument {
  if (x == "parse") {
    throw baml.errors.ParseError { message: x }
  }
  if (x == "invalid") {
    throw baml.errors.InvalidArgument { message: x }
  }
  if (x == "num") {
    NumV { n: 1 }
  } else {
    BoolV { b: true }
  }
}

function PlainCall(x: string) -> string {
  ev(x).to_string() catch_all (e) {
    let pe: baml.errors.ParseError => "parse",
    let ie: baml.errors.InvalidArgument => "type",
    _ => "other",
  }
}

function MatchWrapped(x: string) -> string {
  match (ev(x)) {
    NumV { n: let n } => n.to_string(),
    BoolV { b: let b } => b.to_string(),
  } catch_all (e) {
    let pe: baml.errors.ParseError => "parse",
    let ie: baml.errors.InvalidArgument => "type",
    _ => "other",
  }
}

// The same two forms also agree when the known throw types are handled
// without a wildcard: neither has a spurious `unknown` tail.
function PlainCallWithoutWildcard(x: string) -> string {
  ev(x).to_string() catch_all (e) {
    let pe: baml.errors.ParseError => "parse",
    let ie: baml.errors.InvalidArgument => "type",
  }
}

function MatchWrappedWithoutWildcard(x: string) -> string {
  match (ev(x)) {
    NumV { n: let n } => n.to_string(),
    BoolV { b: let b } => b.to_string(),
  } catch_all (e) {
    let pe: baml.errors.ParseError => "parse",
    let ie: baml.errors.InvalidArgument => "type",
  }
}

// A genuinely unknown-throwing callback remains unknown both at top level and
// when nested in match arms, so wildcard catch_all arms stay reachable.
function UnknownPlain(cb: (value: int) -> string throws unknown) -> string {
  cb(1) catch_all (e) {
    _ => "other",
  }
}

function UnknownMatchWrapped(
  cb: (value: int) -> string throws unknown,
  select: bool,
) -> string {
  match (select) {
    true => cb(1),
    false => cb(2),
  } catch_all (e) {
    _ => "other",
  }
}

// Omitting the wildcard from either genuinely-unknown form is non-exhaustive.
function UnknownPlainWithoutWildcard(
  cb: (value: int) -> string throws unknown,
) -> string {
  cb(1) catch_all (e) {
    let pe: baml.errors.ParseError => "parse",
  }
}

function UnknownMatchWrappedWithoutWildcard(
  cb: (value: int) -> string throws unknown,
  select: bool,
) -> string {
  match (select) {
    true => cb(1),
    false => cb(2),
  } catch_all (e) {
    let pe: baml.errors.ParseError => "parse",
  }
}

//----
//- diagnostics
// E0063
//
//   ⚠ unreachable arm
//     ╭─[catch_match_arm_throw_propagation.baml:30:10]
//  30 │     _ => "other",
//     ·          ───────
//     ╰────
// E0063
//
//   ⚠ unreachable arm
//     ╭─[catch_match_arm_throw_propagation.baml:41:10]
//  41 │     _ => "other",
//     ·          ───────
//     ╰────
// E0062
//
//   × non-exhaustive catch_all on type unknown; missing: unknown
//     ╭─[catch_match_arm_throw_propagation.baml:88:3]
//  88 │ ╭─▶   cb(1) catch_all (e) {
//  89 │ │       let pe: baml.errors.ParseError => "parse",
//  90 │ ╰─▶   }
//     ╰────
// E0062
//
//   × non-exhaustive catch_all on type unknown; missing: unknown
//      ╭─[catch_match_arm_throw_propagation.baml:97:3]
//   97 │ ╭─▶   match (select) {
//   98 │ │       true => cb(1),
//   99 │ │       false => cb(2),
//  100 │ │     } catch_all (e) {
//  101 │ │       let pe: baml.errors.ParseError => "parse",
//  102 │ ╰─▶   }
//      ╰────
