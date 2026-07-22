class TaskIdentity {
  name: string,
  package: string,
}

class PromptRenderRecipe {
  template: (baml.llm.Context) -> baml.llm.PromptAst throws never,

  function render(self, provider: Provider, output_type: type) -> baml.llm.PromptAst throws never {
    self.render_with_output_format(provider, output_type, "")
  }

  // Runners that add another model-visible result branch, such as the
  // prompt/SAP tool fallback, extend ctx.output_format here. The LLM function
  // still owns where that contract appears in its prompt.
  function render_with_output_format(
    self,
    provider: Provider,
    output_type: type,
    suffix: string,
  ) -> baml.llm.PromptAst throws never {
    let primitive = baml.llm.from_shorthand(provider.render_shorthand()) catch (e) {
      _ => baml.sys.panic("ai: invalid provider render shorthand"),
    };
    let base = baml.llm.build_prompt_context(primitive, output_type);
    self.template(baml.llm.Context {
      ...base,
      output_format: base.output_format + suffix,
    })
  }
}

// Executable compatibility spelling for the proposed Task<T, P>. See D-001
// and D-002 in _plan/bepv2/_internal/deviations.md.
class Task<T> {
  provider: Provider,
  prompt: baml.llm.PromptAst,
  identity: TaskIdentity?,
  arguments: map<string, unknown>,
  tools: Tool[],
  tags: map<string, string>,
  _render: PromptRenderRecipe,

  // The concrete runner determines both the return value and error channel
  // through associated types. The named argument makes the execution policy
  // explicit at every non-default call site.
  function run<Output, Error, R extends Runner<Task<T>, Output = Output, Error = Error>>(
    self,
    runner: R,
  ) -> Output throws Error {
    runner.run(self)
  }

  function output_type(self) -> type throws never {
    reflect.type_of<T>()
  }

  function provider_name(self) -> string throws never {
    self.provider.name()
  }

  function messages(self) -> Messages throws never {
    prompt_to_messages(self.prompt)
  }

  function with_provider(self, provider: Provider) -> Task<T> throws never {
    Task<T> {
      provider: provider,
      prompt: self._render.render(provider, reflect.type_of<T>()),
      identity: self.identity,
      arguments: self.arguments,
      tools: self.tools,
      tags: self.tags,
      _render: self._render,
    }
  }

  function with_tools(self, tools: Tool[]) -> Task<T> throws never {
    Task<T> {
      provider: self.provider,
      prompt: self.prompt,
      identity: self.identity,
      arguments: self.arguments,
      tools: tools,
      tags: self.tags,
      _render: self._render,
    }
  }
}

function task<T>(
  provider: Provider,
  template: (baml.llm.Context) -> baml.llm.PromptAst throws never,
) -> Task<T> throws never {
  task_named<T>(provider, "anonymous", {}, [], template)
}

function task_named<T>(
  provider: Provider,
  name: string,
  arguments: map<string, unknown>,
  tools: Tool[],
  template: (baml.llm.Context) -> baml.llm.PromptAst throws never,
) -> Task<T> throws never {
  let recipe = PromptRenderRecipe { template: template };
  Task<T> {
    provider: provider,
    prompt: recipe.render(provider, reflect.type_of<T>()),
    identity: TaskIdentity { name: name, package: "bepv2_reference" },
    arguments: arguments,
    tools: tools,
    tags: {},
    _render: recipe,
  }
}
