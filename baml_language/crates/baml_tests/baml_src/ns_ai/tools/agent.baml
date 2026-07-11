class Agent {
  inner: Provider,
  tools: Tool[],
  dispatch: (ToolCall[]) -> ToolResult[] throws never,
  stop_when: ((StepInfo) -> bool throws never)?,

  implements Provider {
    function display_name(self) -> string throws never { "agent" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }

  implements Generate {
    function generate<T>(self, request: Request<T>) -> Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      let bound = Request<T> {
        provider: self.inner,
        prompt: request.for_provider(self.inner).prompt,
        identity: request.identity,
        arguments: request.arguments,
        tools: self.tools,
        options: RequestOptions { dispatch: self.dispatch },
        tags: request.tags,
        _render: request._render,
        _messages: request._messages,
      };
      let budget = Budget { max_steps: null, stop_when: self.stop_when };
      let outcome = run_agent<T>(bound, budget = budget) catch (e) {
        let unknown: baml.errors.UnknownError => throw unknown,
        _ => throw baml.errors.UnknownError { data: e, message: ["agent loop failed"] },
      };
      match (outcome) {
        let done: Done<T> => Response<T> { value: done.value, meta: done.meta },
        let stopped: BudgetReached => throw baml.errors.UnknownError {
          data: stopped, message: ["agent budget reached before a final value"],
        },
        let handoff: Handoff => throw baml.errors.UnknownError {
          data: handoff, message: ["agent handed off before a final value"],
        },
      }
    }
  }
}
