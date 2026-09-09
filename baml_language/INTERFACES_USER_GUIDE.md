# Using BAML interfaces from Python, TypeScript, and Rust

> **Design preview.** This guide describes the SDK experience proposed in [the interface design](/Users/aaron/projects/baml/baml_language/MEDIA_INTERFACES_AND_BRIDGES_DESIGN.md). General interface support and several helpers shown here are not implemented in the current SDKs. These examples specify the API we intend to deliver, rather than instructions for a released version.

A BAML interface lets a function work with different implementations of the same operations. The implementation can live in BAML or a supported host language. This guide focuses on Python, TypeScript and Rust, with Go signatures alongside the associated-type examples.

If you are using an implementation supplied by BAML, it usually just works: create the object, pass it to a function, and call its methods. You do not need to convert it to an interface first.

| What you want to do | What you need to know |
|---|---|
| Override an LLM function's client | Pass the generated client directly. |
| Call `Agent.run(spec)` | Call the method directly. A typed spec supplies the output type. |
| Use an interface returned by BAML | Keep the returned object and await its methods. |
| Implement an interface in your language | Python uses a generated implementation base; TypeScript/Rust use `Greeter.implement(...)` / `Greeter::implement(...)`. Pass the resulting object directly. |
| Keep a callback or host implementation around | Ordinary calls retain what BAML needs automatically. Keep the runtime, and any explicitly chosen shorter scope, open. |
| Use an interface with associated types | Choose the associated type when binding; keep that choice fixed. |
| Implement a method with its own type parameters | Prefer an ordinary native adapter where supported; use explicit `bind_handlers` for advanced generic dispatch. |
| Store an interface in JSON or a database | Store application data or configuration; a live interface object is not a serialized value. |

Generate the SDK with `baml generate`, as usual. No interface-specific generation command is needed. Imported BAML packages such as `ai` and provider packages must be included in the SDK's configured export roots or reachable public API.

Examples assume your SDK is named `baml_sdk`, your generated application functions live at its root, and your application has initialized it normally. Python and TypeScript examples run inside an async application. Rust examples run inside an async function returning a compatible `Result`.

## What is generated for an existing BAML implementation?

`ResponsesClient` already exists in BAML and implements `Client` and `StreamingClient`. Its generated Python class is a reference to that BAML object. It contains forwarding methods, not a second OpenAI implementation and not a host implementation base.

```python
# Proposed generated signature excerpt. Imports and most factory options omitted.
class ResponsesClient:
    @classmethod
    async def new_async(
        cls, *, model: str = "gpt-4o-mini",
    ) -> "ResponsesClient": ...

    async def id(self) -> str: ...
    async def render(self, input: ModelTurnInput) -> Request: ...
    async def invoke(self, input: ModelTurnInput) -> ModelTurn: ...
    async def invoke_stream(self, input: ModelTurnInput) -> TurnStream: ...
```

The factory executes in BAML and returns a live reference. Calling `await client.invoke(input)` invokes the existing BAML method and returns a `ModelTurn` record. Passing the client back into BAML preserves its receiver; it does not register a Python callback. Public inherent BAML methods are generated too; this excerpt focuses on the implemented interfaces.

| Generated role | Purpose |
|---|---|
| `ResponsesClient` | Use a specific BAML implementation, with its factory and methods. |
| `ClientRef` | Use a returned Client without needing its concrete class name. |
| `ClientImplementation` (Python) / `ClientHost` (TS, Go, Rust contract) | Supply a new implementation from the host language. |
| `ClientInput` | Accept generated concrete clients, checked refs and explicitly declared host implementations as function arguments. |

A generated BAML facade does not inherit the host implementation base. The compiler's implementation declaration proves membership, and the generated input codec checks and projects its receiver. No `.as_client()` step is needed.

## Client overrides: pass the client

Suppose your BAML project declares an LLM function named `extract_resume`. To override its client for one call, construct a generated provider client and supply it:

### Python

```python
from baml_sdk import extract_resume_async
from baml_sdk.vendor.openai import ResponsesClient

client = await ResponsesClient.new_async(model="my-model")
resume = await extract_resume_async(text, client=client)
```

### TypeScript

```typescript
import { extract_resume_async } from "./baml_sdk/index.js";
import { ResponsesClient } from "./baml_sdk/vendor/openai/index.js";

const client = await ResponsesClient.new({ model: "my-model" });
const resume = await extract_resume_async(text, { client });
```

### Rust

```rust
use baml_sdk::{extract_resume_async, ExtractResumeOptions};
use baml_sdk::vendor::openai::{ResponsesClient, ResponsesClientOptions};

let client = ResponsesClient::new(
    ResponsesClientOptions::default().model("my-model"),
).await?;

let resume = extract_resume_async(
    text.to_owned(),
    ExtractResumeOptions::default().client(&client),
).await?;
```

The Rust options builder is the proposed generated surface for optional named arguments. Its `client` setter accepts generated implementing objects directly and retains what the call needs. Omitting the setter leaves the function's default selection in place.

There is no `.as_client()`, host binding, or output-type token in these examples. `ResponsesClient` already implements BAML's `Client` interface. A different generated implementation works the same way. The interface parameter determines which operations the function can use.

The same applies to a streaming entrypoint: pass the client directly. It must implement the interface required by that entrypoint's generated signature. Creating a client does not give it streaming support that its implementation lacks.

The async return conventions are the same for functions and methods: Python `await` yields the declared native value and failures raise; TypeScript returns `Promise<T>`; Rust `.await` yields `Result<T, Error<E>>`, and `.await?` yields `T`. `E` is the declared BAML error type, or `std::convert::Infallible` for `throws never`. Client creation yields a live `ResponsesClient`; extraction yields a copied `Resume` model, object, or struct.

No ordinary Rust call requires a caller-created `ctx`. Refs carry their runtime; free functions and factories use the selected SDK runtime. Optional per-call controls use generated `_with_options` siblings consistently. Incoming `HostCallContext` is different: the bridge supplies it to a host implementation for cancellation and tracked work during one invocation; it is not an ownership scope.

## `Agent.run(spec)`: call the method normally

