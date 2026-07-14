# 11. Tool Calling After BEP-062

This page describes the BEPv2 tool API after BEP-062, the Function interface,
lands. BEPv2 depends on the BEP-062 semantics described here: the compiler-
derived `Function` interface, tuples, named tuples, spread/rest, and
`reflect.call`.

BEP-062 gives every function value a compiler-derived `Function` interface,
including its parameter, return, and error types. It also adds tuple and named
tuple types plus `reflect.call`. Together, those features let the standard
library turn an ordinary typed function into an executable tool without a
hand-written dispatcher.

## What changes

Before BEP-062, the executable reference must keep these pieces separate:

```text
Tool { name, description, input type/schema }
ToolCall { id, name, args }
application dispatch function: ToolCall[] -> ToolResult[]
```

The dispatch function parses JSON, finds the handler by name, calls it, and
serializes the result. The type checker cannot connect those steps once the
handlers are stored in a heterogeneous list.

After BEP-062, `ai.tool` captures the function value that performs those steps:

```baml
class WeatherArgs {
  city: string,
  days: int,
}

function get_weather(args: WeatherArgs) -> Forecast
    throws WeatherUnavailable {
  weather_service.lookup(args.city, args.days)
}

let weather = ai.tool(
  "get_weather",
  "Get a forecast for a city.",
  get_weather,
)
```

The task and agent APIs still use `Tool[]`. A tool registry is heterogeneous,
so the driver needs one erased runtime interface. The difference is that each
application tool now owns its validated handler rather than depending on one
global name switch.

## Safe V1 function shape

The first version accepts a function with exactly one required class argument:

```baml
function ai.tool<A, R, E, F>(
  name: string,
  description: string,
  handler: F,
) -> Tool
where F: Function<
  Params = (A,),
  Variadic = never,
  OptionalParams = {},
  Returns = R,
  Throws = E,
>, A: class
```

This restriction is deliberate. Provider tool protocols describe arguments as
a JSON object with named properties. A single class maps to that contract
without guessing names for positional tuple entries or mixing positional,
optional, and variadic parameters.

Later versions may support named tuples directly. They should not expose every
possible BAML function signature until there is one unambiguous mapping from
that signature to a provider-neutral tool schema.

## Runtime tool contract

Conceptually, `ai.tool` returns an implementation of this erased interface:

```baml
interface Tool {
  function name(self) -> string throws never
  function description(self) -> string throws never

  // Standard, provider-neutral JSON Schema.
  function input_schema(self) -> json throws never

  // Validates args, invokes the captured function, and serializes its result.
  function invoke(self, call: ToolCall) -> ToolResult throws never
}
```

The concrete function-backed implementation retains `F`, `A`, `R`, and `E`.
Its erased `invoke` method performs the following lowering:

```text
1. Confirm the call targets this tool.
2. Parse call.args as A.
3. reflect.call(handler, (parsed_args,), {})
4. Serialize Returns to json.
5. Return ToolResult { id: call.id, output, is_error: false }.
```

Argument parse failures and declared `Throws` values become error tool results
by the tool's configured error policy. A panic or runtime corruption remains a
failed agent run unless an explicit boundary catches it. Every result retains
the provider's original call ID.

`input_schema()` starts with standard JSON Schema produced from `A`. Provider
adapters then transform that schema to their own supported dialect. OpenAI
strict mode, Anthropic structured tools, or another vendor's limitations do
not leak into the function or task declaration.

## Capturing application-only dependencies

Secrets, database handles, authorization context, and other host-only inputs
must not appear in the model-visible argument class. Bind them before creating
the tool:

```baml
function lookup_order_impl(args: LookupOrderArgs, auth: AuthContext) -> Order {
  orders.for_tenant(auth.tenant_id, args.order_id)
}

let lookup_order = ai.tool(
  "lookup_order",
  "Look up one order.",
  (args: LookupOrderArgs) -> Order {
    lookup_order_impl(args, current_auth)
  },
)
```

BEP-062 method values also work because they are closures that capture `self`:

```baml
class OrderTools {
  auth: AuthContext,

  function lookup(self, args: LookupOrderArgs) -> Order {
    orders.for_tenant(self.auth.tenant_id, args.order_id)
  }
}

let scoped = OrderTools { auth: current_auth }
let lookup_order = ai.tool("lookup_order", "Look up one order.", scoped.lookup)
```

