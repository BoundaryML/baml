class TaskIdentity {
  name: string,
  package: string,
}

class RequestOptions {
  dispatch: ((ToolCall[]) -> ToolResult[] throws never)?,
  tool_middleware: ToolMiddleware[]?,
  observers: AgentObserver[]?,
  recorders: AgentRecorder[]?,
  run_id: string?,
}

class PromptRenderRecipe {
  template: (baml.llm.Context) -> baml.llm.PromptAst throws never,

  function render(self, provider: Provider, output_type: type) -> baml.llm.PromptAst throws never {
    let primitive = baml.llm.from_shorthand(provider.render_shorthand()) catch (e) {
      _ => baml.sys.panic("root.ai: invalid provider render shorthand"),
    };
    self.template(baml.llm.build_prompt_context(primitive, output_type))
  }
}

class Request<T> {
  provider: Provider,
  prompt: baml.llm.PromptAst,
  identity: TaskIdentity?,
  arguments: map<string, unknown>,
  tools: Tool[],
  options: RequestOptions,
  tags: map<string, string>,
  _render: PromptRenderRecipe,
  /// Explicit structural view for hand-written requests that contain roles or
  /// media. Null means derive a one-message text view from PromptAst.
  _messages: ChatMessage[]?,

  function messages(self) -> ChatMessage[] throws never {
    self._messages ?? prompt_to_messages(self.prompt)
  }
  function output_type(self) -> type throws never {
    reflect.type_of<T>()
  }
  function provider_name(self) -> string throws never {
    self.provider.display_name()
  }
  function for_provider(self, provider: Provider) -> Request<T> throws never {
    Request<T> {
      provider: provider,
      prompt: self._render.render(provider, reflect.type_of<T>()),
      identity: self.identity,
      arguments: self.arguments,
      tools: self.tools,
      options: self.options,
      tags: self.tags,
      _render: self._render,
      _messages: self._messages,
    }
  }
  function with_tools(self, tools: Tool[]) -> Request<T> throws never {
    Request<T> {
      provider: self.provider,
      prompt: self.prompt,
      identity: self.identity,
      arguments: self.arguments,
      tools: tools,
      options: self.options,
      tags: self.tags,
      _render: self._render,
      _messages: self._messages,
    }
  }
  function with_options(self, options: RequestOptions) -> Request<T> throws never {
    Request<T> {
      provider: self.provider,
      prompt: self.prompt,
      identity: self.identity,
      arguments: self.arguments,
      tools: self.tools,
      options: options,
      tags: self.tags,
      _render: self._render,
      _messages: self._messages,
    }
  }
  function with_dispatch(
    self,
    dispatch: (ToolCall[]) -> ToolResult[] throws never,
  ) -> Request<T> throws never {
    self.with_options(RequestOptions {
      dispatch: dispatch,
      tool_middleware: self.options.tool_middleware,
      observers: self.options.observers,
      recorders: self.options.recorders,
      run_id: self.options.run_id,
    })
  }
  function with_tool_middleware(self, middleware: ToolMiddleware[]) -> Request<T> throws never {
    self.with_options(RequestOptions {
      dispatch: self.options.dispatch,
      tool_middleware: middleware,
      observers: self.options.observers,
      recorders: self.options.recorders,
      run_id: self.options.run_id,
    })
  }
  function with_observers(self, observers: AgentObserver[]) -> Request<T> throws never {
    self.with_options(RequestOptions {
      dispatch: self.options.dispatch,
      tool_middleware: self.options.tool_middleware,
      observers: observers,
      recorders: self.options.recorders,
      run_id: self.options.run_id,
    })
  }
  function with_recorders(self, recorders: AgentRecorder[]) -> Request<T> throws never {
    self.with_options(RequestOptions {
      dispatch: self.options.dispatch,
      tool_middleware: self.options.tool_middleware,
      observers: self.options.observers,
      recorders: recorders,
      run_id: self.options.run_id,
    })
  }
  function with_run_id(self, run_id: string) -> Request<T> throws never {
    self.with_options(RequestOptions {
      dispatch: self.options.dispatch,
      tool_middleware: self.options.tool_middleware,
      observers: self.options.observers,
      recorders: self.options.recorders,
      run_id: run_id,
    })
  }
}

// Shared lazy prompt recipe for the hand-written task companions. Compiler
// desugaring will eventually synthesize this recipe directly.
function prompt_template(
  text: string,
  with_output_format: bool,
) -> (baml.llm.Context) -> baml.llm.PromptAst throws never {
  (ctx: baml.llm.Context) -> baml.llm.PromptAst {
    if (with_output_format) {
      baml.llm.assemble_prompt_ast(["", "\n", ""], [text, ctx.output_format])
    } else {
      baml.llm.assemble_prompt_ast(["", ""], [text])
    }
  }
}

function request<T>(
  provider: Provider,
  template: (baml.llm.Context) -> baml.llm.PromptAst throws never,
) -> Request<T> throws never {
  task_request<T>(provider, "", "", {}, [], RequestOptions {}, {}, template)
}

function task_request<T>(
  provider: Provider,
  name: string,
  package: string,
  arguments: map<string, unknown>,
  tools: Tool[],
  options: RequestOptions,
  tags: map<string, string>,
  template: (baml.llm.Context) -> baml.llm.PromptAst throws never,
) -> Request<T> throws never {
  let recipe = PromptRenderRecipe { template: template };
  Request<T> {
    provider: provider,
    prompt: recipe.render(provider, reflect.type_of<T>()),
    identity: TaskIdentity { name: name, package: package },
    arguments: arguments,
    tools: tools,
    options: options,
    tags: tags,
    _render: recipe,
    _messages: null,
  }
}

/// Structural manual constructor for multimodal/role-sensitive scenarios.
/// The prompt recipe remains the execution source; `messages()` returns the
/// explicitly supplied role/media view without flattening.
function task_request_messages<T>(
  provider: Provider,
  name: string,
  package: string,
  arguments: map<string, unknown>,
  tools: Tool[],
  options: RequestOptions,
  tags: map<string, string>,
  messages: ChatMessage[],
  template: (baml.llm.Context) -> baml.llm.PromptAst throws never,
) -> Request<T> throws never {
  let built = task_request<T>(provider, name, package, arguments, tools, options, tags, template);
  Request<T> {
    provider: built.provider,
    prompt: built.prompt,
    identity: built.identity,
    arguments: built.arguments,
    tools: built.tools,
    options: built.options,
    tags: built.tags,
    _render: built._render,
    _messages: messages,
  }
}