An `Agent` is a generated BAML object that implements `Runner`. You can call its `run` method directly.

The client override belongs on `Agent.new(...)`. `run(...)` receives a function spec. If the agent has no client override, it uses the spec's default client.

### Python

```python
from baml_sdk import extract_resume_spec_async
from baml_sdk.ai import Agent

agent = await Agent.new_async(client=client, max_steps=12)
spec = await extract_resume_spec_async(text)

result = await agent.run(spec)
resume = result.value
```

### TypeScript

```typescript
import { extract_resume_spec_async } from "./baml_sdk/index.js";
import { Agent } from "./baml_sdk/ai/index.js";

const agent = await Agent.new({ client, max_steps: 12 });
const spec = await extract_resume_spec_async(text);

const result = await agent.run(spec);
const resume = result.value;
```

### Rust

```rust
use baml_sdk::extract_resume_spec_async;
use baml_sdk::ai::{Agent, AgentOptions};

let agent = Agent::new(
    AgentOptions::default().client(&client).max_steps(12),
).await?;
let spec = extract_resume_spec_async(text.to_owned()).await?;

let result = agent.run(&spec).await?;
let resume = result.value;
```

`spec` has type `FunctionSpec<Resume>`, so `run` returns `RunResult<Resume>`. Python and TypeScript obtain the BAML output type from the spec; Rust also infers its native type parameter from the typed argument. You do not need `.as_runner()` or a separate `Resume` type token.

The same agent can run another spec with a different output type. `Agent` itself is not parameterized by the output type.

`RunResult` is an ordinary result record: read `result.value`, `result.journal`, and `result.usage` directly. Its journal is a live BAML object, so journal methods still use async calls. The value follows the output type's usual representation. The direct `extract_resume_async(...)` call already returns the extracted value when you do not need the full run result.

Records are copied across the boundary, while live objects inside them remain shared. Replacing a record field locally does not update the original BAML record; calling a method on its journal acts on the shared journal. Reading these result fields needs no extra runtime call.

If you write a BAML function that accepts a `Runner`, pass the agent directly there too. A function that accepts a custom runner's error type must preserve that type; an agent with fallible `run` does not satisfy a `Runner` annotation whose associated `Error` is fixed to `never`.

## A small interface you can use in all three languages

Declare the shared operations in BAML:

```baml
interface Greeter {
    function greet(self, name: string) -> string throws never
    function label(self) -> string throws never { "greeter" }
}

class FriendlyGreeter {
    prefix: string,

    implements Greeter {
        function greet(self, name: string) -> string throws never {
            `${self.prefix}, ${name}!`
        }
    }
}

function welcome(greeter: Greeter, name: string) -> string throws never {
    greeter.greet(name)
}

function make_greeter() -> Greeter throws never {
    FriendlyGreeter { prefix: "Hello" }
}
```

`make_greeter` returns an implementation of `Greeter`. You can call its methods or pass it back to another BAML function:

### Python

```python
from baml_sdk import make_greeter_async, welcome_async

greeter = await make_greeter_async()
message = await greeter.greet("Ada")          # "Hello, Ada!"
label = await greeter.label()                 # "greeter"
again = await welcome_async(greeter, "Grace") # "Hello, Grace!"
```

### TypeScript

```typescript
import { make_greeter_async, welcome_async } from "./baml_sdk/index.js";

const greeter = await make_greeter_async();
const message = await greeter.greet("Ada");
const label = await greeter.label();
const again = await welcome_async(greeter, "Grace");
```

### Rust

```rust
use baml_sdk::{make_greeter_async, welcome_async};

let greeter = make_greeter_async().await?;
let message = greeter.greet("Ada".to_owned()).await?;
let label = greeter.label().await?;
let again = welcome_async(&greeter, "Grace".to_owned()).await?;
```

The returned interface object keeps the implementation's identity and state. Passing it back does not create a new greeter. Its `label` method uses the default body declared in BAML.

You do not need the concrete `FriendlyGreeter` type to use this interface. The implementation could change without changing these calls.

## Implement an interface in your language

To supply behavior from your application, explicitly declare which BAML interface you implement. Python offers a generated implementation base; TypeScript and Rust offer a synchronous implementation factory. Runtime binding happens automatically at an async call boundary. All three languages also offer a host contract and explicit binder for adapters and advanced registration choices.

### Python

```python
from pydantic import BaseModel
from baml_bridge import HostCallContext
from baml_sdk import GreeterImplementation, welcome_async

class PythonGreeter(BaseModel, GreeterImplementation):
    prefix: str

    async def greet(self, name: str, *, ctx: HostCallContext | None = None) -> str:
        if ctx is not None:
            ctx.raise_if_cancelled()
        return f"{self.prefix}, {name}!"

greeter = PythonGreeter(prefix="Hello")
message = await welcome_async(greeter, "Ada")
label = await greeter.label()
```

`GreeterImplementation` is generated from the BAML interface. It declares your intent to implement that contract and supplies the default `label()` method, which executes its BAML body. The SDK binds the object when it first crosses into BAML and reuses its active identity on later calls. Creating or validating the Pydantic model does not contact the runtime.

Pydantic is optional. An ordinary class can inherit `GreeterImplementation` and use its own constructor. You do not write a destructor or manage bridge handles.

Calling `await greeter.greet("Ada")` directly on this Python implementation is an ordinary Python call; it can omit `ctx`. BAML calls the same Python receiver through the bridge, which supplies the context and checks arguments/results. Both paths share state. Calling a generated BAML default such as `label()` still enters BAML; calling a bound SDK ref uses its checked invocation path. Local calls do not automatically gain bridge cancellation or lifetime checks.

For an existing class you cannot change, implement `GreeterHost`'s methods and bind explicitly:

```python
from baml_sdk import GreeterRef

greeter = await GreeterRef.bind(existing_python_greeter)
message = await welcome_async(greeter, "Ada")
label = await greeter.label()
```

The scope argument is optional. Omitting it means there is no shorter revocation boundary than the runtime; it does **not** retain every bound object until runtime shutdown. A registration can be released while the runtime remains open once its last BAML reference, SDK reference and active operation are gone and the necessary collection/release processing runs.

