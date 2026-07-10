// BEP-064 v2 reference implementation, intentionally local to baml_tests.
// Scenario code uses root.ai exclusively; compiler desugaring is deferred, so
// task_request is the hand-written companion seam.

interface Provider {
  function display_name(self) -> string throws never { "provider" }
  function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  function with_retry(self, max: int) -> Retry throws never {
    Retry { inner: self, max: max }
  }
  function fallback_to(self, other: Provider) -> Fallback throws never {
    Fallback { members: [self, other] }
  }
  function traced(self, meter: UsageMeter) -> Traced throws never {
    Traced { inner: self, meter: meter }
  }
}

class MessagePart {
  text: string?,
  image: image?,
  audio: audio?,
  pdf: pdf?,
  video: video?,

  function from_text(text: string) -> MessagePart throws never {
    MessagePart { text: text }
  }
}

class ChatMessage {
  role: string,
  parts: MessagePart[],

  function user(text: string) -> ChatMessage throws never {
    ChatMessage { role: "user", parts: [MessagePart.from_text(text)] }
  }
  function system(text: string) -> ChatMessage throws never {
    ChatMessage { role: "system", parts: [MessagePart.from_text(text)] }
  }
  function assistant(text: string) -> ChatMessage throws never {
    ChatMessage { role: "assistant", parts: [MessagePart.from_text(text)] }
  }
}

function prompt_to_messages(rendered_prompt: baml.llm.PromptAst) -> ChatMessage[] throws never {
  [ChatMessage.user(baml.llm.prompt_to_text(rendered_prompt))]
}

class Usage {
  input_tokens: int,
  output_tokens: int,
}

class Meta {
  provider: string,
  model: string?,
  request_id: string?,
  finish_reason: string?,
  usage: Usage?,
  attributes: map<string, unknown>,
  raw: json?,
}

class Response<T> {
  value: T,
  meta: Meta,
}

class TaskIdentity {
  name: string,
  package: string,
}

class RequestOptions {
  dispatch: ((ToolCall[]) -> ToolResult[] throws never)?,
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

interface Generate requires Provider {
  function generate<T>(self, request: Request<T>) -> Response<T>
    throws baml.errors.CallError | baml.errors.UnknownError
}

interface Streaming requires Provider {
  function stream<TPartial, T>(self, request: Request<T>) -> baml.llm.Stream<TPartial, T>
    throws baml.errors.StreamError | baml.errors.UnknownError
}

function run_with_meta<T>(request: Request<T>) -> Response<T>
    throws baml.errors.CallError | baml.errors.UnknownError {
  match (request.provider) {
    let generate: Generate => generate.generate<T>(request),
    _ => throw baml.errors.Unsupported {
      message: "provider cannot generate: " + request.provider_name(),
    },
  }
}

function run<T>(request: Request<T>) -> T
    throws baml.errors.CallError | baml.errors.UnknownError {
  run_with_meta<T>(request).value
}

function stream<TPartial, T>(request: Request<T>) -> baml.llm.Stream<TPartial, T>
    throws baml.errors.StreamError | baml.errors.UnknownError {
  match (request.provider) {
    let streaming: Streaming => streaming.stream<TPartial, T>(request),
    _ => throw baml.errors.Unsupported {
      message: "provider cannot stream: " + request.provider_name(),
    },
  }
}
