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

function RoundTripOptionalInt(x: int?) -> int? {
  x
}

function RoundTripOptionalResume(r: Resume?) -> Resume? {
  r
}

function RoundTripOptionalUnion(u: (int | string)?) -> (int | string)? {
  u
}

function RoundTripResume(r: Resume) -> Resume {
  r
}

function RoundTripOptionalContainer(c: OptionalContainer) -> OptionalContainer {
  c
}
