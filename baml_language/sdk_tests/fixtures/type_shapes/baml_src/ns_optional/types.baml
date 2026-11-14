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

// One request before and after a release adds two optional fields. Rust
// callers build both with `SetupLinkRequest::new(project_id, name)`, so
// adding an optional field breaks no caller that uses the constructor.
class SetupLinkRequest {
  project_id string
  name string
  max_deployments int?
}

class SetupLinkRequestNext {
  project_id string
  name string
  max_deployments int?
  expires_in_days int?
  label string?
}

function round_trip_setup_link_request(r: SetupLinkRequest) -> SetupLinkRequest {
  r
}

function round_trip_setup_link_request_next(r: SetupLinkRequestNext) -> SetupLinkRequestNext {
  r
}
