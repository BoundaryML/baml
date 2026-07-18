// Adversarial host-SDK coverage for class instance methods and `Self`.
// The methods intentionally exercise nested type positions, nullable defaults,
// transparent aliases, generator-owned local names, and Go name projection
// collisions. Receiver-free static methods are covered by
// `ns_static_method_edges` through package-level Go helpers.

class MethodSelfEdges {
  round_trip string

  function clone(self) -> Self {
    self
  }

  function nullable(self, value: Self? = null) -> Self? {
    value
  }

  function list(self, values: Self[]) -> Self[] {
    values
  }

  function map(self, values: map<string, Self>) -> map<string, Self> {
    values
  }

  function union(self, value: Self | string) -> Self | string {
    value
  }

  // Collides with the projected Go field `RoundTrip`.
  function roundTrip(self) -> string {
    self.round_trip
  }

  // Collides with the generator-owned class metadata method.
  function BAMLClassName(self) -> string {
    self.round_trip
  }

  function protected_locals(
    self,
    ctx_: string,
    err_: string,
    result_: string,
    zero_: string,
    bootstrap: string,
    receiver_: string,
  ) -> string[] {
    [ctx_, err_, result_, zero_, bootstrap, receiver_]
  }

  function alias(self, value: MethodSelfAlias) -> MethodSelfAlias {
    value
  }

  function throw_error(self, message: string) -> string throws MethodSelfError {
    throw MethodSelfError { message: self.round_trip + ":" + message }
  }

  function panic(self, message: string) -> never {
    baml.sys.panic(self.round_trip + ":" + message)
  }

  function sleep_ms(self, ms: int) -> null throws baml.errors.Io {
    baml.sys.sleep(baml.time.Duration.from_nanoseconds(ms * 1000000n))
  }

  // The default compares against a receiver field, so a successful call
  // simultaneously proves receiver injection, default wiring, and media
  // input/output codecs.
  function round_trip_image(self, value: image, expected_receiver: string = "edge") -> image {
    if (self.round_trip == expected_receiver) {
      value
    } else {
      baml.sys.panic("wrong method receiver")
    }
  }
}

type MethodSelfAlias = MethodSelfEdges

class MethodSelfError {
  message string
}

class EmptyMethodSelf {
  function identity(self) -> Self {
    self
  }
}
