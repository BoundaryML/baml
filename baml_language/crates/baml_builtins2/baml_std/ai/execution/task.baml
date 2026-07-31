class TaskIdentity {
    name: string,
    package: string,
}

/// A typed, inspectable model call recipe. Constructed automatically through
/// `F@task(...)` for declarative model functions.
class Task<T> {
    provider: Provider,
    prompt: baml.llm.PromptAst,
    identity: TaskIdentity?,
    arguments: map<string, unknown>,
    tools: ai.tools.Tool[],
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

    /// Runs this task to completion and returns the final typed value —
    /// direct-call semantics with configuration. Use it after
    /// `with_provider`/`with_tools`, or pass a configured Agent runner.
    /// Tasks are stateless recipes: each call is an independent fresh run,
    /// and a task can be completed any number of times.
    ///
    /// Throws `ai.IncompleteRun` if the run stops at a policy stop, handoff, or
    /// interruption instead of finishing. The throw is lossless — it
    /// carries the resumable outcome, so a catch site can continue via
    /// `ai.run.AgentSession.of(task, incomplete.outcome)`.
    function complete(
        self,
        runner: ai.run.Agent<T> = ai.run.Agent<T>.new(),
    ) -> T throws ai.IncompleteRun | ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
        ai.internal._done<T>(self.run(runner = runner)).value
    }

    function output_type(self) -> type throws never {
        reflect.type_of<T>()
    }

    /// The prompt-render recipe this task was declared with — what a
    /// provider adapter renders (with its own output-format conventions)
    /// to produce the opening request. Public so adapter authors never
    /// reach for the private field.
    function recipe(self) -> PromptRenderRecipe throws never {
        self._render
    }

    /// A canonical description of this task's executable contract:
    /// identity, output type, provider protocol, and the tool schemas the
    /// model can see. Two tasks with equal fingerprints are STRUCTURALLY
    /// COMPATIBLE — a session saved under one may be restored under the
    /// other. Handler implementations are not (and cannot be) part of the
    /// contract; that is the same reason tasks do not serialize.
    function contract_fingerprint(self) -> string throws never {
        let tools = self.tools.map((tool: ai.tools.Tool) -> string {
            `${tool.name}|handoff=${tool.handoff}|${baml.json.stringify(tool.input_schema)}`
        });
        tools.sort_by_key((entry: string) -> string {
            entry
        });
        let identity = match (self.identity) {
            let id: TaskIdentity => id.package + "." + id.name,
            null => "anonymous",
        };
        `baml-contract/v1;${identity};${ai.internal._output_type_fingerprint<T>()};${self.provider.name()};${tools.join(";")}`
    }

    function provider_name(self) -> string throws never {
        self.provider.name()
    }

    function messages(self) -> Messages throws never {
        ai.messages._prompt_to_messages(self.prompt)
    }

    function with_provider(self, provider: Provider) -> Task<T> throws ai.InvalidRequest {
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

    function with_tools(self, tools: ai.tools.ToolInput[]) -> Task<T> throws never {
        //# Replace the task's tool roster without executing it
        Task<T> {
            provider: self.provider,
            prompt: self.prompt,
            identity: self.identity,
            arguments: self.arguments,
            tools: ai.tools.normalize_tools(tools),
            tags: self.tags,
            _render: self._render,
        }
    }
}

function task<T>(
    provider: Provider,
    template: (baml.llm.Context) -> baml.llm.PromptAst throws never,
) -> Task<T> throws ai.InvalidRequest {
    ai.internal._task_named<T>(provider, "anonymous", {}, [], template)
}

/// The versioned nominal + structural fingerprint of an output type. For
/// PROVIDER AUTHORS: a conversation created by `begin<T>` should report
/// this value from `output_type_fingerprint()` so the Agent's invariant
/// checks can refuse a wrong-task continuation.
function output_fingerprint<T>() -> string throws never {
    ai.internal._output_type_fingerprint<T>()
}


