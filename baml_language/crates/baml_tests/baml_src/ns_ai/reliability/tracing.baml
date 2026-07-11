class UsageMeter {
  entries: Meta[],

  function record(self, meta: Meta) -> null throws never {
    let _ = self.entries.push(meta);
    null
  }
  function calls(self) -> int throws never { self.entries.length() }
  function total(self) -> Usage throws never {
    let input = 0;
    let output = 0;
    for (let entry in self.entries) {
      match (entry.usage) {
        let usage: Usage => {
          input = input + usage.input_tokens;
          output = output + usage.output_tokens;
        },
        null => {},
      }
    }
    Usage { input_tokens: input, output_tokens: output }
  }
}

class Traced {
  inner: Provider,
  meter: UsageMeter,

  implements Provider {
    function display_name(self) -> string throws never { "traced" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
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
