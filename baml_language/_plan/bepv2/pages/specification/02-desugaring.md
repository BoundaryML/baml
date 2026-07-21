# 2. What an LLM Function Desugars To

The compiler generates one public companion for an LLM function:
`.task(...)`. It creates a task value and performs no I/O. Normal calls and
drivers both use that same task representation.

## What to remember

- `MyFunction(args)` runs the provider's default drive behavior.
- `MyFunction.task(args)` only creates a `Task<T>`.
- Streaming, agents, and resources are library drivers, not extra generated
  function companions.

## Source declaration

```baml
function ExtractInvoice(document: pdf) -> Invoice {
  provider: AccurateModel
  tools: [lookup_vendor]
  prompt: `
    ${role("system")} You extract invoices precisely.
    ${role("user")} Extract this invoice: ${document}
    ${ctx.output_format}
  `
}
```

`client:` remains accepted as compatibility syntax, but canonical BEPv2 source
uses `provider:`. The provider and tool roster are defaults, not an execution
policy.

## The sole generated companion

### `.task` is a declaration selector, not a function-value member

In BAML source, `MyFunction.task(...)` is resolved specially when
`MyFunction` is a path that resolves directly to an LLM-function declaration.
The compiler can therefore typecheck the declaration's arguments, return type,
prompt recipe, default provider, and synthetic `$provider` parameter together.
An implementation may lower this selector to a hidden companion item such as
`MyFunction$task`; it is not a runtime property stored on the function value.

```baml
// Valid: ExtractInvoice resolves directly to the LLM-function declaration.
let task = ExtractInvoice.task(scan)

// Also valid: an ordinary function value remains directly callable.
let call = ExtractInvoice
let invoice = call(scan)

// Invalid: the callable value's type has no generated `task` member.
call.task(scan)
```

The final line is a compile error even when the local was initialized directly
from `ExtractInvoice`. Member lookup follows the static type of `call`; it does
not use constant propagation or declaration-provenance tracking to recover
compiler selectors. Passing an LLM function through a parameter, collection,
or returned function value has the same result.

V1 guarantees `.task(...)` in call position on an LLM-function declaration
path. It does not require `MyFunction.task` itself to be a first-class function
value. Code that needs a first-class task factory can wrap the declaration-path
call in an ordinary typed function or closure. Supporting a distinct
first-class `LlmFunction<Args, T>` value that retains task-construction metadata
would be a separate language feature, not an accidental consequence of
ordinary function values.

The public companion has a compiler-defined omitted-argument rule:

```text
ExtractInvoice.task(document)
  == ExtractInvoice.task(document, $provider = AccurateModel)
```

The compiler typechecks the injected or explicit provider expression first,
infers its static type as `P`, and then lowers to the equivalent required-
provider helper below:

```baml
function ExtractInvoice.task<P extends ai.Provider>(
  document: pdf,
  $provider: P,
) -> ai.Task<Invoice, P> {
  ai.Task<Invoice, P> {
    $provider: $provider,
    prompt: <lazy prompt recipe rendered for $provider and Invoice>,
    identity: ai.TaskIdentity {
      name: "ExtractInvoice",
      package: <current package>,
    },
    arguments: { "document": document },
    tools: [lookup_vendor],
    options: ai.TaskOptions {},
    tags: {},
    transcript: null,
    _render: <private render recipe>,
  }
}
```

This is compiler lowering notation, not a claim that ordinary BAML supports
overloads or a `typeof(value)` type operator. Calling `.task(...)` without an
override yields `Task<Invoice, P_default>`, where `P_default` is the static type
already inferred for `AccurateModel`. Supplying `$provider = CheapModel` yields
`Task<Invoice, P_cheap>` from the override expression's static type.

`reflect.type_of<Invoice>()` is used by the task's `output_type()` implementation
and by schema generation. It produces a runtime `type` value for an already
known type argument; it does not recover the static type of `AccurateModel`.

`$provider` is a compiler-reserved named parameter and task field. The sigil
makes the execution override visually distinct from the function's domain
arguments and prevents a user parameter from colliding with it:

```baml
let task = ExtractInvoice.task(scan, $provider = CheapModel)
```

No `.stream`, `.agent`, `.background`, `.with_meta`, `.prompt`, or `.parse`
execution companions are generated. Inspection and parsing are library
operations on `Task<T>` or its identity/parser recipe.

## Compiler-injected `$...` parameters

LLM functions reserve parameter names beginning with `$` for compiler-injected
execution controls. The parameters are accepted by both the directly callable
LLM function and `.task(...)`:

```baml
ExtractInvoice(scan, $provider = CheapModel)       // executes and returns Invoice
ExtractInvoice.task(scan, $provider = CheapModel)  // returns Task<Invoice, CheapModel>
```