An existing method-shaped object without the generated implementation declaration still needs this explicit bind to establish interface membership. That is a type-registration choice, not a requirement to keep the object forever. The binder also supports explicit associated-type choices, adapter overrides and runtime selection.

For deterministic earlier teardown, choose a shorter scope:

```python
import baml_sdk

async with baml_sdk.bridge_scope() as scope:
    greeter = await GreeterRef.bind(existing_python_greeter, scope=scope)
    message = await welcome_async(greeter, "Ada")
```

Leaving this block revokes the scoped registration even if BAML retained a reference, and waits for admitted bridge work to drain. Ordinary reference cleanup is automatic. Use an explicit scope for earlier callback/interface revocation or as an application-chosen teardown edge for registrations that could participate in a cycle. Application resources remain yours to close; the scope does not call arbitrary `close()` or `aclose()` methods on your implementation.

### TypeScript

```typescript
import { Greeter, welcome_async } from "./baml_sdk/index.js";

using greeter = Greeter.implement({
  greet(name, ctx) {
    ctx.signal.throwIfAborted();
    return `Hello, ${name}!`;
  },
});

const message = await welcome_async(greeter, "Ada");
const label = await greeter.label();
```

`Greeter.implement` accepts an object literal or a class instance providing the generated `GreeterHost` contract. It is synchronous and only creates the implementation object; runtime registration happens when it is used. Reuse that result to keep the same active BAML identity. A second factory call deliberately creates a separate adapter instance, even if you pass the same raw object.

A TypeScript implementation may return a value or a Promise. Calling through the generated interface is always asynchronous, including the inherited BAML `label` default. A TypeScript `implements GreeterHost` clause or a cast does not supply runtime registration metadata; the factory provides that explicit declaration. No decorator or generated superclass is required.

`using` calls the wrapper's `[Symbol.dispose]()` when the containing block exits, including on exceptions. It releases your local reference; an independently retained BAML reference keeps the implementation alive. You can use `const` for a longer-lived implementation and call `close()` explicitly when finished. Garbage-collection cleanup is a fallback, not a promise of timely disposal.

For an explicit registration lifetime, use `await using` on the bridge scope:

```typescript
await using scope = await bamlSdk.bridgeScope();
using greeter = await GreeterRef.bind(implementation, { scope });
await welcome_async(greeter, "Ada");
```

The scope's `[Symbol.asyncDispose]()` revokes its scoped callbacks and interface registrations and awaits admitted bridge work. It does not dispose independently owned application resources. Omitting scope uses the SDK runtime's default. Await calls that must finish before scope exit; if returning a call's result from the block, use `return await ...` so cleanup runs after it completes.

**Node 22:** compile this TypeScript to an older JavaScript target such as ES2022, with `ESNext.Disposable` included in your TypeScript libs. Node 22 provides both disposal symbols but rejects the raw syntax. TypeScript can compile the declarations away; Node 24 supports the syntax natively. Plain JavaScript targeting Node 22 can call `.close()` from `try/finally`.

### Rust

```rust
use std::convert::Infallible;
use baml_bridge::{Error, HostCallContext};
use baml_sdk::{BoxFuture, Greeter, GreeterHost, welcome_async};

struct RustGreeter {
    prefix: String,
}

impl GreeterHost for RustGreeter {
    fn greet<'a>(
        &'a self,
        name: String,
        ctx: HostCallContext,
    ) -> BoxFuture<'a, Result<String, Error<Infallible>>> {
        Box::pin(async move {
            ctx.check_cancelled()?;
            Ok(format!("{}, {}!", self.prefix, name))
        })
    }
}

let greeter = Greeter::implement(RustGreeter { prefix: "Hello".to_owned() });

let message = welcome_async(&greeter, "Ada".to_owned()).await?;
let label = greeter.label().await?;
```

`Greeter::implement` moves your implementation into a generated wrapper with shared ownership internally. It is synchronous; no scope, async bind, or user-written destructor is necessary. The wrapper implements the function's generated input role and binds when the call needs it. Its `Clone` shares the implementation; `Drop` releases that clone's ownership. BAML retains its own ownership if it stores the object, so dropping your local wrapper does not invalidate a retained BAML reference.

The generated host trait requires `Send + Sync + 'static`. Move or share owned application state into the implementation; do not borrow a stack-local value that may disappear while BAML still holds the object. Use a mutex or another synchronization mechanism when calls mutate shared state. Do not hold a synchronous lock across an `await`. Ordinary `Drop` releases ownership; awaiting bridge-work drain requires an explicit async scope close or runtime shutdown. Resources inside your host implementation still follow their own native ownership and application cleanup rules.

The factory declares the interface and all its required interfaces. Implementing extra Rust traits alone does not register additional BAML interfaces: Rust has no general mechanism to enumerate those traits. Use the explicit host-type registration API for an object exposing several unrelated BAML interfaces. This keeps one stable implementation set for the object. The scoped `bind_greeter(&scope, implementation, overrides).await?` form remains available for earlier revocation or explicit overrides.

The boxed future is part of the generated Rust host contract. It allows the SDK to call different implementations through the same interface while borrowing `self` for a call. The generated ref's ordinary methods remain normal async Rust methods.

### Required methods and defaults

The implementations above provide only `greet`. BAML supplies the default `label` body. A default can call other methods on the same object, including your host implementation.

When using Python's generated implementation base, overriding its inherited `label` member explicitly replaces that default. The SDK records the override when binding. With the lower-level host contract and binder, use the generated overrides argument: Python `GreeterOverrides(label=...)`, TypeScript `{ overrides: { label: ... } }`, or Rust `GreeterOverrides` with its generated callback type. An omitted override keeps the BAML default; a same-named extra method on an unrelated raw host object is not automatically an override.

## Which generated names do I use?

For an interface such as `Greeter`, the SDK exposes these roles:

| Name | Use it for |
|---|---|
| `GreeterHost` | Type-checking or implementing new host behavior. |
| `GreeterRef` | The live interface object returned by BAML or by binding a host implementation. |
| `GreeterImplementation` | Python's opt-in base, or the owned implementation object produced by the TypeScript/Rust factory. Handles automatic binding and BAML defaults. |
| `Greeter.implement` / `Greeter::implement` | TypeScript/Rust factory declaring a host implementation without async runtime registration. |
| `GreeterInput` | Annotating a helper accepting generated concrete implementors, checked refs, and declared native implementation objects. |

