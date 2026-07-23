class UsageMeter {
  entries: Meta[],

  function record(self, meta: Meta) -> null throws never {
    let _ = self.entries.push(meta);
    null
  }
  function calls(self) -> int throws never { self.entries.length() }
  function total(self) -> Usage throws never {
    aggregate_usage(self.entries) ?? Usage { input_tokens: 0, output_tokens: 0 }
  }
}

class Traced {
  inner: Provider,
  meter: UsageMeter,

  implements Provider {
    function display_name(self) -> string throws never { "traced" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
    function is_effectful(self) -> bool throws never { self.inner.is_effectful() }
  }

  implements Generate {
    function generate<T>(self, request: Request<T>) -> Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      match (self.inner) {
        let generate: Generate => {
          let response = generate.generate<T>(request.for_provider(self.inner));
          self.meter.record(response.meta);
          response
        },
        _ => throw baml.errors.Unsupported { message: "traced inner cannot generate" },
      }
    }
  }
}