The model sees only `LookupOrderArgs` in both cases.

## Three tool owners remain distinct

| Tool owner | Schema source | Executor | Function value required? |
| --- | --- | --- | --- |
| Application function | `Function.Params` / argument class | BEPv2 driver | Yes |
| MCP or runtime registry | Runtime JSON Schema | MCP/runtime adapter | No |
| Provider-owned tool | Typed provider configuration | Provider | No |

Function-backed tools do not replace MCP tools. An MCP server discovers names
and schemas at runtime and supplies a JSON-to-JSON handler. The standard
`Tool` interface can erase both implementations into the same active roster.

Provider-owned web search, code execution, and retrieval are different again.
They are configured on the provider and executed by it. The application agent
driver observes their events but does not dispatch them as application
functions.

## Relationship to BEP-059 function tools

BEP-059 proposes a generated `f$tool` argument class with an `execute` method.
That remains useful as an optional typed representation of a proposed call or
as a structured-output fallback for a provider without native tools.

It is no longer the canonical application dispatch mechanism after BEP-062.
The function value already carries its full signature, and `reflect.call`
provides validated invocation. Generating a second executable class for every
function would duplicate that information and require extra name-based
dispatch.

In short:

```text
Function value + ai.tool(...) = canonical executable application tool
f$tool value                  = optional typed call data / fallback encoding
```

## Tool selection and final structured output

The LLM function's return type and its application tools are independent:

- `T`, including a class, enum, primitive, union, recursive class, or list,
  describes the final typed result.
- `Tool[]` describes actions the model may request before producing that
  result.
- A provider adapter may use native tool calling for actions and a separate
  structured-output mechanism for the final `T`.

A return type such as `(A | B)[]` does not mean “parallel tool calls.” Parallel
tool use occurs only when the provider returns multiple `ToolCall` values in
one step. The agent may execute independent application calls concurrently,
then submits one result for every call ID.

## Hooks, dynamic tools, and switching providers

BEP-062 changes tool construction and invocation, not agent-loop ownership:

- `before_tool_call` may allow, rewrite, or block a proposed call;
- rewritten calls preserve their ID and are validated against the selected
  function-backed tool before invocation;
- blocked calls produce an error result for that ID;
- `after_tool_call` runs only after an actual invocation;
- a `ToolRegistry` may add function-backed or MCP tools between steps; and
- provider switching re-encodes the same standard schemas for the new
  provider's dialect.

The registry rejects duplicate names. Dispatch first resolves the name to one
tool, then calls that tool's erased `invoke`; results correlate by ID, never
array position.

## LLM functions used as tools

An LLM function also implements BEP-062's `Function` interface. Turning it into
a tool is nevertheless explicit:

```baml
let delegate = ai.tool(
  "draft_reply",
  "Ask the drafting model for a proposed reply.",
  DraftReply,
)
```

This tool performs a nested model call, which affects cost, latency, tracing,
and recursion risk. `ai.tool(...)` is the reviewable boundary where the user
chooses that behavior. The compiler-only `DraftReply.task(...)` selector does
not become a member of the ordinary function value.

## Normative rules after BEP-062

1. An application function tool MUST retain its concrete `Function` signature
   until it is wrapped behind the erased `Tool` interface.
2. V1 MUST accept only signatures that map unambiguously to one provider tool
   argument object.
3. Tool input schemas MUST begin as standard provider-neutral JSON Schema.
4. Provider adapters, not application handlers, MUST apply vendor schema
   restrictions and wire encodings.
5. Host-only arguments MUST be captured or bound before tool construction and
   MUST NOT appear in the model-visible schema.
6. Invocation MUST validate arguments before calling the function and MUST
   preserve the original tool-call ID.
7. Dynamic MCP tools and provider-owned tools MUST remain supported without a
   BAML function value.
8. An LLM function MUST require explicit `ai.tool(...)` wrapping before it is
   exposed as a nested model-call tool.

Until BEP-062 lands, the executable reference keeps `Tool.parameters: type`,
standard JSON Schema, and an explicit dispatcher. That is a compatibility
implementation of the same ownership model, not the final ergonomic API.
