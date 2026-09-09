# Interface codegen: decisions to review before the BEP changes

This is an internal companion to [the design](MEDIA_INTERFACES_AND_BRIDGES_DESIGN.md). It collects the remaining API feedback. **The API examples below are proposed unless explicitly marked verified** in the source audit. The BEP and website remain unchanged.

Implementation update (F52): Python and Node now have generated interface/concrete caller paths, directional input types and checked declaring-view projection. Actual Node probes cover associated pins, non-class receivers, callbacks, generic concrete methods and offline client/Agent factories. Host implementation examples and full client/Agent execution remain proposals. See [the plan](INTERFACES_IMPLEMENTATION_PLAN.md) for exact evidence; the original source audit below is historical.

### Verified Node call shapes (F50–F52)

These are calls against actual generated files, checked by TypeScript and executed through the Node bridge. In the table, `b` is `import * as b from "./generated/baml_sdk/index.js"`. The [checked-in consumers](interface_probes/typescript/README.md#generated-interface-and-concrete-callers-f50) contain the full setup and assertions.

| Expression | Awaited result | What the generated API does |
|---|---|---|
| `b.vendor.openai.ResponsesClient.new_async({ model: "facade-probe", api_key: "unused" })` | `b.vendor.openai.ResponsesClient` | Runs the BAML factory and returns a retained concrete object. No host implementation class is involved. |
| `client.id()` | `string` | Executes the BAML method on that receiver. Its generated signature returns `Promise<string>`. |
| `b.ai.Agent.new_async({ client })` | `b.ai.Agent` | Accepts the concrete client directly through `ClientInput`; no conversion method is required. This verifies construction, not `Agent.run` or network calls. |
| `b.make_text_decoder_async()` | `b.DecoderRef<string, never>` | Fixes Output and Error on the returned view. `decoder.decode(image)` returns `Promise<string>`. |
| `echo.echo("Ada", { $types: { T: BamlType.from("string") } })` | `string` | Supplies a per-invocation method type. The current generated caller requires `$types`; the simpler inference-only proposal later in this document is not implemented yet. |

The generated decoder's `decode` method accepts a trailing optional `$opts` containing `$ctx?: BamlCallContext`. Free functions and factories have that same caller-option mechanism; context is not exclusive to methods. The SDK supplies an entirely different `HostCallContext` when entering a host implementation. None of these calls constructs `GenericCall`.

`throws never` describes the BAML application-error contract. These Promises can still reject on bridge failure, cancellation or a panic. TypeScript's Promise type contains only the success type; the descriptor preserves and validates the separate error contract. F52 connects interface/concrete method type tokens to invariant `BamlType<T>` evidence. Free-function inference and generic record constructor metadata remain separate gates.

### Explicit method choices (F52)

Import `BamlType` from `@boundaryml/baml-bridge`. The generated caller now has this shape:

```ts
echo<T>(value: NoInfer<T>, options: { $types: { T: BamlType<T> } }): Promise<T>;
```

The token chooses the BAML type **and** its native result type. Passing `BamlType.from("string")` returns `Promise<string>`; `42` is rejected as its argument. Even when the input is the literal `"Ada"`, the result is `string`, since BAML may produce another string. Interface associated bindings remain choices of the receiver view; method generics are choices for this call.

Generated nominal factories do not create objects:

```ts
const text = BamlType.from("string");
const payload = b.FacadeRecordType(b.FacadeBoxType(text));
// payload: BamlType<b.FacadeRecord<b.FacadeBox<string>>>
const returned = await box.echo(record, { $types: { U: payload } });
// returned: b.FacadeRecord<b.FacadeBox<string>>
```

A class or enum named `Foo` gets `FooType(...)`; all generic arguments are explicit, in declaration order. If that name is already authored, the generator allocates an underscore suffix. These factories retain their generating SDK, including nested codec checks. Interfaces use `FooRef.type(...)`, which can also select a checked receiver view. Internal builtin carrier declarations do not get misleading nominal factories; their canonical builtin surface is separate.

`NoInfer` is imported through an allocated runtime alias, so an authored class named `NoInfer` or `_BamlNoInfer` cannot shadow it. Type evidence remains invariant after declaration emission. Runtime checks remain necessary for JavaScript callers, BAML int versus float (both native `number`), declared generic bounds and values created dynamically.

This explicit API covers generated interface/concrete methods. Optional free-function `$types`, inferred native literals, generic copied-record `$types`, host implementation registration, and other bridge ports still need corresponding work. See plan F52 for final executed checks.

The next external revision should answer five questions in order: **What can I call? What comes back? How do I supply my own implementation? Which types are fixed? When can a call fail?** Introduce bridge machinery only when an example needs it. The acceptance checklist in §5 separates this proposed API from work verified against generated SDKs.

## 1. Start with using an existing implementation

There are three different generated roles:

| Role | Example | What the user does |
|---|---|---|
| Concrete BAML implementation | `ResponsesClient` | Calls its generated factory, then its methods or a function accepting its interface. The methods execute the existing BAML implementation. |
| Checked interface reference | `ClientRef` | Calls the interface's methods. The reference retains the receiver and its checked implementation information. |
| New host implementation | `ClientHost` plus a generated implementation wrapper/binder | Supplies native method bodies for BAML to call. Merely having matching method names does not register an implementation. |

The separate generated **input role**, such as `ClientInput`, accepts the first two and an explicitly declared host implementation source. Ordinary argument encoding obtains the required interface view. Users need no `.as_client()`.

For example, the proposed Python sequence is:

```python
from baml_sdk.vendor.openai import ResponsesClient
from baml_sdk import extract_resume_async

client = await ResponsesClient.new_async(model="my-model")
resume = await extract_resume_async("Ada's resume", client=client)
```

Here `extract_resume_async` is an illustrative application function returning `Resume`. Its generated client argument is `ClientInput | None` if its elaborated BAML signature takes `Client?`. A streaming companion uses its actual `StreamingClient?` contract. The factory returns a live `ResponsesClient`; extraction returns a native `Resume` record. Neither import asks the user to implement OpenAI behavior.

`Agent.run(spec)` follows the same rule. `FunctionSpec<Out>` determines the call's output type; the agent is not permanently specialized to Out. The awaited result is a copied `RunResult<Out>` containing the native `value`, a retained live `journal`, and copied usage data. See [the guide's complete usage sequence](INTERFACES_USER_GUIDE.md#agentrunspec-call-the-method-normally). No handler object is needed to use the BAML agent.

## 2. Associated types are fixed choices, not method type arguments

Use this small fixture to explain both results and errors:

```baml
class ReadError { message: string }

interface Source {
    type Output
    type Error = never
    function read(self) -> Self.Output throws Self.Error
}

function read_text(source: Source<Output=string, Error=ReadError>)
    -> string throws ReadError {
    source.read()
}
```

For that input, substitute **Output = string, Error = ReadError** before projecting host types. Each call returns a string or throws a ReadError. The user cannot choose a different Output when calling `read()`.

| Language | Ref and input types | Caller signature | Host contract |
|---|---|---|---|
| Python | `SourceRef[str, ReadError]`, `SourceInput[str, ReadError]` | `async def read(self) -> str` | `SourceHost[O, E]`; `async def read(self, *, ctx: HostCallContext | None = None) -> O` |
| TypeScript | `SourceRef<string, ReadError>`, `SourceInput<string, ReadError>` | `read(): Promise<string>` | `SourceHost<O, E>`; `read: (ctx: HostCallContext) => O | Promise<O>` |
| Go | `SourceRef[string, ReadError]`, `SourceInput[string, ReadError]` | `Read(context.Context) (string, error)` | `SourceHost[O, E]`; `Read(context.Context) (O, error)` |
| Rust | `SourceRef<String, ReadError>`, input trait with fixed Output/Error | `async fn read(&self) -> Result<String, Error<ReadError>>` | `SourceHost` with associated Output/Error; boxed-future signature below |

Python and TS raise/reject with an SDK BAML-error wrapper containing the declared ReadError value. Go carries it through `error`. Rust represents it in `Error<ReadError>`. Bridge failures, panics and cancellation remain separate failure categories. **Error is still a fixed associated binding in Python, TS and Go**, even though their native exception/error channels do not enforce it statically.

The generated host shape may have an unused E parameter in Python/TS/Go. That is intentional: matching its methods alone proves nothing about the error binding. The registration descriptor supplies the binding; ref/input types retain it, including an invariant brand where needed. Validate actual thrown values on each invocation. A fabricated annotation or cast cannot change the descriptor.

### Declaring a host implementation

These Python/TS examples show ordinary native bodies and explicit associated choices. `read_text_async` is generated from the BAML function above. Helper export names are proposals to freeze in codegen tests.

```python
from baml_bridge import HostCallContext
from baml_sdk import SourceRef, read_text_async, types

class TextSource:
    async def read(self, *, ctx: HostCallContext | None = None) -> str:
        if ctx is not None:
            ctx.raise_if_cancelled()
        return "Ada"

source = await SourceRef.bind(
    TextSource(), output=types.string, error=types.ReadError,
)
text: str = await source.read()
again: str = await read_text_async(source)
```

```typescript
import { Source, read_text_async, types } from "./baml_sdk/index.js";
import type { ReadError } from "./baml_sdk/index.js";

const source = Source.implement<string, ReadError>({
  read() { return "Ada"; },
}, { output: types.string, error: types.ReadError });

const text: string = await source.read();
const again: string = await read_text_async(source);
```

This implementation happens not to throw, but declares ReadError as its allowed error contract. Omitting Error would select the exact default `never`; it would not mean “infer any error this body might throw.” A `Source<Error=never>` is not repinned to `Source<Error=ReadError>` merely because its body fits both method signatures.

Go uses `SourceTypes[O, E]` with `Output TypeToken[O]` and `Error TypeToken[E]`. Its binder checks both tokens and returns `(SourceRef[O, E], error)`. `any` as a Go generic constraint does not mean every Go type has a BAML codec. A wrong native token type should fail Go compilation; an unsupported codec or forged runtime descriptor must fail binding.

Rust can derive statically known bindings from its implementation trait:

```rust
pub trait SourceHost: Send + Sync + 'static {
    type Output: BamlValue + Send + Sync + 'static;
    type Error: BamlValue + Send + Sync + 'static;

    fn read<'a>(
        &'a self, ctx: HostCallContext,
    ) -> BoxFuture<'a, Result<Self::Output, Error<Self::Error>>>;
}

// Given H: SourceHost<Output = String, Error = ReadError>:
// Source::implement(H) -> SourceImplementation<String, ReadError>
// source.read().await -> Result<String, Error<ReadError>>
```

`BoxFuture<'a, T>` means `Pin<Box<dyn Future<Output = T> + Send + 'a>>`. The lifetime allows the future to borrow the retained receiver. It is not a type-generic virtual method. Callers on the generated ref supply no HostCallContext.

For `never`, target the supported compiler: the pinned Rust 1.93 probe rejects `Error<!>` in generic position, so the current target uses `Error<Infallible>`. Native `!` is the preferred spelling after a verified minimum-toolchain change. That choice does not alter BAML semantics.

### Three type choices users should not confuse

| Choice | Who fixes it? | What callers need to know |
|---|---|---|
| Associated `Source.Output` | The implementation, checked when obtaining a `Source` view | `SourceRef[str, ReadError].read()` always produces `str`. It is not a generic method. |
| Method parameter in `echo<T>(value: T) -> T` | Each invocation, by inference or explicit checked type evidence | The same receiver can echo a string and then an integer. The generated caller must connect that invocation's argument and result types. |
| An unspecified associated type in a required-interface bound | The implementation knows it, but the current view may not expose it | Obtain a checked view with the missing bindings before using a caller whose native signature needs those bindings. A guess, cast, or declaration default is not evidence. |

**Associated types alone do not require `GenericCall`.** A host `Source` with Output fixed to string implements an ordinary string-returning `read` method. Reserve the advanced handler explanation for contracts whose ordinary native adapter cannot preserve the type relationship. Calling a BAML implementation and implementing that same interface in the host are separate codegen decisions.

The fully specified `Source` example therefore needs no language-specific fallback: Python and TS express its fixed bindings as generic parameters; Go does so on its generated types; Rust can use associated types on its host trait and generic parameters on its checked ref. These are different native spellings of the same BAML contract, not four different interface semantics.

For an unspecified required binding, Python uses `selected = await value.as_interface(SourceRef[str, ReadError])`. A successful projection returns that exact ref type and retains the same receiver. A wrong choice fails before calling `read`; it must not reinterpret or replace the implementation's bindings. This is an advanced recovery path, **not** an extra step for ordinary generated concrete inputs, client overrides, or an already fully specified required interface. F44 verifies this path in Python; F51 adds the corresponding generated Node path below. Other bridges remain unverified.

### Selecting a checked view in TypeScript (verified in F51)

For the `RequiredBase<T>` probe, Output and Error are associated types. The generated factory's signature is:

```typescript
static type<T, Output, Error>(
  argument: BamlType<T>, output: BamlType<Output>, error: BamlType<Error>,
): BamlInterfaceType<RequiredBaseRef<T, Output, Error>>;
```

The parameter names above are explanatory; generated private parameter names may differ. Ordinary arguments come first, then associated bindings in declaration order. Every argument is explicit on this type selector. Calling a factory that already returns a fully specified interface still requires no selector.

```typescript
import { BamlType, Never } from "@boundaryml/baml-bridge";
import { RequiredBaseRef, required_unpinned_async } from "./baml_sdk/index.js";

const value = await required_unpinned_async();
const text = BamlType.from("string");
const selected = await value.as_interface(
  RequiredBaseRef.type(text, text, BamlType.from(Never)),
);
const result: string = await selected.apply("Ada");
selected.close();
value.close();
```

`Ref.type(...)` constructs a type token; it neither constructs nor registers a receiver. `as_interface` checks that token against the existing receiver and returns an independently retained view. Wrong Output/Error choices reject; they do not change the object. A pending projection survives local close just like a pending method call. The check and its decoding use the receiver's original SDK, including generated constructors nested inside type arguments.

`BamlType<T>` keeps T invariant in emitted declarations. Primitive tokens, typed array/optional composition and ordinary generated record constructors have verified paths. An opaque reflected schema is `BamlType<unknown>` and cannot be assigned to `BamlType<string>` as static proof. F52 adds SDK-bound nominal class/enum factories and typed interface/concrete method choices; see the walkthrough below. General free-function inference and nested root-scoped conformance definitions remain gates. TypeScript also represents both BAML int and float as number; the wire token and runtime checks retain the distinction the native annotation cannot express.

### What must generation preserve?

1. Keep interface parameters, associated bindings, and method parameters in separate semantic slots. Emit native ref/input parameters in interface-parameter order followed by associated-declaration order. Preserve declared variance for ordinary parameters; associated pins remain exact/invariant.
2. Resolve defaults and bounds with the compiler's rules. A default referring to another parameter is substituted, not copied as an unresolved native name. Preserve required-interface bindings too.
3. Substitute associated uses recursively: return values, arguments, nested records/unions, callbacks and declared errors. An associated type appearing only in `throws` must not disappear.
4. Preserve symbolic projections when their owner is generic. Do not turn `T.Output` into `unknown` because a concrete owner is unavailable at generation time.
5. Generate separate typed input acceptance and output decoding. A returned ref is already checked; a new host implementation must first register. Native inheritance or structural matching never supplies the runtime witness.

A runtime-created BAML type may share the same native `DynamicValue` representation with many other BAML types. **Its native wrapper type cannot uniquely determine its BAML type.** The Rust static derivation above therefore applies only where sealed codec metadata identifies one exact BAML type. Dynamic registration requires a checked per-instance descriptor and codec, via the explicit dynamic binding/handler path. Python/TS/Go tokens have the same requirement: names or native constructors are not sufficient evidence. Final dynamic binder names remain an implementation gate.

### A returned implementation needs no binding

Extend the same BAML fixture with an implementation and a factory:

```baml
class TextSource {
    text: string,
    implements Source {
        type Output = string
        type Error = ReadError
        function read(self) -> string throws ReadError { self.text }
    }
}

function make_text_source(text: string)
    -> Source<Output=string, Error=ReadError> throws never {
    TextSource { text: text }
}
```

These are the corresponding proposed caller signatures, with ownership helpers omitted:

```python
async def make_text_source_async(text: str) -> SourceRef[str, ReadError]: ...
async def read_text_async(source: SourceInput[str, ReadError]) -> str: ...
```

```typescript
declare function make_text_source_async(text: string): Promise<SourceRef<string, ReadError>>;
declare function read_text_async(source: SourceInput<string, ReadError>): Promise<string>;
```

```go
func MakeTextSource(ctx context.Context, text string) (SourceRef[string, ReadError], error)
func ReadText(ctx context.Context, source SourceInput[string, ReadError]) (string, error)
```

```rust
pub async fn make_text_source_async(text: String)
    -> Result<SourceRef<String, ReadError>, Error<Infallible>>;
pub async fn read_text_async(source: &impl SourceInput<Output = String, Error = ReadError>)
    -> Result<String, Error<ReadError>>;
```

Rust's `SourceInput` uses associated types; its returned `SourceRef` uses native generic parameters. These express the same fixed BAML bindings. `Error<Infallible>` on the factory means it has no declared application error; it can still fail at the bridge. The `ReadError` inside its returned ref instead describes **future calls to `read()`**. Do not merge those two error contracts.

For Python, the complete invocation is `source = await make_text_source_async("Ada")`, then `text = await source.read()` or `text = await read_text_async(source)`. Both yield `str`. Nothing is registered as a host implementation: the receiver stays in BAML. TS, Go and Rust follow the same sequence with the return conventions above.

The factory has intentionally returned an interface. A factory returning concrete `TextSource` instead produces its generated concrete facade, which also satisfies `SourceInput[str, ReadError]`. The input codec projects that existing receiver; it does not copy its fields or choose new associated types.

### Required interfaces preserve bindings, not just method names

The stdlib's `Iterator` requires `Iterable<Item=Self.Item, Error=Self.Error>`. Therefore an `IteratorRef<string, ReadError>` is accepted at an `IterableInput<string, ReadError>` argument. It is **not** accepted at `IterableInput<string, never>`. The default `Error=never` only fills an omitted declaration binding; it cannot replace an existing receiver's Error when obtaining another interface view.

Here are the proposed caller signatures for that same iterator. These are declaration excerpts using the SDK types introduced above, not standalone generated files:

| Language | Create an iterator with `iter()` | Advance it with `next()` |
|---|---|---|
| Python | `async def iter(self) -> IteratorRef[str, ReadError]` | `async def next(self) -> str \| Done` |
| TypeScript | `iter(): Promise<IteratorRef<string, ReadError>>` | `next(): Promise<string \| Done>` |
| Go | `Iter(context.Context) (IteratorRef[string, ReadError], error)` | `Next(context.Context) (IteratorNext[string], error)` |
| Rust | `async fn iter(&self) -> Result<IteratorRef<String, ReadError>, Error<Infallible>>` | `async fn next(&self) -> Result<IteratorNext<String>, Error<ReadError>>` |

`IteratorNext<T>` is the generated representation of BAML's `T | Done` union in Go/Rust, as specified in design §9.2.1. The `iter()` call has `throws never`; its returned iterator's later `next()` calls may throw ReadError. Python/TS/Go retain that distinction in checked method metadata even when the native signature cannot express both error contracts. Infrastructure failures remain possible on either call.

Codegen must substitute the complete `requires` expression and nested return bindings. Do not copy parameters by coincidentally matching their names or apply the required interface's defaults again. A host registration that supplies Iterator must also satisfy its required Iterable contract before registration becomes visible; it need not ask the user to register the same receiver twice. The compiler's actual default bodies determine which methods the user must supply.

There are two separate promises here:

| User action | What generation and dispatch must provide |
|---|---|
| Pass an `IteratorRef` to an `IterableInput` parameter | Evidence for that exact required interface, including Item and Error. Preserve this evidence even through a parameter already annotated `IteratorInput`. |
| Call `await iterator.iter()` directly | The inherited caller method, its substituted result/error types, and dispatch to the compiler-selected declaring interface on the same receiver. No explicit conversion is needed. |

Requirements can be transitive. For example, `Root<T>` requiring `Middle<Value=T, Failure=never>`, which requires `Base<string, Output=Self.Value, Error=Self.Failure>`, proves `Base<string, Output=T, Error=never>`. The shared compiler export must resolve the intermediate projections; individual generators must not invent their own associated-type solver. Conversely, `requires Base<string>` without associated pins is only a bound. It does not prove a particular Output or replace an unspecified Error with `never`, even if Base declares that default. A checked specialization may obtain more evidence from the actual receiver later.

Inherited method lookup must follow BAML's rules too: an own declaration shadows its requirement closure; reaching the same realized declaring interface by two paths is one candidate; distinct surviving candidates can be ambiguous. Do not choose the first method with a matching name. Preserve the compiler's ambiguity and concrete-Self restrictions, with an explicit checked qualification path where available. Obtaining a required view neither creates another host registration nor copies receiver state; every view and admitted call retains the existing receiver through ordinary reference ownership.

**Current evidence (F43–F44):** the compiler exports resolved caller contracts separately from own implementation obligations, and emission bakes the selected declaring views into runtime dispatch metadata. The shared engine and generated Python execute inherited calls, including `iterator.iter()`, transitive associated results and inherited generic methods. Compiler/engine checks cover diamonds, shadowing, ambiguity and retained receivers; native typing rejects an Error-only mismatch. F44 repairs generation of the unpinned `RequiredUnpinned.apply` case by documenting an explicit checked declaring-view projection and exposing the ordinary method on that fully specified ref. The full fixture generates and its native projection executes with exact static types. This does not supply a fallback for arbitrary dependent projections or establish language parity; see plan F44 for exact evidence and limits.

## 3. Context and GenericCall are different things

| Value | Who supplies it? | Purpose |
|---|---|---|
| Caller options / Go `context.Context` | Caller, where wanted or required by the native API | Control that invocation. They do not move a reference into another runtime. |
| `HostCallContext` | SDK while entering a host body | Cancellation and tracking work performed for the invocation. No ownership scope is created by accepting it. |
| `GenericCall` | SDK while entering an explicitly registered advanced handler | Checked method type arguments, arguments, bounds, result/error contracts and invocation context. |
| `BridgeScope` | Application, optionally | Groups registrations for revocation and draining. Ordinary calls use automatic ownership without one. |

For Python ordinary implementations, use the optional keyword-only context shown above. The SDK always supplies it; a direct local call may omit it. Do not guess whether to inject context by counting arguments or catching a TypeError from user code. Generation records the adapter signature. If an authored argument already uses that native name, select and document a collision-free generated context name; do not rename the authored wire argument or overwrite it. The precise naming policy still needs a compile-checked collision fixture.

TS context is the adapter's trailing argument; a method that ignores it can omit the parameter in its implementation. Rust's explicit host trait requires it. Those implementation choices do not add context parameters to ordinary Python/TS/Rust ref calls or require local calls to go through the SDK.

**Context belongs to an invocation, not to methods specifically.** Apply the same incoming-context policy to generated host-callable adapters and interface methods. A callback that needs cancellation or tracked workers needs access to that invocation's context too. Preserve the convenience of ordinary context-free callbacks through an explicit adapter mode; never inspect a body by trial-calling it. Existing callback APIs lacking context are implementation work, not a reason to invent different lifetime rules for methods. Free BAML functions and methods use the same native caller-options convention; Go uses its ordinary caller-supplied `context.Context` for both.

Introduce the advanced types before the first handler example. The proposed minimal TypeScript signatures are:

```typescript
interface GenericCall {
  argument(name: string): DynamicValue;
  return_value(value: DynamicValue): HostOutcome;
}
type EchoHandler = (call: GenericCall) => HostOutcome | Promise<HostOutcome>;
```

`DynamicValue` is a checked value with its exact BAML type and owned nested references. `HostOutcome` is an SDK-created completion bound to this invocation. These signature excerpts omit other operations; users cannot manufacture either by constructing a matching object. `return_value` checks the result type; engine adoption checks it again. A completion for another invocation or a duplicate/late completion must be rejected and its transferred ownership released.

`Echo.bind_handlers({ echo: handler })` returns `EchoRef`. Calling `await echo.echo("Ada")` on that ref invokes the handler with an SDK-created `call: GenericCall`; the caller receives a string. An ordinary `EchoHost.echo(value)` never secretly changes to accept GenericCall. A method with its own T does not automatically require this handler path; adapter eligibility depends on whether the backend can preserve the full contract.

For publication, put the declaration and invocation beside that explanation:

```baml
interface Echo {
    function echo<T>(self, value: T) -> T throws never
}
```

```typescript
import type { GenericCall, HostOutcome } from "@boundaryml/baml-bridge";
import { Echo } from "./baml_sdk/index.js";

const handler = (call: GenericCall): HostOutcome => {
  const value = call.argument("value"); // DynamicValue, retaining exact BAML T
  return call.return_value(value);     // Completion for this invocation only
};
const echo = await Echo.bind_handlers({ echo: handler }); // EchoRef
const text: string = await echo.echo("Ada");
```

The proposed caller method is `echo<T>(value: T): Promise<T>` for values with supported codecs; the handler is `EchoHandler`, defined above. TypeScript's T alone cannot prove codec support, so encoding checks it before invoking the handler. This simple string call can infer its BAML type. Ambiguous or runtime-created types need checked type evidence through the generated specialization API, whose exact spelling is still an open gate. Callers do not construct `GenericCall` in either case.

Do not label `handler` an `EchoHost`, and do not demonstrate `handler("Ada")`. The distinct binding API and declared argument type make that a type error. If the user instead supplies an ordinary `EchoHost` implementation, its ordinary method remains locally callable; the checked ref is a separate object that invokes it through the SDK.

## 4. Arrays and strings are implementors too

The [actual stdlib](crates/baml_builtins2/baml_std/baml/ns_iter/iter.baml) implements **Iterable** for `T[]` and `string`. Calling `iter()` produces an **Iterator**. Do not present a native array as though it already implements the complete BAML Iterator contract.

| Crossing | Required behavior | Earliest failure if invalid |
|---|---|---|
| BAML `string[]` returned as `string[]` | Copy its data to the native collection type. | Data conversion. |
| BAML `string[]` returned as `Iterable<Item=string, Error=never>` | Retain that array behind a checked interface ref; dispatch its existing implementation. | BAML checking for a static mismatch; checked projection for dynamic input. |
| Empty native list passed to a BAML function taking `string[]` and returning that Iterable | The declared array input determines its type. Copy into BAML, then project. | Argument encoding, before the BAML function body. |
| Native list passed directly to an Iterable parameter | Require an explicit host adapter or the typed-data factory above. No implicit native-iteration registration in this PR. | Native checking where possible; otherwise argument encoding before execution. |
| Ref with wrong associated pins or a same-named declaration from another compilation | Reject it using the retained exact declaration and pins. | Argument encoding/admission before the receiving body runs. |
| Host `next()` returns the wrong Item or throws the wrong Error | Reject completion as a host contract violation. | Completion before delivery; binding cannot predict arbitrary future host behavior. |

This policy supports non-class implementations without sharing a native list's mutable storage with BAML. Calling methods can update iterator state in its owning language. Editing a copied native list does not update the retained BAML array.

An interface pin answers “which implementations are acceptable?”, not “which concrete value should the SDK create?”. `Iterable<Item=string>` can describe an array, a string, or another implementing type. Even a nonempty native list must follow the explicit conversion rule above: its current elements are not a declaration of its BAML concrete type. The typed-data factory selects `string[]` unambiguously. An explicit host adapter instead keeps its receiver in the host language and registers that adapter's implementation; it does not install a blanket implementation for every native list or replace BAML's existing array implementation.

`Iterator.next()` really returns `Item | Done`. Done differs from null, so nullable items remain usable. Do not add a new wire “item versus end” discriminant that BAML did not declare. If Item itself includes Done, the source contract already permits overlap; a native union wrapper cannot invent a distinction. Include that case in union-projection review.

Use an authored factory to make the native-list case concrete:

```baml
function text_items(values: string[])
    -> baml.iter.Iterable<Item=string, Error=never> throws never {
    values
}
```

`items = await text_items_async([])` in Python has the proposed type `IterableRef[str, Never]`, where `Never` is `typing.Never`. The empty list is copied with element type `string` from the parameter. `iterator = await items.iter()` returns `IteratorRef[str, Never]`; `await iterator.next()` returns `str | Done`, including Done immediately for this empty input. The SDK owns the returned refs automatically. Editing the original list later does not edit the BAML array.

There is one further boundary that applies to **all** implementors. An interface method `combine(self, other: Self)` does not become `combine(other: InterfaceRef)` just because both inputs implement the same interface. They may have different concrete types. Preserve [TYPE_SYSTEM.md's concrete-Self rule](TYPE_SYSTEM.md#self): use a checked concrete view/type witness for such operations; do not expose an unsafe existential overload. Likewise, a receiverless operation needs a concrete type witness. These are BAML callability constraints, independent of whether Python, TS, Go or Rust can express a method signature. Codegen should expose the supported concrete path and explain the restriction, rather than drop the whole interface or defer a statically known mistake until a host body runs.

### What can we check before a method runs?

Separate three questions: **Can the SDK represent this declaration? Does this particular receiver implement it? Does this invocation obey it?** Successful generation answers only the first. Successful binding cannot prove what arbitrary Python or JavaScript code will return tomorrow.

| Stage | Example | Required result |
|---|---|---|
| BAML compilation | Two overlapping implementations, missing required associated pins, or an invalid concrete-Self call. | Report the source error. Do not change the language rules to fit a bridge. |
| SDK generation | A legal member has no ordinary host-method projection in a backend. | Generate its supported caller and separately typed handler path. If no checked projection exists, report the exact unsupported member and role; do not silently omit it. |
| Native type checking | A `SourceRef[str, ReadError]` is supplied where `SourceInput[int, ReadError]` is required. | Reject when expressible. Python users must run a type checker to receive this early check. Casts, unchecked code and deserialized inputs still need runtime checking. |
| Binding a host receiver | The declared Output token has no codec, or a required implementation is missing. | Reject before making the registration usable; release provisional ownership. Matching method names is insufficient. |
| Admitting a call | A dynamic input has the wrong associated pins, wrong runtime identity, or an unadapted native list. | Reject before the receiving function or method body runs. Keep already valid refs usable. |
| Completing a host call | A string-producing host method returns an integer or throws an undeclared value. | Report a host contract violation before delivering the result to BAML; release rejected result ownership. The host body has already run, so its side effects are not rolled back. |

For example, binding `TextSource` with Output=string validates its declared adapter and codecs. It does not execute `read()` as a trial call. Each later bridge invocation checks the returned value. Likewise, an ordinary local `await implementation.read()` runs Python directly: BAML's completion checks apply when the method is invoked through the bridge.

The associated Error parameter needs particular attention in codegen tests. `SourceHost<O, E>` may have the same ordinary method shape for several E choices in Python, TS or Go. That shape is an implementation-body contract, not a checked interface identity. The generated ref/input representation must preserve exact E even when E appears only in metadata. Test a mismatch in **Error alone**, with Output unchanged; testing only string-versus-integer output would miss this bug. Do not add a fake user method just to make E appear in the implementation's signature.

### What does “implementing the interface” promise?

There is one BAML method contract. For `Source<Output=string, Error=ReadError>`, it is `read() -> string throws ReadError`. A Python annotation, TypeScript method shape or Go `error` return is not a second declaration of that BAML contract.

Explain a host implementation in four steps:

1. **Write the native body.** Its ordinary method returns a string. A native type checker can catch an incompatible return annotation where supported. Associated Output/Error choices alone do not turn this method into a `GenericCall` handler.
2. **Declare the implementation and its types.** The generated wrapper or binder selects Source, Output=string and Error=ReadError. The SDK validates the declaration, codecs, required methods and supported calling convention. Registration does not call `read()` to see what it does.
3. **Pass it to BAML.** The SDK obtains a checked view for that exact contract and retains the receiver. An existing checked ref or BAML-defined implementation already has implementation evidence; it does not register a new host type at every argument crossing.
4. **Invoke `read()`.** The SDK supplies the native invocation context and checks the returned or thrown value against the BAML contract. If an unchecked body returns an integer, the call fails as a host contract violation. The body has already run, so its state changes cannot be undone by rejecting its return value.

The generated native contract may permit a body accepting broader inputs or returning narrower values. That does not change the interface view's declared types or associated bindings. In particular, a body that never throws can implement a Source whose allowed Error is ReadError; this does not make an existing `Source<Error=never>` view interchangeable with it. Associated pins are exact.

**Current evidence:** F57 implements declaration-based registration in the VM and its final 12-test selection passes. It checks declarations, required/default methods, pins and coherence, and retains every instance's original receiver, including markers. Native Host compatibility tests, binders, advanced frame transport and receiverless host methods remain open. Do not describe those SDK APIs as available based on this VM result.

### Does an array have to become a host implementation?

No. A BAML array returned as an interface already has its concrete BAML type and compiler-defined implementation. Its SDK interface ref retains that value and calls the selected BAML methods; it needs neither a generated array class nor host method registration.

A native Python list, JS array, Rust Vec or Go slice is a different input case. A concrete BAML array parameter can request a checked copy with a known element type. An interface parameter alone does not authorize guessing an implementation from the native container's shape. A supported explicit conversion must fix the concrete BAML type and check its implementation, including associated pins, before admitting the call. This also gives an empty array an element type without inspecting nonexistent elements. See §4 for the non-class rules and failure stages.

The review tests must cover both paths separately: an existing non-class BAML receiver works through its ref; an undeclared native adapter is rejected before the receiving body runs. Neither failure should be deferred to an unrelated method call or blamed on BAML's legal non-class implementation.

## 5. Evidence required before external publication

Compile complete examples against generated files, then run the same portable SDK cases through each actual bridge. Handwritten signature checks prove only language feasibility. The shared test-name parity check proves coverage declarations, not native execution.

The focused acceptance set is: exact Output/Error pins; a default Error=never; nested associated projections; a runtime-created type; direct generated-provider inputs; Agent output inference; required/default/generic methods; array/string receivers; empty and nullable collections; wrong-pin rejection before state changes; invalid host completion; and retention/release while the runtime remains open. Add host context-name collisions and separate ordinary/advanced handler types to native compile checks. Keep the existing broader lifetime/race gates in [the plan](INTERFACES_IMPLEMENTATION_PLAN.md).

Open gates are the final native generic-adapter eligibility table, dynamic binder spelling, context collision naming, complete generated snippets and actual native execution. This note settles how to explain and evaluate them; it does not mark those gates complete.

For each published example, require one reviewable bundle: the BAML fixture; generated imports and signatures; native construction and invocation; the success type and error channel; and a matching executable SDK case. Include expected compile failures separately from executable fixtures. In particular, check wrong associated pins, unsupported native values, and concrete-Self misuse without preventing valid fixtures from generating. A legal BAML interface with a backend limitation must have a documented checked fallback or an explicit generation diagnostic, never a silently missing method or an `any` substitute.

Record caller support and host-implementation support separately for each member. A backend may be able to call a BAML generic method while requiring an advanced handler to implement it. Likewise, a class-only generator limitation must not reject a valid array implementation in the language checker. The backend review table must name the member, its exact caller signature, its ordinary host signature or handler type, and the earliest rejection point for each unsupported path. Keep entries unverified until the generated positive and negative examples actually compile; bridge execution is a separate gate.

### Review packets for the next BEP revision

Organize the external walkthrough by task. For each row below, put the generated signature immediately before the call and label the resulting value immediately after it. Introduce every helper type before its first use. This table is a documentation and verification queue, not a support claim.

| Task | Example to show | Evidence required |
|---|---|---|
| Use an existing BAML implementation | Import `ResponsesClient`, call its real factory, then pass it to an LLM client override | Actual generated facade and factory return type; accepted input type; native output record; offline invocation with a stub provider. Construction alone does not verify an override. |
| Call a stateful BAML object | Create one Agent, call `run` with two differently typed specs | Both exact `RunResult` types, journal ownership and failure channel; method execution, not only type inference. |
| Use an interface returned by BAML | `make_text_source` followed by `source.read()` | The factory's error contract separately from the returned ref's method error contract, in Python, TS, Go and Rust. |
| Create a host implementation | Implement ordinary `Source.read`, declare Output/Error, pass it into BAML | Generated host contract, binding/factory return type, SDK-supplied context, and an actual BAML-to-host method call. Show the ordinary local call separately if useful. |
| Implement an advanced generic operation | Register the separate Echo handler, then call the checked Echo ref | Imports and definitions of `GenericCall`, `DynamicValue` and `HostOutcome`; the handler's exact signature; ordinary caller's exact result; wrong completion rejection. |
| Use a non-class BAML receiver | Return an empty `string[]` as Iterable, call `iter`, then `next` | Checked ref types and `Done` result, including nullable elements; no generated class requirement and no implicit native-list registration. |

Use Python and TS first in the external explanation, then a compact Go/Rust comparison. This is reading order, not a reduction of the all-bridge implementation scope. Each backend needs its own checked examples; a Rust trait prototype or a TS declaration check does not establish that the generated SDK executes correctly.

Keep three results separate in the plan: **generated**, **native type-checked**, and **executed through the bridge**. Pair each positive example with the meaningful negative case: a mismatch in Error alone; a missing associated binding; incompatible method type evidence; or a native collection without a declared conversion. When runtime rejection is expected, assert that the receiving body has not run. For a rejected host completion, assert cleanup instead: that body has already run, and the SDK cannot undo its side effects.

## 6. How the generator gets from BAML to those signatures

The implementation should follow one sequence shared by the backends. Language-specific syntax comes last:

1. **Read the checked declaration and implementation rules.** Include required interfaces, default bodies and implementations outside classes. A class's inherent method list is not the list of methods it implements. Keep the declaration's caller contract separately from a particular implementation's method signature.
2. **Build a substitution environment.** Identify parameters by declaration and slot, not their printed names. Keep interface parameters, associated bindings, concrete Self and method parameters separate. Resolve associated defaults and required-interface bindings with compiler rules. Never sort associated names to determine native generic argument order.
3. **Project a member for its role.** Substitute `Self.Output` with the interface's Output binding; on an unspecialized `SourceRef[O, E]`, that is O, while on `SourceRef[str, ReadError]` it is str. Keep method-owned parameters generic. A projection such as `T.Output` belonging to a still-generic T must remain symbolic or use the checked specialization/handler path. It is neither an error-recovery type nor `Any`.
4. **Choose the native crossing direction.** Values supplied to BAML use input acceptance; values delivered to native code use decoded references. Apply this inside callbacks and nested values, as described below.
5. **Emit declarations and executable codecs together.** Allocate collision-free names for token, input, ref, host contract and implementation facade. Emit imports, descriptors, method dispatch and decoding registration from the same semantic plan. A type annotation alone does not make an interface callable.

Concrete-Self restrictions are checked before an existential caller method is emitted. Non-class implementors go through the same implementation-rule lookup as classes. Native limitations are reported against the affected member and role; they must not invalidate a legal BAML implementation.

### Input and output depend on which way the value crosses

`SourceInput` is the set of things the SDK can accept and project. `SourceRef` is what the SDK hands back after checking. This distinction also applies when BAML calls a Python/TS/Rust/Go implementation:

| Position containing `Source<Output=string, Error=ReadError>` | Native projection |
|---|---|
| Argument supplied by a native caller to BAML | `SourceInput[str, ReadError]` in Python; corresponding input role in the other languages. |
| BAML result delivered to a native caller | `SourceRef[str, ReadError]`. |
| Argument delivered by BAML to an ordinary host method | `SourceRef[str, ReadError]`; the host body can call `read()` immediately. |
| Result supplied by an ordinary host method to BAML | The matching input role; an already checked ref, generated concrete implementation, or explicitly declared host source can be returned. Validate/project it as part of completion. |

The last row does not accept arbitrary method-shaped objects. It uses the same nominal registration and exact-pin checks as an ordinary argument, with provisional registrations cleaned up if completion is rejected.

For example, if BAML declares `function visit(source: Source<Output=string, Error=ReadError>) -> string`, the Python caller accepts a `SourceInput[str, ReadError]`. If the same function shape is implemented as a Python callback, the callback receives a `SourceRef[str, ReadError]`. Reusing the caller's argument annotation for the host callback would lose the guarantee that `source.read()` is available.

Build this recursively for callable arguments/results and records containing interfaces. Do not achieve it by casting a mutable `list[SourceRef[...]]` or a generic record to a wider input container. Generate direction-aware codecs and the required native input shapes, respecting each language's container rules. This changes neither BAML variance nor the agreed copied-container ownership boundary.

### What the current source proves, and what it does not

This source audit includes F30's generated Python callers, F31–F39's concrete facade work, F40's initial Python input roles, F41's copied-record admission repair, F42's required-input evidence, and F43's inherited caller generation/dispatch. Direct Python record/live inputs and pinned inherited callers have execution evidence; unpinned specialization, recursive input projection and remaining host/backend roles are incomplete.

| Layer | Present behavior | Required next change |
|---|---|---|
| [Shared interface graph](crates/baml_codegen_types/src/interfaces.rs) and [export](crates/baml_ide/src/symbol_pool/interface_export.rs) | Preserve symbolic RuntimeTy, associated declaration order, required/default methods, original generic slots, non-class receiver patterns and compiler-derived existential callability. | Finish the role/substitution plan across backends; retain useful concrete-Self restriction details for diagnostics. |
| [Ordinary symbol conversion](crates/baml_ide/src/symbol_pool.rs) | `convert_tir_leaf` maps associated projections to Unknown alongside recovery errors. LLM argument collection strips the injected client override. | Preserve legal symbolic projections across free functions and class members too; restore client arguments once their input codecs exist. The new interface graph alone does not repair this path. |
| [Class projection](crates/baml_compiler2_hir_ty/src/class_projection.rs) | The defining package selects Record, Live or Builtin once; bytecode and SymbolPool retain the same metadata, including source-less packages and runtime mount stubs. Downstream extensions retain interface dispatch without changing a dependency's concrete codec. | The engine and Python generator now consume this decision for retained concrete objects and live facades. Complete the other generators and native input roles; the flag alone does not establish their public API. |
| [Python type translation](sdks/python/rust/sdkgen_python_pydantic2/src/translate_ty.rs) and [caller emission](sdks/python/rust/sdkgen_python_pydantic2/src/interface_refs.rs) | Emit generic Ref classes, ordered invariant associated parameters, async required/default methods and decoder registration. F40 adds direct-head/union Input roles and private evidence for multiple views. Explicit generic method choices use `_types`; tokens preserve ordinary arguments separately from associated pins. `never` projects to `typing.NoReturn`. | F42 adds normalized transitive required-input evidence; F43 emits compiler-resolved inherited callers and prepares calls before returning an awaitable. F44 verifies explicit declaring-view specialization for an unpinned requirement. Complete recursive input/host roles and general symbolic projections; F41 verifies direct copied-record admission. Other unsupported `T.Output` projections still fail generation with a member path; they need their own checked native API. Concrete-Self and receiverless methods need their complete concrete API. |
| [Python result decoding](sdks/python/src/baml_bridge/proto.py) | Resolves generated Ref subclasses and invokes methods on their retained issuing runtime. Pydantic records accept already decoded live children. | F34 checks generated-facade selection against the loaded SDK bundle using native declaration evidence. F35 captures the generated call's runtime and codecs; F36–F38 add checked concrete method dispatch. F39 emits concrete facade methods and registers their entries. Generic live class tokens and generic record annotations preserve those native types. Audit issuer/codec propagation through every callback/error/stream path separately. |

The generated shared Python fixture passes all eight runtime cases, and its concrete input case now receives an actual generated `FriendlyGreeter` with callable `greet`/`label` methods. F39's generic concrete typing consumer has zero diagnostics; its runtime probe verifies generic/default/inherent methods, owner-side mutation, concrete Self, retained calls and copied generic records with live children. It also executes the real `ResponsesClient` factory/id and passes the client directly to `Agent.new_async`, without a network call or a provider-specific adapter.

After F40's initial input-role emission, `interface_probes/python/generated_concrete_callers.py` reports zero Pyright diagnostics: the generated argument is now `GreeterInput`, and `FriendlyGreeter` satisfies it. The new generated positive input consumer also passes; its negative companion rejects all five marked invalid inputs, including a mismatch in Error alone. F41 repairs the copied-record runtime rejection discovered in F40. The generated input probe and six shared Python cases now pass, including a record copied into BAML and retained as a marker interface. These results do not establish complete recursive input support, host binding, `Agent.run(spec)` inference, LLM function override arguments, streaming, configuration construction or all-bridge parity. Source-only SDKs also need checked declaration identity before they can emit usable live concrete facades.

### Input evidence must represent more than one interface instantiation

A concrete BAML type can implement different instantiations of the same interface. For example, it may satisfy both `Convert<int>` and `Convert<string>`. Do not encode this by inheriting the same Python generic base twice with different arguments: native inheritance is not the BAML implementation relation. Likewise, do not infer associated Error from the native exception signature, or assume a concrete method must have the interface's required parameter names.

F40 probes an invariant private witness type with per-view overloads against actual generated files. `DualMapper` implements both `Mapper<int, Error=string>` and `Mapper<string, Error=never>`. Python accepts that same generated object at both inputs and rejects a ref with the wrong Error or item type. The methods run successfully through both interface inputs. Its concrete facade has no unqualified `map` method because that name is ambiguous; obtaining either interface view at a typed argument still works.

The public caller signatures in that fixture are:

```python
async def use_int_mapper_async(value: MapperInput_[int, str]) -> int: ...
async def use_text_mapper_async(value: MapperInput_[str, typing.NoReturn]) -> str: ...
```

The trailing underscore is intentional: this fixture also declares a BAML class named `MapperInput`, so the generated role gets a collision-free name. The private witness method is generated machinery, not a method users implement or call. It lets a native checker distinguish complete interface views without imposing a public method signature on the concrete facade. The associated pins remain invariant even when a method's implementation has a narrower throwing effect.

A known-implementor union alone does not support open host implementations. Runtime admission remains authoritative: a separate native check copies the generated private marker onto an unrelated object and confirms rejection. This does not yet test forged descriptors or every dynamic entry path. Host registration will need to supply its own checked evidence; the initial input implementation does not provide that role.

### Copied records need conformance checks too

Consider a generated Pydantic record whose BAML class implements an empty `Tagged` interface. It has a known concrete BAML declaration and can be passed as `Tagged` without becoming a host implementation. The intended crossing is:

```text
generated record → validate concrete declaration and fields → copy into BAML
                 → check the Tagged implementation → invoke the function
```

Subsequent edits to the Python record do not edit that copy. If BAML returns an interface view of it, that view retains the new BAML receiver. A plain Python object with similar fields has no such concrete declaration. This is also different from an untyped native list: the generated record identifies its concrete source type, whereas an `Iterable` destination alone cannot choose a list's element type or concrete BAML representation.

F40's generated probe passed native typing but failed runtime admission with `TaggedRecord is not assignable to Tagged`. F41 repairs this by resolving the concrete source, checking its generic arguments and implementation against the retained VM contract, and validating/materializing the fields under that source contract. The unchanged probe now passes. Host callback returns use the same rule with strict result-field validation; transports no longer run a competing name-only return check before the engine. See plan F41 for tests and the remaining bridge gates.

The initial Python input translator handles direct interface heads and unions containing them. Required-interface refs, conditional implementations, aliases, nested records/containers, callable direction and other backends remain separate acceptance gates. Passing the direct-input probe must not turn those unchecked cases into an “all interfaces just work” claim.

### F46 finding and F47 resolution: callback types follow the crossing direction

F46 verified a Python async handler receiving a `CounterRef`, calling a BAML method, and retaining that receiver. Its generated callback annotation still omitted asynchronous completion. F47 corrects that direction recursively through callable types, aliases, sequences and mappings. For example, the generated signatures now include:

```python
async def visit_counter_async(
    initial: int,
    callback: Callable[[CounterRef], int | Awaitable[int]],
) -> int: ...

async def call_counter_factory_async(
    factory: Callable[[], CounterInput | Awaitable[CounterInput]],
) -> CounterRef: ...

async def add_counter_map_async(
    values: Mapping[str, CounterInput], amount: int,
) -> list[int]: ...
```

The callback receives a checked interface reference and can use `await counter.add(2)` immediately. A factory can return a generated concrete `StoredCounter` directly; completion checks its implementation and returns a checked `CounterRef`. Containers copy their structure across the boundary while retained children keep their receivers. Canonical output aliases remain output types; private input companions describe accepted arguments without requiring users to import those helpers.

Optional callbacks use a Protocol with positional required arguments and `Unpack` of a **closed** optional-keyword `TypedDict`. The closed dictionary matters: an open dictionary can describe additional keywords, so it incorrectly rejects an ordinary handler such as `async def update(counter: CounterRef, *, amount: int = 1) -> int`. Both generic free-function callbacks and callbacks passed to generic interface methods have generated typing and execution evidence.

Run `sdks/python/.venv/bin/python interface_probes/python/check_input_roles.py <directory-containing-baml_sdk>` against the generated shared fixture. The positive consumer passes strict Pyright; the negative consumer rejects exactly fourteen marked calls. These exercise parameter direction, awaitable result direction, aliases, container implementations, optional names/types and generic relationships. Plan F47 records the separate runtime results.

Remaining limits: invariant generic BAML classes still need canonical arguments; this is not a generic record input-construction solution. Generic optional keywords that cannot be represented as Python identifiers still need a checked adapter. Returned BAML closures retain their original runtime/map and native handle, but their public annotation is still synchronous `Callable`; a typed lifecycle/async callable surface remains unfinished. Custom awaitables execute on the existing dispatch loop, so application-owned event-loop routing remains open. Host interface registration and the corresponding other-language ports remain separate gates.

### Registration metadata prerequisite (F53)

A method can have type arguments even when none appears in its values:

```baml
interface RecordMarker {}
interface BoundedMethod {
    function check<T extends RecordMarker, U>(self) -> string throws never
}
```

The loaded declaration now retains both T and U and T's RecordMarker bound. `check` still returns a string; the choices constrain the call and must be supplied/validated independently of that return type. Python and TypeScript's existing shared generic-method cases now include this shape. Native annotations remain distinct from runtime conformance evidence.

This closes a metadata gap needed for host registration. It does not yet make a Python object with a `check` method into a BAML implementation. The registration endpoint and generated implementation adapters are still pending. Plan F53 records actual verification separately from the proposed host API earlier in this document.
