// Regression fixture for issue #4059: BAML identifiers that are Python
// reserved words must generate importable, round-trippable Python.
//
// Covers the 24 hard keywords BAML admits at source; the other 11 hard keywords
// are also BAML keywords and are rejected by the grammar. NAME positions (class
// / enum / type-alias names) admit those keywords too — both the uppercase-initial
// None / True / False and lowercase ones like pass / import / lambda — all through
// the same escape path. Python SOFT keywords (`case`, `type`) are valid
// identifiers and appear as negative controls that must NOT be escaped.

// (a) enum members — the headline #4059 case (`None` etc. would emit a bare
//     `None = "None"` and fail to import).
enum Kw {
  False
  None
  True
  and
  as
  assert
  async
  def
  del
  elif
  except
  finally
  from
  global
  import
  lambda
  nonlocal
  not
  or
  pass
  raise
  try
  with
  yield
  // soft keyword — valid Python identifier, must NOT be escaped (control)
  case
}

// (b) class fields, including the `lambda` silent-corruption trap (a bare
//     `lambda: int` parses as a lambda expression and drops the field).
class Fields {
  pass int
  lambda int
  from int
  global int
  del int
  async int
  // soft keyword — valid Python identifier, must NOT be escaped (control)
  type int
}

// (c) keyword class name.
class None {
  value int
}

// (d) keyword enum name.
enum True {
  A
  B
}

// (e) keyword type-alias name.
type False = int

// (f) LOWERCASE keyword NAMES — the same escape path as the uppercase trio above.
//     `class pass` is referenced by another class field so cross-reference
//     escaping (`p: pass_`) is exercised, not just the definition site.
class pass {
  value int
}

class PassHolder {
  p pass
}

// (g) lowercase keyword enum name.
enum import {
  A
  B
}

// (h) lowercase keyword type-alias name.
type lambda = int

// Identity round-trips so the sdk-test pytest exercises encode + decode of
// escaped enum members and class fields end-to-end through the engine.
function round_trip_kw(k: Kw) -> Kw {
  k
}

function round_trip_fields(f: Fields) -> Fields {
  f
}

// NAME-position round-trip: proves the engine's raw wire FQN `user.None` decodes
// back to the escaped Python class `None_` end-to-end through the engine, not
// only in unit probes.
function round_trip_none(n: None) -> None {
  n
}

// Lowercase NAME-position round-trip: same proof for `class pass` (raw wire FQN
// `user.pass` -> escaped Python class `pass_`), covering the lowercase escape
// path end-to-end through the engine.
function round_trip_pass(p: pass) -> pass {
  p
}
