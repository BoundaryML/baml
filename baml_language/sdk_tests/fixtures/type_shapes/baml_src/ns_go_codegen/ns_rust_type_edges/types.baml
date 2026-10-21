// `$rust_type` is builtin-only syntax, so user code exercises the opaque leaf
// through a constructible standard-library class that owns native state.
class ResponseEnvelope {
  response baml.http.Response
  optional_response baml.http.Response?
  responses baml.http.Response[]
  responses_by_name map<string, baml.http.Response>
}

function make_opaque_response(body: string) -> baml.http.Response {
  baml.http.Response.new(207, { "x-baml-go": "opaque" }, body.to_utf8())
}

function read_opaque_response(response: baml.http.Response) -> string throws baml.errors.Io | baml.errors.Timeout {
  response.text()
}

function round_trip_opaque_response(response: baml.http.Response) -> baml.http.Response {
  response
}

function round_trip_optional_opaque_response(response: baml.http.Response?) -> baml.http.Response? {
  response
}

function round_trip_opaque_responses(responses: baml.http.Response[]) -> baml.http.Response[] {
  responses
}

function round_trip_opaque_response_map(responses: map<string, baml.http.Response>) -> map<string, baml.http.Response> {
  responses
}

function round_trip_response_envelope(envelope: ResponseEnvelope) -> ResponseEnvelope {
  envelope
}

function defaulted_opaque_response(
  response: baml.http.Response = baml.http.Response.new(208, {}, "default body".to_utf8()),
) -> baml.http.Response {
  response
}

function invoke_opaque_response_callback(
  callback: (baml.http.Response) -> baml.http.Response,
  response: baml.http.Response,
) -> baml.http.Response {
  callback(response)
}