Most application code can let the SDK and type checker infer these types. Generated BAML functions already accept the appropriate input role. Python protocols, TypeScript interfaces, and Rust traits express it according to the host language.

Use the same distinction for provider clients: `ClientHost` is for implementing your own client, while generated `ResponsesClient` values already satisfy `ClientInput`. They do not need to go through `ClientHost` or binding again.

## Associated types: choose once for each implementation

An associated type describes a choice made by the implementation:

```baml
class DecodeError { message: string }

interface Decoder {
    type Output
    function decode(self, input: image) -> Self.Output throws DecodeError
}

function read_text(decoder: Decoder<Output=string>, input: image)
    -> string throws DecodeError {
    decoder.decode(input)
}
```

`read_text` requires a decoder whose `Output` is exactly `string`.

### What does the caller see?

The following are proposed generated signature excerpts. T is the native representation of the fixed BAML Output type; it is not chosen anew on each decode call.

```python
class DecoderRef(Generic[T]):
    async def decode(self, input: BamlImage) -> T: ...
```

```typescript
interface DecoderRef<T> {
  decode(input: BamlImage): Promise<T>;
}
```

```go
func (d DecoderRef[T]) Decode(ctx context.Context, input baml_go.Image) (T, error)
```

```rust
impl<T: BamlValue> DecoderRef<T> {
    pub async fn decode(&self, input: BamlImage) -> Result<T, Error<DecodeError>>;
}
```

| BAML return type | Python | TypeScript | Go | Rust |
|---|---|---|---|---|
| `Decoder<Output=string>` | `DecoderRef[str]` | `DecoderRef<string>` | `DecoderRef[string]` | `DecoderRef<String>` |
| `Decoder<Output=Invoice>` | `DecoderRef[Invoice]` | `DecoderRef<Invoice>` | `DecoderRef[Invoice]` | `DecoderRef<Invoice>` |

After obtaining a text decoder, Python calls `text = await decoder.decode(image)` and receives `str`; TS receives `string` after await; Go receives `(string, error)`; Rust receives `Result<String, Error<DecodeError>>` after await. Python and TS raise/reject with SDK exceptions on failure. None of these callers constructs a type token or `GenericCall`.

A BAML-defined concrete implementation supplies these choices already. The registration steps below apply when supplying a new host implementation.

### How does the host implementation choose Output?

In Python, bind a string-returning implementation with the string type token:

```python
from baml_sdk import DecoderRef, types

decoder = await DecoderRef.bind(
    implementation,
    output=types.string,
)
```

In TypeScript, the native generic type and BAML type token agree:

```typescript
const decoder = Decoder.implement<string>(implementation, {
  output: types.string,
});
```

Python and TypeScript cannot reliably recover BAML runtime types from native generic annotations alone. The token supplies that information. For a generated model, use its generated token, such as `types.Invoice`.

TypeScript checks the token against the output type inferred from your implementation or selected by `implement<string>(...)`. A wider token does not silently widen that choice. If you intentionally want a union output contract, select it explicitly—for example, `implement<string | number>(...)` with its matching generated token. The explicit `DecoderRef.bind` API uses the same rule. Runtime checks still apply if a type assertion or `any` bypasses the compiler.

In Go, generate `DecoderHost[T]` with `Decode(context.Context, BamlImage) (T, error)` and `DecoderRef[T]` for callers. Binding takes a matching `TypeToken[T]`; Go has no native associated-type declaration to extract. A `DecoderHost[string]` becomes a `DecoderRef[string]`. The explicit binder in design §9.5 shows scoped registration; an ordinary binding uses the SDK runtime's default lifetime. The type token identifies a supported BAML type, not merely a Go reflection name.

In Rust, your implementation supplies the associated type in its trait implementation:

```rust
impl DecoderHost for TextDecoder {
    type Output = String;

    fn decode<'a>(
        &'a self,
        input: BamlImage,
        ctx: HostCallContext,
    ) -> BoxFuture<'a, Result<String, Error<DecodeError>>> {
        Box::pin(self.decode_text(input, ctx))
    }
}

let decoder = Decoder::implement(TextDecoder::new());
```

Here `decode_text` is your application's implementation, with the shown result type. The generated factory derives the BAML output token from `String`; no second token is needed.

The associated type stays fixed. A `Decoder<Output=string>` does not become a `Decoder<Output=Invoice>` because you annotate or cast it differently. A default associated type is an actual default choice, not a wildcard accepting any choice.

If Output is a runtime-created BAML type with no generated model, the SDK needs its checked type descriptor as well as its dynamic value representation. Several BAML types may use the same native dynamic wrapper; that wrapper's Python/TS/Go/Rust type alone cannot identify Output. The ordinary Rust factory above derives the token only for statically represented types such as String. The explicit dynamic binding API remains a design gate; see the internal [codegen review note](INTERFACES_CODEGEN_REVIEW.md).

### What if the associated type is an error?

Keep it in the generated type too. For example, `Iterator<Item=string, Error=ReadError>` becomes `IteratorRef[str, ReadError]` in Python, `IteratorRef<string, ReadError>` in TS, `IteratorRef[string, ReadError]` in Go, and `IteratorRef<String, ReadError>` in Rust.

Its `next()` returns an item or the generated `Done` sentinel on success. Python/TS raise or reject with the SDK's typed BAML-error wrapper for a ReadError; Go returns a typed wrapper through `error`; Rust returns `Error<ReadError>` through `Result`. The binding remains exact even where the host language has no typed exceptions. A default `Error=never` means no application errors, not “any error type.” Bridge failures remain possible.

The intended Rust native spelling for never is `!`; the repository's current Rust 1.93 requires `std::convert::Infallible` in generic positions. Switching generated code to `!` requires a supported-toolchain update; no such update is implied by these examples.

## Can an array or string implement an interface?

Yes. The stdlib implements `Iterable` for BAML arrays and strings. Returning an interface does not require a class receiver:

