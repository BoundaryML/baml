// Tool-loop fallback for providers that can generate typed values but do not
// expose a native tool-calling wire protocol. Each provider turn asks for
// either the final T or ToolCalls and SAP parses that union.
class PromptToolProvider {
    // A prompt adapter is deliberately backed by the one-turn generation
    // capability, never CompletionProvider. This keeps an internal agent loop
    // from re-entering provider-owned completion.
    inner: GenerationProvider,
}

implements Provider for PromptToolProvider {
  function name(self) -> string throws never {
    "prompt-tools(" + self.inner.name() + ")"
  }
  function render_shorthand(self) -> string throws never {
    self.inner.render_shorthand()
  }
}

class PromptToolConversation {
    owner: PromptToolProvider,
    history: MessageHistory,
    recipe: PromptRenderRecipe,
    identity: TaskIdentity?,
    arguments: map<string, unknown>,
    tags: map<string, string>,

    implements Conversation {
        function provider(self) -> Provider throws never {
            self.owner
        }
        function messages(self) -> Messages throws never {
            self.history
        }
    }
}

function _prompt_tool_schema(tool: Tool) -> json throws never {
    { "name": tool.name, "description": tool.description, "parameters": tool.input_schema }
}

function _prompt_tool_output_suffix(tools: Tool[]) -> string throws never {
    let schemas = tools.map((tool: Tool) -> json {
        _prompt_tool_schema(tool)
    });
    "\n\n"
        + `
            The output contract above permits either the final result or a ToolCalls value.
            To use tools, return ToolCalls with one or more calls. Each call must have a
            unique id, an exact tool name from the catalog, and args matching that tool's
            parameters JSON Schema. Do not invent tool names. After tool results are
            provided, choose tools again or return the final result.

            Available tools:
            ${baml.json.stringify(schemas)}
        `
}

function _prompt_tool_message_text(message: Message) -> string throws never {
    message
        .parts()
        .map((part: MessagePart) -> string {
            part.text() ?? ""
        })
        .join("")
}

function _prompt_tool_role(role: MessageRole) -> string throws never {
    match (role) {
        MessageRole.System => "system",
        MessageRole.User => "user",
        MessageRole.Assistant => "assistant",
        MessageRole.Tool => "tool",
    }
}

function _prompt_tool_history(history: MessageHistory) -> string throws never {
    if (history.entries.length() <= 1) {
        return "";
    }
    history
        .entries
        .slice(1, history.entries.length())
        .map((message: Message) -> string {
            _prompt_tool_role(message.role()) + ": " + _prompt_tool_message_text(message)
        })
        .join("\n")
}

function _prompt_tool_turn<T>(
    provider: PromptToolProvider,
    state: PromptToolConversation,
    tools: Tool[],
    output_suffix: string,
) -> Task<T> throws never {
    //# Render the task with its output contract
    let rendered = state.recipe.render_with_output_format(provider.inner, reflect.type_of<T>(), output_suffix);
    //# Reattach prior tool-loop conversation
    let prior = _prompt_tool_history(state.history);
    let prompt = if (prior.length() == 0) {
        rendered
    } else {
        baml.llm
            .assemble_prompt_ast(
                ["", "\n\nConversation so far:\n", ""],
                [root.ai._prompt_text_content(rendered), prior],
            )
    };
    let recipe = PromptRenderRecipe {
        template: (ctx: baml.llm.Context) -> baml.llm.PromptAst {
            prompt
        },
    };
    Task<T> {
        provider: provider.inner,
        prompt: prompt,
        identity: state.identity,
        arguments: state.arguments,
        tools: tools,
        tags: state.tags,
        _render: recipe,
    }
}

implements ToolCallingProvider for PromptToolProvider {
  function begin<T>(self, task: Task<T>) -> Conversation
      throws never {
    PromptToolConversation {
      owner: self,
      history: task.messages().to_history(),
      recipe: task._render,
      identity: task.identity,
      arguments: task.arguments,
      tags: task.tags,
    }
  }

  function step<T>(self, conversation: Conversation, tools: Tool[]) -> ModelStep<T>
      throws root.ai.ToolError | baml.errors.UnknownError {
    //# Resume the prompt-tool conversation
    let state = match (conversation) {
      let value: PromptToolConversation => value,
      _ => throw root.ai.ToolError {
        message: "prompt tool adapter received another provider's conversation",
      },
    };
    //# Resolve the backing generation provider
    //# Use plain generation when no tools are active
    if (tools.length() == 0) {
      let response = self.inner.generate<T>(_prompt_tool_turn<T>(self, state, [], "")) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["prompt tool generation failed"],
        },
      };
      return ModelStep<T> { outcome: response.value, meta: response.meta };
    }
    //# Ask for tool calls or the final value
    let suffix = _prompt_tool_output_suffix(tools);
    let response = self.inner.generate<T | ToolCalls>(
      _prompt_tool_turn<T | ToolCalls>(self, state, tools, suffix),
    ) catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["prompt tool generation failed"],
      },
    };
    //# Preserve the chosen outcome
    match (response.value) {
      let calls: ToolCalls => {
        //## Append tool calls to prompt history
        state.history.entries.push(ChatMessage.assistant(baml.json.stringify({
          "calls": calls.calls.map((call: ToolCall) -> json {
            { "id": call.id, "name": call.name, "args": call.args }
          }),
        })));
        ModelStep<T> { outcome: calls, meta: response.meta }
      },
      let value: T => ModelStep<T> { outcome: value, meta: response.meta },
    }
  }

  function submit(self, conversation: Conversation, results: ToolResult[]) -> Conversation
      throws root.ai.ToolError {
    //# Resume the prompt conversation receiving results
    let state = match (conversation) {
      let value: PromptToolConversation => value,
      _ => throw root.ai.ToolError {
        message: "prompt tool adapter received another provider's conversation",
      },
    };
    //# Append tool results to prompt history
    for (let result in results) {
      state.history.entries.push(ChatMessage {
        message_role: MessageRole.Tool,
        content: [TextPart {
          value: baml.json.stringify({
            "id": result.id,
            "output": result.output,
            "is_error": result.is_error,
          }),
          metadata: {},
        }],
        metadata: null,
      });
    }
    state
  }
}

function _adapt_tool_calling_provider(
    provider: Provider,
) -> ToolCallingProvider throws baml.errors.Unsupported {
    //# Prefer native tools and adapt generation-only providers
    match (provider) {
        let native: ToolCallingProvider => native,
        let generated: GenerationProvider => PromptToolProvider { inner: generated },
        _ => {
            throw baml.errors.Unsupported {
      message: "provider cannot run a tool loop: " + provider.name(),
    }
        },
    }
}
