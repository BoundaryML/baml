class Fallback {
    first: Provider,
    rest: Provider[],

    function providers(self) -> Provider[] throws never {
        [self.first].concat(self.rest)
    }
}

class FallbackConversation {
    owner: Fallback,
    recipe: PromptRenderRecipe,
    identity: TaskIdentity?,
    arguments: map<string, unknown>,
    tools: ai.tools.Tool[],
    tags: map<string, string>,
    member_index: int,
    inner: Conversation,
    progressed: bool,

    implements Conversation {
        function provider(self) -> Provider throws never {
            self.owner
        }

        function messages(self) -> Messages throws never {
            self.inner.messages()
        }
        function output_type_fingerprint(self) -> string? throws never {
            self.inner.output_type_fingerprint()
        }
    }
}

implements Provider for Fallback {
  function name(self) -> string throws never { "fallback" }
  function render_shorthand(self) -> string throws never { self.first.render_shorthand() }
}


// Fallback may switch members only until the first provider turn succeeds.
// Once a model turn has produced tool calls or a value, changing providers is
// an explicit Agent conversation import rather than a blind replay.
implements AgentProvider for Fallback {
  function begin<T>(self, task: Task<T>) -> Conversation
      throws ai.Failure | baml.errors.UnknownError {
    let provider = ai.internal._fallback_member(self.first);
    FallbackConversation {
      owner: self,
      recipe: task._render,
      identity: task.identity,
      arguments: task.arguments,
      tools: task.tools,
      tags: task.tags,
      member_index: 0,
      inner: provider.begin<T>(task.with_provider(self.first)),
      progressed: false,
    }
  }

  function step<T>(
    self,
    conversation: Conversation,
    tools: ai.tools.Tool[],
  ) -> ModelStep<T> throws ai.Failure | baml.errors.UnknownError {
    let state = match (conversation) {
      let value: FallbackConversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(
        null,
        "fallback received another provider's conversation",
      ),
    };
    if (!ai.internal._same_provider_instance(state.owner, self)) {
      throw baml.errors.UnknownError.with_message<never>(
        null,
        "fallback received a conversation owned by another fallback instance",
      )
    }
    let members = self.providers();
    let failures: ai.Failure[] = [];
    //# Try another provider only before any model turn has succeeded
    while (state.member_index < members.length()) {
      let member = members[state.member_index];
      let provider = ai.internal._fallback_member(member);
      let result = provider.step<T>(state.inner, tools) catch (e) {
        let failure: ai.Failure => {
          if (
            state.progressed
              || !ai.internal._may_retry_model_step(failure, null)
              || state.member_index + 1 >= members.length()
          ) {
            throw e;
          }
          failures.push(failure);
          state.member_index = state.member_index + 1;
          let next = members[state.member_index];
          let next_provider = ai.internal._fallback_member(next);
          state.inner = next_provider.begin<T>(Task<T> {
            provider: next,
            prompt: state.recipe.render(next, reflect.type_of<T>()),
            identity: state.identity,
            arguments: state.arguments,
            tools: state.tools,
            tags: state.tags,
            _render: state.recipe,
          });
          continue;
        },
        _ => throw baml.errors.UnknownError.with_message<never>(
          e,
          "fallback(" + member.name() + ") step",
        ),
      };
      state.progressed = true;
      return result;
    }
    match (failures.at(failures.length() - 1)) {
      let failure: ai.Failure => throw failure,
      null => throw baml.errors.Unsupported {
        message: "fallback has no Agent-capable member",
      },
    }
  }

  function submit(
    self,
    conversation: Conversation,
    results: ai.tools.ToolResult[],
  ) -> Conversation throws ai.Failure | baml.errors.UnknownError {
    let state = match (conversation) {
      let value: FallbackConversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(
        null,
        "fallback received another provider's conversation",
      ),
    };
    if (!ai.internal._same_provider_instance(state.owner, self)) {
      throw baml.errors.UnknownError.with_message<never>(
        null,
        "fallback received a conversation owned by another fallback instance",
      )
    }
    let member = self.providers()[state.member_index];
    let provider = ai.internal._fallback_member(member);
    state.inner = provider.submit(state.inner, results);
    state
  }
}

implements ConversationAppendProvider for Fallback {
  function append_messages(
    self,
    conversation: Conversation,
    messages: Messages,
  ) -> null throws ai.Failure | baml.errors.UnknownError {
    let state = match (conversation) {
      let value: FallbackConversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(
        null,
        "fallback received another provider's conversation",
      ),
    };
    if (!ai.internal._same_provider_instance(state.owner, self)) {
      throw baml.errors.UnknownError.with_message<never>(
        null,
        "fallback received a conversation owned by another fallback instance",
      )
    }
    let member = self.providers()[state.member_index];
    let provider = match (member) {
      let capability: ConversationAppendProvider => capability,
      _ => throw baml.errors.Unsupported {
        message: "fallback member cannot append exact conversation messages: " + member.name(),
      },
    };
    let _ = provider.append_messages(state.inner, messages);
    // A fallback cannot reconstruct appended exact state on another member.
    // Pin the wrapper to this member after the first successful append.
    state.progressed = true;
    null
  }
}

function fallback(providers: Provider[]) -> Fallback throws baml.errors.InvalidArgument {
    match (providers.at(0)) {
        let first: Provider => {
            Fallback { first: first, rest: providers.slice(1, providers.length()) }
        },
        null => {
            throw baml.errors.InvalidArgument {
      message: "fallback requires at least one provider",
    }
        },
    }
}
