// Regression fixture for issue #4059: BAML identifiers that are Python
// reserved words must generate importable, round-trippable Python.
//
// Covers the 24 hard keywords BAML admits at source; the other 11 hard keywords
// are also BAML keywords and are rejected by the grammar. NAME positions (class
// / enum / type-alias names) use the three uppercase-initial keywords BAML
// allows as type names: None, True, False. Python SOFT keywords (`case`, `type`)
// are valid identifiers and appear as negative controls that must NOT be escaped.

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
