# Using `OpenAiStrict` from BAML

`OpenAiStrict` is the `root.ai` reference provider for getting a value of the
declared BAML return type through OpenAI's strict schemas. It is implemented in
BAML in this directory so its provider-specific behavior is visible and
replaceable.

> This is BEPv2 reference code under `baml_tests`, not the old `baml.ai`
> implementation. Examples therefore use `root.ai` and manually expanded task
> helpers until compiler desugaring is enabled.

Read this page first, then:

- [Structured return types](./structured-outputs.md) explains primitives,
  enums, classes, unions, arrays, recursion, and the actual wire schema.
- [Tools and agents](./tools-and-agents.md) explains local tools, provider
  tools, tool loops, `-> null`, parallel calls, and finalization.

## The two jobs called "strict"

There are two related but different constraints:

1. **Constrain the final BAML value.** The function's return type `T` becomes a
   strict schema. A normal `Generate` call must produce exactly one `T`.
2. **Constrain application-tool arguments.** Every local tool's argument class
   becomes a strict object schema before your dispatcher receives a call.

Do not confuse a final structured value with a tool action. For example,
`(Approved | Rejected)[]` is one ordered return value, not several parallel
tool calls.

## Provider setup

```baml
let provider: root.ai.Provider = root.ai.OpenAiStrict {
  model: "gpt-5.6-luna",
  api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
  base_url: null,
  extra_headers: null,
  extra_body: null,
}
```

`model` is explicit. `base_url`, headers, and body overrides are escape hatches
for a compatible endpoint or deployment-specific request data.

## A normal typed result

The intended source task is ordinary BAML:

```baml
enum Risk { Low Medium High }

class Incident {
  title: string,
  risk: Risk,
}

function ClassifyIncident(text: string) -> Incident {
  client: StrictOpenAI
  prompt: `Classify ${text}. ${ctx.output_format}`
}
```

Until task desugaring is implemented, the scenario fixtures spell the request
and execution explicitly:

```baml
function ClassifyIncident_request(
  text: string,
  provider: root.ai.Provider,
) -> root.ai.Request<Incident> {
  root.ai.task_request<Incident>(
    provider,
    "ClassifyIncident",
    "my_package",
    { "text": text },
    [],
    root.ai.RequestOptions {},
    {},
    (ctx: baml.llm.Context) -> baml.llm.PromptAst {
      baml.llm.from_shorthand(`Classify ${text}. ${ctx.output_format}`)
        catch_all (e) { _ => baml.sys.panic("invalid prompt") }
    },
  )
}

function ClassifyIncident_manual(
  text: string,
  provider: root.ai.Provider,
) -> Incident {
  root.ai.run<Incident>(ClassifyIncident_request(text, provider))
}
```

The generated task syntax should lower to this shape; users should not
eventually need to write the `_request` or `_manual` functions.

## What happens on the wire

For a plain `Generate<T>` call, `OpenAiStrict`:

1. derives standard JSON Schema from `T`;
2. applies OpenAI's strict-schema restrictions;
3. wraps `T` under the required property `value`;
4. sends one synthetic function tool named `__baml_return_output`;
5. forces that tool and sets `parallel_tool_calls: false`;
6. decodes `arguments.value` as `T` using strict JSON decoding.

The wrapper is what permits root primitives, enums, arrays, and unions even
though an OpenAI function's `parameters` schema must have an object root.

```text
BAML T
  -> standard JSON Schema
  -> OpenAI strict-schema adapter
  -> { value: T-schema }
  -> one forced __baml_return_output call
  -> strict JSON decode of value as T
```

## Choosing a provider surface

| Need | Use |
| --- | --- |
| One typed value, no local tools | `OpenAiStrict` through `Generate` |
| Local functions your process executes | `OpenAiStrict` through the `Tools` loop |
| Dynamic local roster | `root.ai.Agent { inner, tools, dispatch, ... }` |
| OpenAI-hosted web/file/code tools | A Responses provider capability that explicitly models them |
| Image generation | An image-output provider, not `OpenAiStrict` |

`OpenAiStrict` currently uses Chat Completions wire formats. It is not a
synonym for every OpenAI endpoint or every server-hosted tool.

## Observing requests

Provider requests and responses are emitted through `log.info`. Logs are off
by default:

```sh
infisical run --env=test -- \
  target/debug/baml-cli test \
  --from crates/baml_tests/baml_src \
  -i 'integ-test*::*OpenAI strict*' \
  --logs INFO > /tmp/baml-openai-strict.log 2>&1
```

The raw log lets you verify the generated schema, forced tool choice, parallel
policy, response arguments, and model refusal/error payloads.

## Current boundaries of the reference implementation

- Strict output and local tool calls are implemented on Chat Completions.
- Server-hosted Responses tools are a separate provider concern.
- `OpenAiStrict` does not generate images.
- Strict map-like `map<string, T>` schemas are rejected instead of silently
  weakening them to an unconstrained object.
- Plain strict generation recognizes OpenAI's `refusal` field. The current
  multi-turn tool-loop wire model still needs a normalized refusal outcome.
- Tool choice is currently `auto` in a local tool loop. A future request policy
  should expose `auto`, `required`, and a specific tool without conflating that
  choice with the BAML return type.

The OpenAI behavior summarized here corresponds to the official
[function-calling guide](https://developers.openai.com/api/docs/guides/function-calling)
and [Structured Outputs guide](https://developers.openai.com/api/docs/guides/structured-outputs).