They are not part of the user-declared domain signature, captured
`arguments`, prompt variables, or model-visible schema. This BEP initially
specifies `$provider`; ordinary user parameters may not begin with `$`.

## Plain calls use the provider's default drive

The original LLM function remains callable and accepts `$provider`. After the
compiler injects the default when omitted, its conceptual required-provider
helper is:

```baml
function ExtractInvoice<P extends DriveProvider>(
  document: pdf,
  $provider: P,
) -> Invoice
```

The direct form requires `P: DriveProvider`; `.task(...)` only requires `P: Provider`
because an explicit lifecycle driver may need a provider that is not directly
drivable, such as a realtime-only provider.

Calls lower directly to the standard drive function:

```baml
ExtractInvoice(scan)

// lowers to
ai.drivers.drive(ExtractInvoice.task(scan))
```

An explicit provider override lowers identically:

```baml
ExtractInvoice(scan, $provider = CheapModel)

// lowers to
ai.drivers.drive(
  ExtractInvoice.task(scan, $provider = CheapModel),
)
```

This is the only privileged driver choice. `drivers.drive` invokes the
selected provider's `DriveProvider` capability. It does not impose “one generation”
as universal behavior. A normal provider may drive through one `generate`
call; an agent provider may drive a complete tool loop. In every case the
direct LLM function returns its declared `T` or throws. Callers wanting
budget/handoff outcomes call `drivers.run_agent` explicitly.

Conceptually, direct-call lowering can also be read as:

```baml
let task = ExtractInvoice.task(scan, $provider = selected)
task.$provider.drive<Invoice>(task).value
```

## `Task<T>` is the universal invocation value

```baml
class Task<T, P extends Provider = Provider> {
  $provider: P,
  prompt: baml.llm.PromptAst,
  identity: TaskIdentity?,
  arguments: map<string, unknown>,
  tools: Tool[],
  options: TaskOptions,
  tags: map<string, string>,
  transcript: Transcript?,
  _render: PromptRenderRecipe,

  function messages(self) -> Messages throws never
  function output_type(self) -> type throws never
  function with_provider<Q extends Provider>(self, provider: Q) -> Task<T, Q> throws never
  function with_tools(self, tools: Tool[]) -> Task<T, P> throws never
  function with_transcript(self, transcript: Transcript) -> Task<T, P> throws never
}
```

`P` preserves the concrete provider type and therefore its capability
evidence. `Task<T>` is shorthand for `Task<T, Provider>` after intentional
existential erasure. `Task<T, P>` means one typed model invocation that has not
run. It carries no provider wire body and is process-local because its provider
and private render recipe are values.

`with_provider` re-renders the prompt from `_render`; it never merely swaps
the field. This preserves provider-sensitive prompt context during fallback
and during an intentional mid-loop provider change.

Manual construction remains possible:

```baml
let task = ai.task<Invoice>(Fast, prompt`
  Extract ${document}.
  ${ctx.output_format}
`)
```

## Streaming projection

Streaming needs the compiler-derived partial form of `T`. This does not
justify a generated execution function. The compiler may project a task at
the driver boundary:

```baml
type StreamTask<T, TPartial, P extends Provider = Provider>
  = <compiler-known view of Task<T, P>>

ai.drivers.stream(ExtractInvoice.task(scan))
// T = Invoice, TPartial = baml.macros.stream_type!(Invoice)
```

Conceptually the accepted type is
`StreamTask<T, baml.macros.stream_type!(T), P>`; the projection is PPIR/type
information, not another network-capable companion.

## Direct provider escape hatch

A custom driver can call a statically known capability directly:

```baml
function custom_run<T, P extends ai.GenerationProvider>(task: ai.Task<T, P>) -> T {
  task.$provider.generate<T>(task).value
}
```

Or a hand-written function can build a task and invoke a concrete provider:

```baml
function Blah() -> string {
  let task = ai.task<string>(MyDefault, prompt`Say hello.`)
  MyDefault.generate<string>(task).value
}
```

This is intentionally possible, but normal application code should prefer an
LLM function plus a standard driver so it retains task identity, generated
SDK typing, tracing, and shared safety policy. There should be no incentive
to write a raw provider `fetch` call.

## End-to-end trace

```text
ExtractInvoice(scan, $provider = Cheap)
  -> ExtractInvoice.task(scan, $provider = Cheap)       compiler-generated
       -> Task<Invoice, P> { $provider, ... }            P inferred from Cheap; no I/O
  -> ai.drivers.drive(task)                        stdlib delegation
       -> capability check for DriveProvider
       -> Cheap.drive<Invoice>(task)                    provider default policy
          -> one generation call, agent loop, or other typed completion
       -> Response<Invoice> { value, meta }
       -> value
```

Only `.task` construction and direct-call lowering are compiler-owned.
Drivers, providers, and capabilities are normal BAML code and may be extended
with out-of-body `implements` blocks.
