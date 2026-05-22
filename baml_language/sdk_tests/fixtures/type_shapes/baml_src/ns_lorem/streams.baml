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

function RoundTripResumeStream(r: Resume$stream) -> Resume$stream {
  r
}

function RoundTripRootFooStream(f: root.Foo$stream) -> root.Foo$stream {
  f
}

function RoundTripBamlHttpResponseStream(r: baml.http.Response$stream) -> baml.http.Response$stream {
  r
}

function RoundTripBoxOfResumeStream(b: Box<Resume$stream>) -> Box<Resume$stream> {
  b
}

function RoundTripListOfHttpResponse(xs: baml.http.Response[]) -> baml.http.Response[] {
  xs
}

function RoundTripResumeOrHttpResponse(u: Resume | baml.http.Response) -> Resume | baml.http.Response {
  u
}

function RoundTripResumeOrResumeStream(u: Resume | Resume$stream) -> Resume | Resume$stream {
  u
}