```baml
function names() -> string[] throws never {
    ["Ada", "Grace"]
}

function iterable_names()
    -> baml.iter.Iterable<Item=string, Error=never> throws never {
    ["Ada", "Grace"]
}

function as_string_iterable(values: string[])
    -> baml.iter.Iterable<Item=string, Error=never> throws never {
    values
}
```

```python
from baml_sdk import names_async, iterable_names_async, as_string_iterable_async

copied = await names_async()                # list[str]: copied data
source = await iterable_names_async()       # IterableRef[str, Never]: live BAML ref
iterator = await source.iter()              # IteratorRef[str, Never]
first = await iterator.next()               # str | Done; this call yields "Ada"

local = ["Ada", "Grace"]
converted = await as_string_iterable_async(local)
local.append("Lin")  # Does not update the BAML-owned copy behind converted.
```

`Never` in these Python type comments is the native uninhabited typing type, not a value to construct. `Done` is distinct from null; actual elements may themselves be nullable. The ref retains the BAML receiver, and iterator methods update BAML-owned traversal state. Passing a ref back preserves that receiver. An ordinary collection result still uses its copied-data codec.

A native list is not automatically a live host implementation of `Iterable`. The `as_string_iterable` example makes the copy and source element type explicit using an ordinary BAML function. To keep iteration state in Python instead, declare a host implementation and let BAML call it. Automatic native-container convenience conversions are not promised in this PR. Empty/heterogeneous collections must not have their interface bindings guessed from their current elements.

## When do unsupported cases fail?

| Situation | Earliest reliable check |
|---|---|
| Invalid BAML implementation or conflicting associated types | BAML compilation. |
| An SDK cannot expose a statically known contract | SDK generation, with the member and supported alternative identified. |
| Wrong host registration, type token, or unadapted native collection | Binding or argument encoding before the receiver runs; native type checking may catch it sooner. |
| Closed/revoked ref or wrong runtime | Call admission. |
| Host method returns a value or error outside its declared contract | Completion, before BAML receives it. |

A dynamic value API may support a contract that lacks a generated native model. An unsupported member must not silently disappear from the generated SDK. Registration cannot prove that arbitrary future Python/TS/Go/Rust method bodies return valid results, so each bridge also validates actual results.

## Methods with type parameters: infer them when possible

An associated type belongs to an implementation. A method's type parameter belongs to a particular call.

`Agent.run<Out>(spec)` is the common example: one agent can process specs returning different types. A typed `FunctionSpec<Out>` supplies the output type, so ordinary `agent.run(spec)` calls need no extra tokens.

Some methods have no argument from which to infer a type. For a method `make<T>(self) -> T`, supply it explicitly:

| Language | Call |
|---|---|
| Python | `await factory.make(_types={"T": types.Person})` |
| TypeScript | `await factory.make<Person>({ types: { T: types.Person } })` |
| Rust | `factory.make::<Person>().await?` |

If multiple arguments imply conflicting types, fix the arguments rather than forcing a contradictory token. A dynamically loaded BAML type may be represented by a checked dynamic value when there is no generated native model for it.

### Implementing a method that must work for every type

**Start with the method.** Earlier, `Greeter.greet` always accepted and returned strings. A *generic method* lets a caller choose a type for each call. For example, `Echo.echo` accepts a value and returns a value of the same type:

```baml
interface Echo {
    function echo<T>(self, value: T) -> T throws never
}
function echo_text(echo: Echo, text: string) -> string throws never {
    echo.echo(text)
}
```

`T` stands for the type selected for that call. `echo_text` passes a string, so its call uses `T = string`. Another BAML function could pass an integer or a record. The caller uses ordinary arguments and never constructs a bridge invocation object.

**What does the bridge know?** For a call with `T = string` and value `"Ada"`, it knows:

```text
Method:          echo
Type argument:   T = string
Arguments:       value = "Ada"
Expected result: string
```

It also knows the receiver and cancellation context. A **`GenericCall`** is an SDK object containing this information for one invocation. It is not the implementing object, and it does not represent the interface's lifetime.

**When would an implementer see it?** Usually, the SDK should unpack the invocation and call an ordinary host method:

```text
BAML call
  → SDK reads and checks the arguments
  → host method receives its normal arguments
  → SDK checks the returned value
```

A fully general generic implementation may need to handle BAML types without generated native models, or operations the host's type system cannot express directly. For these cases, offer an explicit advanced handler API that receives the checked invocation itself.

Runtime type information does not, by itself, require exposing `GenericCall`. Prefer generated adapters to ordinary host methods wherever they can preserve the full BAML contract. A simple identity method such as `echo` may work through an ordinary adapter; we use it below to make the advanced mechanism easy to follow.

`GenericCall` and `HostOutcome` below are SDK-provided advanced handler types. `call` is constructed by the bridge; `HostOutcome` is the checked completion package explained after the example. The Node example imports their proposed public type exports from `@boundaryml/baml-bridge`; the Web bridge must export equivalent types from `@boundaryml/baml-bridge-web`.

**Register a handler, then call the returned object.** The proposed advanced entrypoint is `bind_handlers`. Its argument is explicitly handler configuration, and its result is the ordinary callable interface reference:

```typescript
import type { GenericCall, HostOutcome } from "@boundaryml/baml-bridge";
import { Echo, echo_text_async } from "./baml_sdk/index.js";

const echo = await Echo.bind_handlers({
  async echo(call: GenericCall): Promise<HostOutcome> {
    // The bridge constructs this GenericCall for one invocation.
    const value = call.argument("value");
    return call.return_value(value);
  },
});

const first = await echo.echo("Ada");
const second = await echo_text_async(echo, "Grace");
// first === "Ada"; second === "Grace"
```

```text
Your call: echo.echo("Ada")
  → SDK constructs GenericCall
  → SDK invokes the registered handler
  → handler retrieves the checked argument
  → handler returns it through call.return_value(...)
  → SDK delivers the native string "Ada" to your caller
```

`call.argument("value")` retrieves the argument named `value` in the BAML declaration. The result carries its BAML type information; it is not necessarily an ordinary JavaScript string or object.

