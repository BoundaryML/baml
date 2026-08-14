// Go has no associated functions. Every method below intentionally omits
// `self`, so the Go SDK must expose it as a collision-safe package helper
// while dispatching the exact class-method FQN over the bridge.

class StaticPayload {
  value string
}

enum StaticMood {
  HAPPY
  SAD
}

type StaticPayloadAlias = StaticPayload

class StaticMethodError {
  message string
}

class Edge {
  function round_trip(value: string) -> string {
    value
  }

  function required(value: string) -> string {
    value
  }

  function defaulted(required: string, optional: string = "default") -> string[] {
    [required, optional]
  }

  function nullable(value: int?) -> int? {
    value
  }

  function list(value: string[]) -> string[] {
    value
  }

  function mapping(value: map<string, int>) -> map<string, int> {
    value
  }

  function protected_locals(
    ctx_: string,
    err_: string,
    result_: string,
    zero_: string,
    bootstrap: string,
    receiver_: string,
    arguments_: string,
  ) -> string[] {
    [ctx_, err_, result_, zero_, bootstrap, receiver_, arguments_]
  }

  function class_value(value: StaticPayload) -> StaticPayload {
    value
  }

  function alias_value(value: StaticPayloadAlias) -> StaticPayloadAlias {
    value
  }

  function enum_value(value: StaticMood) -> StaticMood {
    value
  }

  function union_value(value: string | int) -> string | int {
    value
  }

  function image_value(value: image) -> image {
    value
  }

  function json_value(value: baml.json.json) -> baml.json.json {
    value
  }

  function type_value(value: type) -> type {
    value
  }

  function rust_type_value(value: baml.http.Response) -> baml.http.Response {
    value
  }

  function make_response(body: string) -> baml.http.Response {
    baml.http.Response.new(209, { "x-static": "true" }, body.to_utf8())
  }

  function read_response(value: baml.http.Response) -> string throws baml.errors.Io {
    value.text()
  }

  function throw_error(message: string) -> string throws StaticMethodError {
    throw StaticMethodError { message: message }
  }

  function panic(message: string) -> never {
    baml.sys.panic(message)
  }

  function sleep_ms(ms: int) -> null throws baml.errors.Io {
    baml.sys.sleep(baml.time.Duration.from_nanoseconds(ms * 1000000n))
  }
}

// Normalizes to the same Go package declaration base as Edge.round_trip.
// Typed package allocation must disambiguate it without changing either wire
// identity.
function edge_round_trip(value: string) -> string {
  value
}

class EdgeRound {
  function trip(value: string) -> string {
    value
  }
}
