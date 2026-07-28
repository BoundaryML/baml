class TaskIdentity {
    name: string,
    package: string,
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
        //# Dispatch this typed task to the selected execution policy
        runner.run(self)
    }

    function output_type(self) -> type throws never {
        reflect.type_of<T>()
    }

    function provider_name(self) -> string throws never {
        self.provider.name()
    }

    function messages(self) -> Messages throws never {
        _prompt_to_messages(self.prompt)
    }

    function with_provider(self, provider: Provider) -> Task<T> throws never {
        //# Re-render the same task contract for a different provider
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

    function with_tools(self, tools: ToolInput[]) -> Task<T> throws never {
        //# Replace the task's tool roster without executing it
        Task<T> {
            provider: self.provider,
            prompt: self.prompt,
            identity: self.identity,
            arguments: self.arguments,
            tools: normalize_tools(tools),
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
    tools: ToolInput[],
    template: (baml.llm.Context) -> baml.llm.PromptAst throws never,
) -> Task<T> throws never {
    //# Capture the reusable prompt recipe
    let recipe = PromptRenderRecipe { template: template };
    //# Render the task for its initial provider
    Task<T> {
        provider: provider,
        prompt: recipe.render(provider, reflect.type_of<T>()),
        identity: TaskIdentity { name: name, package: "bepv2_reference" },
        arguments: arguments,
        tools: normalize_tools(tools),
        tags: {},
        _render: recipe,
    }
}
