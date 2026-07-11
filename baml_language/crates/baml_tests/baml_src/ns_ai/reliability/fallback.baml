class Fallback {
  members: Provider[],

  implements Provider {
    function display_name(self) -> string throws never { "fallback" }
    function render_shorthand(self) -> string throws never {
      match (self.members.at(0)) {
        let first: Provider => first.render_shorthand(),
        null => "openai/gpt-5.6-luna",
      }
    }
    function is_effectful(self) -> bool throws never {
      self.members.filter((member: Provider) -> bool { member.is_effectful() }).length() > 0
    }
  }

  implements Generate {
    function generate<T>(self, request: Request<T>) -> Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      for (let member in self.members) {
        if (member.is_effectful()) {
          throw baml.errors.Unsupported {
            message: "fallback refuses effectful members without a durable replay contract",
          };
        }
        match (member) {
          let generate: Generate => {
            let response = generate.generate<T>(request.for_provider(member)) catch (e) {
              _ => {
                let replayable = thrown_error_is_replayable(e);
                if (replayable) { continue; }
                throw e;
              },
            };
            return response;
          },
          _ => {},
        }
      }
      throw baml.errors.UnknownError { data: null, message: ["fallback: all providers failed"] }
    }
  }

  implements Streaming {
    function stream<TPartial, T>(self, request: Request<T>) -> baml.llm.Stream<TPartial, T>
        throws baml.errors.StreamError | baml.errors.UnknownError {
      for (let member in self.members) {
        match (member) {
          let streaming: Streaming => {
            let result = streaming.stream<TPartial, T>(request.for_provider(member)) catch (e) {
              _ => continue,
            };
            return result;
          },
          _ => {},
        }
      }
      throw baml.errors.UnknownError { data: null, message: ["fallback: no streaming provider"] }
    }
  }
}
