// Tool-loop fallback for providers that can generate typed values but do not
// expose a native tool-calling wire protocol. Each provider turn asks for
// either the final T or ToolCalls and SAP parses that union.
class PromptToolProvider {
  inner: Provider,
}

implements Provider for PromptToolProvider {
  function name(self) -> string throws never {
    "prompt-tools(" + self.inner.name() + ")"
  }
  function render_shorthand(self) -> string throws never {
    self.inner.render_shorthand()
  }
}

class PromptToolTranscript {
  owner: PromptToolProvider,
  history: Conversation,
  recipe: PromptRenderRecipe,
  identity: TaskIdentity?,
  arguments: map<string, unknown>,
  options: TaskOptions,
  tags: map<string, string>,

  implements Transcript {
    function provider(self) -> Provider throws never { self.owner }
    function messages(self) -> Messages throws never { self.history }
    function conversation(self) -> Conversation throws never { self.history }
  }
}

function prompt_tool_schema(tool: Tool) -> json throws baml.errors.UnknownError {
  let schema = if (tool.input_schema != null) {
    tool.input_schema
  } else {
    baml.schema.json_schema(tool.parameters) catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["prompt tool schema generation failed for " + tool.name],
      },
    }
  };
  {
    "name": tool.name,
    "description": tool.description,
    "parameters": schema,
  }
}

function prompt_tool_output_suffix(tools: Tool[]) -> string
    throws baml.errors.UnknownError {
  let schemas = tools.map((tool: Tool) -> json { prompt_tool_schema(tool) });
  "\n\n" + `The output contract above permits either the final result or a ToolCalls value.
To use tools, return ToolCalls with one or more calls. Each call must have a
unique id, an exact tool name from the catalog, and args matching that tool's
parameters JSON Schema. Do not invent tool names. After tool results are
provided, choose tools again or return the final result.

Available tools:
${baml.json.stringify(schemas)}`
}

function prompt_tool_message_text(message: Message) -> string throws never {
  message.parts().map((part: MessagePart) -> string { part.text() ?? "" }).join("")
}

function prompt_tool_role(role: MessageRole) -> string throws never {
  match (role) {
    MessageRole.System => "system",
    MessageRole.User => "user",
    MessageRole.Assistant => "assistant",
    MessageRole.Tool => "tool",
  }
}

function prompt_tool_history(history: Conversation) -> string throws never {
  if (history.entries.length() <= 1) { return ""; }
  history.entries.slice(1, history.entries.length()).map(
    (message: Message) -> string {
      prompt_tool_role(message.role()) + ": " + prompt_tool_message_text(message)
    },
  ).join("\n")
}

function prompt_tool_turn<T>(
  provider: PromptToolProvider,
  state: PromptToolTranscript,
  tools: Tool[],
  output_suffix: string,
) -> Task<T> throws never {
  let rendered = state.recipe.render_with_output_format(
    provider.inner,
    reflect.type_of<T>(),
    output_suffix,
  );
  let prior = prompt_tool_history(state.history);
  let prompt = if (prior.length() == 0) {
    rendered
  } else {
    baml.llm.assemble_prompt_ast(
      ["", "\n\nConversation so far:\n", ""],
      [baml.llm.prompt_to_text(rendered), prior],
    )
  };
  let recipe = PromptRenderRecipe {
    template: (ctx: baml.llm.Context) -> baml.llm.PromptAst { prompt },
  };
  Task<T> {
    provider: provider.inner,
    prompt: prompt,
    identity: state.identity,
    arguments: state.arguments,
    tools: tools,
    options: state.options,
    tags: state.tags,
    transcript: null,
    _render: recipe,
  }
}

implements ToolCallingProvider for PromptToolProvider {
  function begin<T>(self, task: Task<T>) -> Transcript
      throws never {
    PromptToolTranscript {
      owner: self,
      history: task.messages().to_conversation(),
      recipe: task._render,
      identity: task.identity,
      arguments: task.arguments,
      options: task.options,
      tags: task.tags,
    }
  }

  function step<T>(self, transcript: Transcript, tools: Tool[]) -> ModelStep<T>
      throws baml.errors.ToolError | baml.errors.UnknownError {
    let state = match (transcript) {
      let value: PromptToolTranscript => value,
      _ => throw baml.errors.ToolError {
        message: "prompt tool adapter received another provider's transcript",
      },
    };
    let generation = match (self.inner) {
      let value: GenerationProvider => value,
      _ => throw baml.errors.UnknownError {
        data: null,
        message: ["prompt tool adapter requires GenerationProvider"],
      },
    };
    if (tools.length() == 0) {
      let response = generation.generate<T>(prompt_tool_turn<T>(self, state, [], "")) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["prompt tool generation failed"],
        },
      };
      return ModelStep<T> { outcome: response.value, meta: response.meta };
    }
    let suffix = prompt_tool_output_suffix(tools);
    let response = generation.generate<T | ToolCalls>(
      prompt_tool_turn<T | ToolCalls>(self, state, tools, suffix),
    ) catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["prompt tool generation failed"],
      },
    };
    match (response.value) {
      let calls: ToolCalls => {
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

  function submit(self, transcript: Transcript, results: ToolResult[]) -> Transcript
      throws baml.errors.ToolError {
    let state = match (transcript) {
      let value: PromptToolTranscript => value,
      _ => throw baml.errors.ToolError {
        message: "prompt tool adapter received another provider's transcript",
      },
    };
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

function tool_calling_provider(provider: Provider) -> ToolCallingProvider
    throws baml.errors.Unsupported {
  match (provider) {
    let native: ToolCallingProvider => native,
    let generated: GenerationProvider => PromptToolProvider { inner: provider },
    _ => throw baml.errors.Unsupported {
      message: "provider cannot run a tool loop: " + provider.name(),
    },
  }
}
