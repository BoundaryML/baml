// Compiler bridges and constructor plumbing. Nothing here is part of the
// public surface: the compiler emits calls to these by name, and the public
// `ai.task<T>` / `ai.fallback` entry points delegate here.

/// Compiler bridge for the `client:` spelling. The returned value is an
/// ordinary Provider and follows the same Task + Agent path as `provider:`.
function _provider_from_client(client: baml.llm.Client) -> ai.Provider throws never {
    ClientProvider { client: client }
}


/// Compiler entry point for constructing a named generated task.
function _task_named<T>(
    provider: ai.Provider,
    name: string,
    arguments: map<string, unknown>,
    tools: ai.tools.ToolInput[],
    template: (baml.llm.Context) -> baml.llm.PromptAst throws never,
) -> ai.Task<T> throws ai.InvalidRequest {
    //# Capture the reusable prompt recipe
    let recipe = _prompt_recipe(template);
    //# Render the task for its initial provider
    ai.Task<T> {
        provider: provider,
        prompt: recipe.render(provider, reflect.type_of<T>()),
        identity: ai.TaskIdentity { name: name, package: "user" },
        arguments: arguments,
        tools: ai.tools.normalize_tools(tools),
        tags: {},
        _render: recipe,
    }
}

/// Compiler entry point for a raw Jinja prompt registered under the generated
/// function name. It constructs the same Task<T> as a backtick prompt.
function _task_named_from_registered_prompt<T>(
    provider: ai.Provider,
    name: string,
    arguments: map<string, unknown>,
    tools: ai.tools.ToolInput[],
) -> ai.Task<T> throws ai.InvalidRequest {
    //# Capture the registered prompt as a reusable recipe
    let recipe = _registered_prompt_recipe(name, arguments);
    //# Render the task for its initial provider
    ai.Task<T> {
        provider: provider,
        prompt: recipe.render(provider, reflect.type_of<T>()),
        identity: ai.TaskIdentity { name: name, package: "user" },
        arguments: arguments,
        tools: ai.tools.normalize_tools(tools),
        tags: {},
        _render: recipe,
    }
}

function _prompt_recipe(
    template: (baml.llm.Context) -> baml.llm.PromptAst throws never,
) -> ai.PromptRenderRecipe throws never {
    ai.PromptRenderRecipe.of(template)
}

function _registered_prompt_recipe(
    function_name: string,
    arguments: map<string, unknown>,
) -> ai.PromptRenderRecipe throws never {
    ai.PromptRenderRecipe {
        template: null,
        registered_function: function_name,
        arguments: arguments,
    }
}

function _fallback_member(provider: ai.Provider) -> ai.AgentProvider throws baml.errors.Unsupported {
    match (provider) {
        let capability: ai.AgentProvider => capability,
        _ => {
            throw baml.errors.Unsupported {
                message: "fallback member cannot run an Agent: " + provider.name(),
            }
        },
    }
}
