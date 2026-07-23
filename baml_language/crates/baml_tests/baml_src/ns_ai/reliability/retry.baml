class Retry {
  inner: Provider,
  max: int,

  implements Provider {
    function display_name(self) -> string throws never { "retry" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
    function is_effectful(self) -> bool throws never { self.inner.is_effectful() }
  }

  implements Generate {
    function generate<T>(self, request: Request<T>) -> Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      let attempt = 0;
      if (self.inner.is_effectful()) {
        throw baml.errors.Unsupported {
          message: "retry refuses an effectful provider without a durable idempotency contract",
        };
      }
      while (true) {
        match (self.inner) {
          let generate: Generate => {
            let response = generate.generate<T>(request.for_provider(self.inner)) catch (e) {
              _ => {
                let replayable = thrown_error_is_replayable(e);
                if (replayable && attempt < self.max) {
                  attempt = attempt + 1;
                  continue;
                }
                throw e;
              },
            };
            return response;
          },
          _ => throw baml.errors.Unsupported { message: "retry inner cannot generate" },
        }
      }
      baml.sys.panic("unreachable")
    }
  }

  implements Streaming {
    function stream<TPartial, T>(self, request: Request<T>) -> baml.llm.Stream<TPartial, T>
        throws baml.errors.StreamError | baml.errors.UnknownError {
      match (self.inner) {
        let streaming: Streaming => streaming.stream<TPartial, T>(request.for_provider(self.inner)),
        _ => throw baml.errors.Unsupported { message: "retry inner cannot stream" },
      }
    }
  }
}