`call.return_value(value)` packages a result checked against this invocation's expected return type. That package is called a `HostOutcome`. The caller ultimately receives `"Ada"`, not the package. A constrained generic method can also use operations promised by its type bounds through the call object.

The SDK constructs `GenericCall` and invokes the handler. Application callers use the returned `echo` reference. Python and Rust follow the same distinction between explicit handler registration and ordinary interface calls; their precise adapters depend on what each language can express.

**How does this relate to agents?**

| What you are doing | API you need |
|---|---|
| Using BAML's `Agent` | `await agent.run(spec)`. Its implementation already exists; no `GenericCall`. |
| Implementing an ordinary host interface | Normal host methods where supported. |
| Implementing a fully general runner | Potentially an advanced handler, because each call can request a different output type. |

`GenericCall` is an advanced implementation API, not the normal way to call or implement every BAML interface.

## Errors and cancellation

The BAML declaration determines which application errors a method may throw. Python exceptions, rejected Promises, and Rust errors must be translated to a declared BAML error when they represent an expected application failure.

For the earlier decoder, use the generated `DecodeError` value:

| Language | Report an expected decode failure from a host method |
|---|---|
| Python | `raise_baml(DecodeError(message="Unsupported image"))` |
| TypeScript | `throwBaml(new DecodeError({ message: "Unsupported image" }))` |
| Rust | `Err(Error::thrown(DecodeError { message: "Unsupported image".into() }))` |

Python's `raise_baml` and TypeScript's `throwBaml` raise a BAML error wrapper. Rust's constructor creates the typed `Error::Thrown` case; callers can match that case. The wrapper retains the original BAML error value so callers can handle it by type.

A wrong return type, an undeclared error, a closed object, or runtime shutdown is not a successful implementation of the method's declared error contract. These surface as separately classified SDK/runtime failures. Even a method declared `throws never` can encounter runtime failure or cancellation when called through an SDK.

Host methods receive a call context. Use its cancellation signal when waiting on HTTP, database, or other async work. Cancellation belongs to the invocation; it does not close the registered object. Calls can overlap, and a host callback can call BAML again. Keep mutable state synchronized and avoid blocking the event loop that needs to run your callbacks.

Python and Rust async applications should use the async SDK. TypeScript interface calls are Promise-based in Node and browsers. A fast or synchronous host implementation does not make a blocking interface call safe.

## Stateful objects: call methods on the owner

State can persist across calls without exposing cross-language writable fields. Give the state one authoritative owner and put useful operations on the interface. This is the PR’s bridge boundary, not a new BAML rule: either language may request a mutating method, but its implementation changes state in the owning language:

```baml
interface Counter {
    function current(self) -> int throws never
    function add(self, amount: int) -> int throws never
}

class StoredCounter {
    stored_count: int,

    implements Counter {
        function current(self) -> int throws never {
            self.stored_count
        }

        function add(self, amount: int) -> int throws never {
            self.stored_count += amount;
            self.stored_count
        }
    }
}

function make_counter(start: int) -> Counter throws never {
    StoredCounter { stored_count: start }
}

function AddAndReturn(counter: Counter, amount: int) -> Counter throws never {
    counter.add(amount);
    counter
}
```

Here BAML owns `stored_count`. The SDK exposes `current()` and `add()`, not that concrete field. A call from Python, TypeScript, or Rust runs the method against the same BAML receiver:

### Python

```python
from baml_sdk import AddAndReturn_async, make_counter_async

counter = await make_counter_async(10)
assert await counter.add(2) == 12
same_counter = await AddAndReturn_async(counter, 5)
assert await same_counter.current() == 17
assert await counter.current() == 17
```

### TypeScript

```typescript
import { AddAndReturn_async, make_counter_async } from "./baml_sdk/index.js";

const counter = await make_counter_async(10);
console.assert((await counter.add(2)) === 12);
const sameCounter = await AddAndReturn_async(counter, 5);
console.assert((await sameCounter.current()) === 17);
console.assert((await counter.current()) === 17);
```

### Rust

```rust
use baml_sdk::{AddAndReturn_async, make_counter_async};

let counter = make_counter_async(10).await?;
assert_eq!(counter.add(2).await?, 12);
let same_counter = AddAndReturn_async(&counter, 5).await?;
assert_eq!(same_counter.current().await?, 17);
assert_eq!(counter.current().await?, 17);
```

Passing the object back retains its receiver identity; it does not serialize and reconstruct `StoredCounter`. The host is invoking behavior on BAML-owned storage, so it is not a second storage writer. `counter` and `same_counter` can be distinct SDK wrappers owning references to the same receiver; native wrapper identity is not promised.

A host language can instead own the state. For example, a Python implementation can keep `_count` privately and implement the same methods:

```python
from baml_sdk import CounterImplementation, AddAndReturn_async

class PythonCounter(CounterImplementation):
    def __init__(self, start: int):
        self._count = start

    async def current(self, *, ctx):
        return self._count

    async def add(self, amount: int, *, ctx):
        self._count += amount
        return self._count

counter = PythonCounter(10)
same_counter = await AddAndReturn_async(counter, 5)
assert await same_counter.current() == 15
assert counter._count == 15
```

In this version Python is the sole state owner. A BAML call to `add` dispatches to Python; BAML does not hold a writable replica of `_count`. TypeScript and Rust implementations follow the same rule and use their language's normal synchronization when calls may overlap.

Ordinary data still crosses as data. Generated DTO fields such as `result.value`, strings, numbers, lists, maps, and nested data classes use their normal SDK representations. Editing a copied DTO in the host does not mutate an earlier BAML value; pass the changed DTO into another call if BAML should receive it. A DTO may contain a live object such as `result.journal`; its ordinary envelope fields remain data, while calls on that live object's methods operate on its retained receiver.

The proposed SDK does not expose general-purpose `get_field`, `set_field`, `ListRef`, map views, or nested shared-storage views for live interface receivers. Interface-field host binding is deferred. A BAML-defined, field-bearing interface may still project useful methods from a BAML-owned implementing class, but only when the runtime has validated the actual implementation witness. Do not treat a matching host object shape as proof that it implements such an interface.

