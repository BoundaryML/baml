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

function PassOptionalInt(x: int?) -> int? {
  x
}

function PassOptionalResume(r: Resume?) -> Resume? {
  r
}

function PassOptionalUnion(u: (int | string)?) -> (int | string)? {
  u
}
