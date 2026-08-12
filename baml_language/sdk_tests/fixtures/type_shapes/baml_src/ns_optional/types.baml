// Optional rows from 18a §"Type shapes":
//   - `int?`                       → `typing.Union[int, None]`
//   - `Resume?` (bare class)       → `typing.Union[Resume, None]`
//   - `(int | string)?`            → `typing.Union[int, str, None]`

class Resume {
  name string
}

class OptionalContainer {
  optional_int int?
  optional_class Resume?
  optional_union (int | string)?
}

function round_trip_optional_int(x: int?) -> int? {
  x
}

function round_trip_optional_resume(r: Resume?) -> Resume? {
  r
}

function round_trip_optional_union(u: (int | string)?) -> (int | string)? {
  u
}

function round_trip_resume(r: Resume) -> Resume {
  r
}

function round_trip_optional_container(c: OptionalContainer) -> OptionalContainer {
  c
}