Method calls may overlap. If an operation such as `add` must be atomic, the owning language implements that guarantee around its own storage. Await calls whose ordering matters; cancellation after dispatch does not imply rollback.

## Lifetimes: temporary, retained, and cyclic values

Interfaces follow the same receiver-lifetime rules as callbacks. Passing a callable keeps its captured state alive while BAML needs it; passing an interface implementation keeps its receiver and checked implementation information alive. Python's generated implementation base and TypeScript/Rust's implementation factories declare the interface. Ordinary calls do not require a scope block or manual registration call.

This includes client overrides and `Agent.run(spec)`: call them normally. A long-lived application may use one global SDK runtime for all three common cases. A BAML-owned counter does not need a Python implementation registration merely because Python calls it; it needs an owned proxy reference rooting its BAML receiver.

**Temporary use.** A request creates an implementation, passes it to BAML, and neither side retains it after the call. Once the call and local references are gone, the receiver is eligible for the normal release and collection process. Keeping the global runtime open must not itself make every temporary receiver permanent, although collection and finalizer timing are not immediate guarantees.

**Intentional retention.** BAML may store a callback, client, service, or counter after the original call finishes, or the host may keep a returned checked ref. That owner keeps the same receiver usable on the open runtime. Passing a returned ref back later continues to target that receiver and its owner-held state. Releasing one local wrapper does not invalidate another live owner.

**A cycle across runtimes.** A host object can retain a BAML ref while BAML retains the host object. Independent garbage collectors cannot generally prove that such a cross-runtime cycle is unreachable. Arrange an explicit application teardown edge when this ownership shape is possible: clear one side's retained reference, close the containing service/runtime, or use an optional shorter scope whose close revokes the registrations it owns. The SDK does not promise automatic cross-runtime cycle collection. Ordinary cycles entirely within BAML remain the BAML collector’s responsibility; a BAML-defined object is not itself evidence of a cross-runtime cycle.

| Action | What it means |
|---|---|
| Pass an ordinary callback/interface input | The SDK automatically retains what the call and any retained BAML values need |
| Store a returned checked ref | Keep its receiver usable on the same open runtime |
| `close()` / TypeScript `using` / Rust `Drop` | Release your particular SDK reference; other owners remain valid |
| Choose an explicit shorter scope | Opt into earlier revocation of its host callbacks and interface registrations |
| Close that scope | Prevent new operations and wait for admitted bridge work to drain; retained refs cannot extend its lifetime |
| Drop a concrete image | Release ordinary media-data ownership, independently of host registration scopes |

Python and TypeScript checked refs expose `clone()` for independent ownership of the same receiver. Ordinary assignment aliases the same wrapper, so closing that wrapper affects its aliases. Rust uses `Clone`/`Drop`; cloning preserves shared receiver state. Your original Python callable, Pydantic implementation or raw JS object is a native object, not a checked proxy: dropping a bridge reference neither makes native local access invalid nor calls that object's application cleanup methods.

For services used across requests, the global runtime's default lifetime is normally sufficient. Keep it open while using live refs. A scope adds a shorter revocation boundary when the application needs one; it is optional and does not replace ordinary receiver ownership.

Memory can still accumulate when either side deliberately retains values, collection/release queues have not run yet, or a cycle spans the runtimes. JavaScript finalization has no guaranteed timing, so use explicit disposal when deterministic local release matters. For cycles, design and invoke an explicit teardown path; a scope is one possible boundary, not an automatic proof that every cycle has been found or collected.

The same optional scope can select callback and interface lifetimes. In the proposed Python API, for example:

```python
# Illustrative generated functions: UseCallback_async and welcome_async.
async with baml_sdk.bridge_scope() as scope:
    callback_input = scope.callable(callback)
    greeter = await GreeterRef.bind(existing_greeter, scope=scope)
    await UseCallback_async(callback_input)
    await welcome_async(greeter, "Ada")
# Both scoped registrations are revoked, even if BAML retained them.
```

`scope.callable` is an optional callback-input wrapper that selects this lifetime; ordinary callbacks are passed directly. TypeScript uses the equivalent `scope.callable(callback)` with `await using scope`, and Rust uses the typed scope helper plus awaited close. Passing an already scoped ref through another call or projecting another interface does not remove its originating scope restriction. New callbacks produced by a scoped host operation inherit that scope unless you explicitly create an independent registration.

Automatic ownership does not promise immediate collection. Python proxy finalization enqueues release; TypeScript has `using` for deterministic local release and best-effort GC cleanup as a fallback; Rust releases local ownership through `Drop`. The BAML heap may still retain the value until its own GC runs. You do not need Python `__del__` or a custom JS finalizer. Cross-runtime cycles persist until the application breaks an ownership edge or tears down the relevant scope or runtime.

Cancellation stops waiting; it cannot forcibly terminate arbitrary host code. Scope close waits for actual admitted bridge work, including workers launched through the proposed `ctx.run_blocking` helper that outlive their canceled coroutine. Tasks or threads you detach outside the bridge’s tracked call tree remain your responsibility to join and clean up. If that worker never finishes, safe closure may remain pending. A callback must not await the drain of a scope whose active work includes itself: the proposed runtime rejects that self-wait after requesting revocation. An outside owner can await closure.

Your application still owns cleanup of its files, database connections, PIL objects and other resources. Scope close releases bridge references and drains bridge work; it does not infer an application `close()`/`aclose()` contract.

Do not send a live interface or callback ref to another process, runtime or worker. It belongs to its runtime and execution context. Concrete images containing owned encoded bytes are portable media data and can remain readable after that runtime closes; a live PIL-backed `ImageSource` follows the callback/interface lifetime rules instead.

## Media and third-party objects

Concrete BAML `image`, `audio`, `video`, and `pdf` values pass directly to functions and callbacks expecting those types. An image passed to a Python callback arrives as a BAML image value that the callback can inspect or return.

The open `media` interface is a marker. Implementing it identifies an object as media; it does not require `snapshot()`, provide bytes, or promise that a journal or model provider can consume the object.

A PIL image can participate through an adapter implementing a capability such as `ImageSource`:

