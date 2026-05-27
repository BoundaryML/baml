// 18a §"Routing rules / stream types" — leaf `lorem`. Each function's
// signature pins one `route_class_ref(name)` call site:
//
//   - `Resume$stream` from `lorem`               → `stream_types.lorem.Resume`
//   - `root.Foo$stream` from `lorem`             → `stream_types.Foo`
//   - `baml.http.Response$stream` from `lorem`   → `stream_types.baml.http.Response`
//
// And the cross-leaf composition cases from 18a:
//   - `Box<Resume$stream>` from `lorem`          → `Box[stream_types.lorem.Resume]`
//   - `baml.http.Response[]` from `lorem`        → `typing.List[baml.http.Response]`
//   - `Resume | baml.http.Response` from `lorem` → `typing.Union[Resume, baml.http.Response]`
//   - `Resume | Resume$stream` from `lorem`      → `typing.Union[Resume, stream_types.lorem.Resume]`
//
// `Box<T>` is declared here so the cross-leaf composition test stays in
// the same leaf as the use site.

class Box<T> {
  v T
}

function round_trip_resume_stream(r: Resume$stream) -> Resume$stream {
  r
}

function round_trip_root_foo_stream(f: root.Foo$stream) -> root.Foo$stream {
  f
}

function round_trip_baml_http_response_stream(r: baml.http.Response$stream) -> baml.http.Response$stream {
  r
}

function round_trip_box_of_resume_stream(b: Box<Resume$stream>) -> Box<Resume$stream> {
  b
}

function round_trip_list_of_http_response(xs: baml.http.Response[]) -> baml.http.Response[] {
  xs
}

function round_trip_resume_or_http_response(u: Resume | baml.http.Response) -> Resume | baml.http.Response {
  u
}

function round_trip_resume_or_resume_stream(u: Resume | Resume$stream) -> Resume | Resume$stream {
  u
}
