// Convenience task client. It normalizes either a one-shot Generate provider
// or an external Harness into Generate; explicit session/event APIs remain
// available through run_harness/resume_harness/stream_harness.
class HarnessAgent {
  inner: Provider,

  implements Provider {
    function display_name(self) -> string throws never {
      "harness-agent:" + self.inner.display_name()
    }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }

  implements Generate {
    function generate<T>(self, request: Request<T>) -> Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      match (self.inner) {
        let generate: Generate => generate.generate<T>(request.for_provider(self.inner)),
        let harness: Harness => {
          let rebound = request.for_provider(self.inner);
          let session = harness.open(SessionOptions { attributes: {} });
          let run = harness.run<T>(session, rebound);
          run.session_token = harness.save_session(session);
          Response<T> { value: agent_run_value(run), meta: run.meta }
        },
        _ => throw baml.errors.Unsupported {
          message: "inner provider supports neither Generate nor Harness",
        },
      }
    }
  }

  implements Streaming {
    function stream<TPartial, T>(
      self,
      request: Request<T>,
    ) -> baml.llm.Stream<TPartial, T>
        throws baml.errors.StreamError | baml.errors.UnknownError {
      match (self.inner) {
        let streaming: Streaming => streaming.stream<TPartial, T>(request.for_provider(self.inner)),
        _ => throw baml.errors.Unsupported {
          message: "external harness events use stream_harness, not the partial-value stream",
        },
      }
    }
  }

  function raw(self) -> Provider throws never { self.inner }
}