```python
source = await ImageSourceRef.bind(pil_adapter)

# A function taking ImageSource can receive source directly.
# Request an encoded concrete image only when the next API needs one.
image = await source.to_image()
```

`pil_adapter` implements the generated async `to_image` method. It can encode the PIL image and construct a BAML image from the encoded bytes. The adapter chooses the format, metadata, and whether to capture or track later mutations. Binding or passing the adapter does not run that conversion.

An adapter remains its own concrete type. It does not match a BAML `image` type pattern; the explicit conversion result does. An object may implement `media`, `ImageSource`, both, or neither. A registered adapter for a third-party type makes that integration explicit without requiring you to modify the third-party class.

The same registration and direct-call rules apply in TypeScript and Rust. Use ordinary concrete image data when no live behavior is needed.

### Matching an interface stays open

In BAML, keep `_` when matching an interface, even if you list every implementation you currently know:

```baml
function Describe(value: media) -> string throws never {
    match (value) {
        let i: image => "image",
        let a: audio => "audio",
        _ => "another media implementation",
    }
}
```

Matching does not invoke conversion methods. A concrete union such as `image | audio | video | pdf` can be exhaustive without `_` because it explicitly lists its possible types.

## Containers and annotations

Direct interface arguments require no special cast. Containers obey their host language's typing rules.

For Python and TypeScript helpers accepting mixed implementations, annotate the container using the generated input role:

```python
clients: list[ClientInput] = [openai_client, custom_client]
```

```typescript
const clients: ClientInput[] = [openaiClient, customClient];
```

In Rust, a heterogeneous borrowed input list can use the input trait:

```rust
let clients: Vec<&dyn ClientInput> = vec![&openai_client, &custom_client];
```

An existing `Vec<&ResponsesClient>` is not automatically a `Vec<&dyn ClientInput>`; construct a new container or map its elements. A generated owned collection may use owned refs instead. Input builders retain the references they need before asynchronous work outlives a borrowed collection.

In BAML itself, mutable collections are invariant: an existing `image[]` cannot be assigned to `media[]`. Build a collection whose declared element type is `media`. An individual image still passes directly to a `media` parameter.

## Multiple interfaces, `Self`, and other advanced operations

Most users can skip this section until an interface declaration uses these features.

**Multiple interfaces.** One object can implement several interfaces. Generated BAML objects carry those implementations automatically. For a host object implementing several interfaces, use one host-type registration containing the implementations, then create instances of that registration. Binding the same raw host object separately to unrelated interface registrations does not establish that they are one BAML concrete type.

**Required interfaces.** If an interface requires another, its generated ref can be passed directly at either input type. Its generated host contract includes the required methods and associated-type choices. For example, `StreamingClientRef.bind(implementation, ...)` binds the streaming and required Client implementations together on one object. Registering only the extra streaming method is not enough; implement the full generated host contract. You do not need two separate bind calls.

**Ambiguous method names.** If two interfaces declare the same method name, select the interface explicitly for that call. The advanced `InterfaceRef.view(value)` operation checks an existing BAML object and returns a view exposing the selected interface. For example, Python and TypeScript can use `await LeftRef.view(object)` before calling the ambiguous method; Rust uses `LeftRef::view(&object).await?`. This is useful disambiguation, not a prerequisite for ordinary interface arguments. It does not bind a new host implementation or copy the object.

**Methods involving concrete `Self`.** A method returning `Self` can often be called through an interface, and its result preserves the concrete implementing type. A method taking another `Self`, such as equality between two values of the same concrete type, needs that concrete type to be known. Use the concrete generated object or the generated checked concrete view. Two arbitrary interface refs do not establish that their underlying types match.

**Static interface operations.** A method with no receiver needs a concrete implementor type. Call the generated concrete factory/static method or supply the generated type witness. An interface name alone cannot choose which implementation to construct.

**Runtime-created types.** When a BAML type has no generated Python, TypeScript, or Rust model, use the generated dynamic-call surface and checked dynamic values with their type tokens. The operations remain available even when the host language cannot name the result type statically.

**Automatic or sealed interfaces.** Some interfaces are supplied automatically by BAML or restrict manual implementations. They can still be used where their operations are legal. They do not expose a host binder that bypasses those restrictions.

## Common questions

**Do client overrides or `Agent.run` require learning interface handles?** No. Pass generated objects and call their methods. The SDK manages references.

**Does my Python or TypeScript object work just because it has the right methods?** It needs explicit implementation intent. In Python, inherit the generated implementation base; in TypeScript, use the generated `implement` factory. Both also support explicit binding. Generated BAML objects already carry their implementations.

**Can my Python implementation be a Pydantic model?** Yes: inherit `BaseModel` and the generated implementation base. Pydantic validates your data fields; the SDK manages interface binding and references. Dumping the model's own fields exports configuration, not a live BAML object. An interface-typed field cannot be serialized to JSON as if it contained only that configuration.

**Can I write a custom provider client?** Yes. Implement and bind the generated `ClientHost`, then pass the resulting client just like a generated provider client. Its request, prompt, journal, callback, stream, and typed-error values remain usable through their generated APIs. For streaming, implement the complete `StreamingClientHost` contract; its binder includes the required Client implementation on the same host type.

**Must I implement default methods?** No. BAML executes their declared bodies unless you supply an explicit override.

**Can callbacks receive or return interfaces?** Yes. Use the same generated input/ref roles. Retained refs remain usable after the callback ends, subject to runtime and scope lifetime.

**Can I dump a live interface object to JSON?** No. Export application data explicitly, or store configuration from which you can create a new implementation. There is no universal snapshot method.

**Can I serialize an `Agent.run` result?** Serialize `result.value` when its output type is portable data. The whole result includes a live journal, so it cannot be exported as portable data without explicitly exporting that journal's contents. Having ordinary record fields does not make their live contents serializable.

**Do Python, TypeScript, and Rust implementations run interchangeably?** Each bridge can connect its host implementations to BAML. A ref is still local to the runtime and host context that owns it; interface support does not create a cross-process object service.

**What if generation or a call says my runtime and SDK do not match?** Regenerate with the matching toolchain and update the runtime/SDK together. The redesign uses a coordinated version change; it does not silently reinterpret an older SDK's objects.
