# FunctionSpec, streaming, and FFI values: reset-friendly design

Status: implemented in the current working tree; focused validation complete
Scope: preserve the useful behavior from `codex/function-spec-streaming-redesign` while removing the cross-layer function-operation redesign
Baseline: surgically restored to `origin/canary` semantics inside `codex/function-spec-streaming-redesign`, then replayed by concern

## 1. Outcome

The final change should do all of the following:

1. Let runtime-created class and enum values cross the FFI boundary, including as streaming partials, without losing nominal identity. These values use the existing tagged, rooted-handle architecture.
2. Preserve the Python SDK type improvements, particularly accurate stream accessor types and `ai.FunctionSpec<Out>` as a one-parameter generic.
3. Keep streaming direct and allocation-conscious: a generated stream binding calls the internal stream companion directly, a stream owns one live capability, and partial runtime values remain rooted handles instead of being structurally copied.
4. Fix optional-enum matches so `null` and unrelated union members reach a wildcard arm instead of being read as enum discriminants.
5. Rename callable companion FQNs from `$` to `@` internally.
6. Expose only three host SDK entry points per authored LLM function:
   - the authored function;
   - the existing language-specific stream binding;
   - a new language-specific spec binding.
7. Keep `ai.FunctionSpec.call`, `parse`, and `client_id` (along with the other useful reflected methods already implemented).
8. Keep portable prompt and media values.
9. Keep deterministic Python identifier projection and report every public symbol that had to be renamed.
10. Keep cancellation propagation and capability lifetime/GC fixes.

The final change should **not** introduce a semantic `FunctionOperation` axis through codegen, the wire protocol, project, or engine layers. A host bridge may retain a local `FunctionOperation::{Direct, Spec, Stream}` selector as API-boundary plumbing, but it must resolve that selector to an exact companion FQN before the wire call. Spec and stream companions remain ordinary compiled functions with exact internal FQNs.

## 2. Terminology and hard boundaries

There are three different concepts that must not be conflated:

| Concept | Example | Meaning |
|---|---|---|
| Authored callable | `user.orders.Extract` | User-declared function identity |
| Internal callable companion | `user.orders.Extract@spec` | Ordinary compiled function hidden from the host's general function surface |
| Generated host binding | Python `Extract_spec`, TypeScript `Extract$stream` | Language-owned public spelling that calls an exact FQN |
| Partial type identity | `Order$stream` | Existing generated type convention; this is not a callable companion |

The `$` to `@` rename applies to **callable companion FQNs only**. It must not rename partial classes, type aliases, or any other generated type whose `$stream` suffix is already part of its schema identity.

The BAML parser supports source references to the compiler-produced callable postfixes
`@spec`, `@stream`, `@render_prompt`, `@build_request`, and `@parse`. This
preserves the ability to exercise ordinary companion functions from BAML source
while changing their exact identity from `$` to `@`. `@stream` is therefore
valid companion-reference syntax, not a special streaming expression or a
second dispatch protocol. Only `@spec` and `@stream` are projected into host SDK
bindings; the other helper companions remain internal to the BAML surface.

## 3. Architectural decision

### 3.1 Exact FQN calls, not operation dispatch

The bridge already knows how to call an ordinary BAML function by FQN. Generated host bindings should use that path:

```text
Python Extract(...)          -> call_function("user.orders.Extract", args)
Python Extract_spec(...)     -> call_function("user.orders.Extract@spec", args)
Python Extract_stream(...)   -> call_function("user.orders.Extract@stream", args)
```

The same rule applies in every SDK. The public spelling differs by language; the wire/runtime FQN does not.

A hand-written host bridge API may expose a local selector for convenience:

```rust
enum FunctionOperation {
    Direct,
    Spec,
    Stream,
}

fn exact_fqn(authored_fqn: &str, operation: FunctionOperation) -> String {
    match operation {
        FunctionOperation::Direct => authored_fqn.to_owned(),
        FunctionOperation::Spec => format!("{authored_fqn}@spec"),
        FunctionOperation::Stream => format!("{authored_fqn}@stream"),
    }
}
```

That enum is boundary-local only. It is not serialized, attached to codegen
symbols, or passed into project/engine dispatch.

The inbound call request therefore stays conceptually simple:

```proto
message CallFunctionArgs {
  string function_name = 1; // exact FQN, including an internal @ companion
  repeated NamedArg args = 2;
  // Existing call context/options fields remain.
  // No FunctionOperation field.
}
```

Do not retain or recreate this shape:

```proto
// Do not keep this architecture.
enum FunctionOperation {
  FUNCTION_OPERATION_DIRECT = 0;
  FUNCTION_OPERATION_SPEC = 1;
  FUNCTION_OPERATION_STREAM = 2;
}

message CallFunctionArgs {
  string function_name = 1;       // authored FQN
  FunctionOperation operation = 2; // second identity axis
}
```

The second form forces every layer to understand projections and caused most of the PR's fan-out. The exact-FQN form preserves ordinary function lookup, parameter binding, tracing, callables, cancellation, and error reporting.

### 3.2 Compiler companions remain ordinary functions

Companions should continue through the normal AST -> HIR -> TIR -> MIR -> bytecode pipeline and retain `FunctionOrigin::Companion`. Their origin is metadata for filtering and diagnostics; it is not a new execution protocol.

A minimal naming helper is enough:

```rust
fn companion_fqn(parent: &Name, suffix: &str) -> Name {
    Name::new(format!("{parent}@{suffix}"))
}

let spec_name = companion_fqn(&function.name, "spec");
let stream_name = companion_fqn(&function.name, "stream");
```

If the existing implementation continues to synthesize helper companions for internal BAML use, rename them consistently too:

```text
Fn$spec           -> Fn@spec
Fn$stream         -> Fn@stream       (callable only)
Fn$render_prompt  -> Fn@render_prompt
Fn$build_request  -> Fn@build_request
Fn$parse          -> Fn@parse
```

Those helper companions may remain compiled, but the SDK generators must not publish standalone host bindings for `@render_prompt`, `@build_request`, or `@parse`. Their public replacement is the returned `FunctionSpec` object.

### 3.3 Source lowering stays small

The terminal segment of a source `@spec` expression should lower to the ordinary internal companion name:

```rust
fn lower_spec_expr(path: &mut [Name]) -> Result<(), LowerError> {
    let Some(last) = path.last_mut() else {
        return Err(LowerError::EmptySpecPath);
    };

    *last = Name::new(format!("{}@spec", last.as_str()));
    Ok(())
}
```

The exact error type and `Name` API should match the surrounding file. The important invariant is that lowering produces a normal callable reference; it must not produce a special MIR expression or a bridge-only operation.

The synthesized stream companion should likewise call `Fn@spec` through normal BAML:

```baml
// Compiler-generated shape, shown as BAML pseudocode.
function Extract@stream(args..., client: Client?, on_event: StreamCallback?)
    -> ai.stream.Stream<Extract$stream, Extract> {
  let spec = Extract@spec(args...)
  spec.stream(client: client, on_event: on_event)
}
```

`Extract$stream` above is intentionally unchanged because it is a partial **type**, not a callable.

## 4. Host SDK surface

### 4.1 What is exported

For each authored LLM function, generators emit:

```text
direct sync/async binding
spec sync/async binding
stream sync/async binding, following that SDK's existing conventions
```

They do not emit host bindings for internal render/build/parse companions.

The apparent tension between “remove external companions except stream” and “add spec” resolves to this final public surface: direct + stream + spec. Spec is the one newly requested host companion; all legacy helper companions disappear from the host API.

### 4.2 Naming belongs to each generator

Do not add a shared `BindingRole` abstraction whose job is to calculate public names for all languages. Each generator already has different casing, async, namespace, collision, and reserved-word rules. It should recognize an internal companion suffix and allocate the public spelling using its existing naming machinery.

The expected defaults are illustrative, not a new cross-language contract:

| SDK | Spec example | Stream example |
|---|---|---|
| Python | `Extract_spec`, `Extract_spec_async` | `Extract_stream`, `Extract_stream_async` |
| TypeScript | `Extract_spec` | keep the generator's existing `$stream` spelling |
| Go | `ExtractSpec` | `ExtractStream` |
| Rust | `extract_spec`, `extract_spec_async` | `extract_stream`, `extract_stream_async` |
| Java | `extractSpec` or current generated convention | current stream convention |
| C# | `ExtractSpec`, `ExtractSpecAsync` | current stream convention |
| Swift | current Swift projection + `_spec` | current stream convention |
| C++ | `Extract_spec`, `Extract_spec_async` | current stream convention |

Authored collisions must go through the language's allocator. For example, if a user authors `Extract_spec`, Python must allocate a deterministic alternate spelling for one of the bindings and report it; the compiler must not globally mutate the BAML symbol.

### 4.3 Generated call shape

Python pseudocode:

```python
def Extract(question: str, *, baml_options: BamlOptions | None = None) -> Answer:
    return _call_sync(
        function_name="user.orders.Extract",
        args={"question": question},
        options=baml_options,
    )

def Extract_spec(
    question: str,
    *,
    baml_options: BamlOptions | None = None,
) -> BamlFunctionSpec[Answer]:
    return _call_sync(
        function_name="user.orders.Extract@spec",
        args={"question": question},
        options=baml_options,
    )

def Extract_stream(
    question: str,
    *,
    client: BamlClient | None = None,
    on_event: StreamCallback | None = None,
    baml_options: BamlOptions | None = None,
) -> BamlStream[AnswerPartial | None | Done, AnswerPartial, Answer]:
    return _call_sync(
        function_name="user.orders.Extract@stream",
        args={
            "question": question,
            "client": client,
            "on_event": on_event,
        },
        options=baml_options,
    )
```

There is no `projection="spec"`, `operation=Spec`, or authored-FQN lookup followed by companion reconstruction.

### 4.4 Symbol-pool behavior

The simplest reset-friendly approach is:

1. Keep companion functions as normal symbols with `FunctionOrigin::Companion`.
2. Stop filtering `@spec` merely because `ai.FunctionSpec` was previously unbridgeable.
3. Let each generator decide which origins/suffixes become public bindings.
4. IDE user-function lists filter companions using origin metadata, not spelling.

Do not attach a `FunctionOperations { spec, stream }` object to every authored function. Do not have the IDE symbol pool reconstruct control parameters and projected return types. The companion declaration already contains the exact signature.

## 5. Tagged FFI capabilities

### 5.1 Security and identity model

Some values crossing FFI are data and should be deeply encoded. Others are live engine-owned capabilities and must retain identity. A `RuntimeTy` string is descriptive and can collide; it is not a trustworthy discriminator.

The boundary uses an explicit kind plus a rooted handle:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaggedHeapHandleKind {
    Callable,
    Stream,
    FunctionSpec,
    RuntimeValue,
}

pub enum BexExternalAdt {
    // ... portable/data ADTs ...
    TaggedHeapHandle {
        kind: TaggedHeapHandleKind,
        ty: RuntimeTy,
        heap_handle: Handle,
    },
}
```

Rules:

- `kind` is the trusted boundary discriminator.
- `ty` is host-facing annotation/diagnostic metadata only.
- `heap_handle` roots the original heap object and is scoped to the engine that issued it.
- Decode validates the handle's tagged kind before selecting a host wrapper.
- Returning the handle inbound resolves the same object; it never performs a lookup by display name.
- Cross-process transport of a live capability is rejected. Prompt/media are portable data and follow a different path.

### 5.2 Outbound conversion for runtime-created class/enum values

Static generated classes/enums can still be encoded structurally into their known host types. A value whose runtime tag is dynamic must leave as `RuntimeValue`:

```rust
fn lift_class_or_enum(
    &self,
    value: HeapValueRef,
    runtime_ty: RuntimeTy,
    type_tag: TypeTag,
) -> Result<BexExternalValue, ConversionError> {
    if type_tag.is_dynamic() {
        let heap_handle = self.heap.root(value)?;
        return Ok(BexExternalValue::Adt(
            BexExternalAdt::TaggedHeapHandle {
                kind: TaggedHeapHandleKind::RuntimeValue,
                ty: runtime_ty,
                heap_handle,
            },
        ));
    }

    self.lift_static_nominal(value, runtime_ty)
}
```

The real implementation must preserve union selection before opaque conversion. For a declared `Static | Dynamic | null` result, determine the selected runtime arm first, then lift a dynamic class/enum as the tagged handle. Otherwise the host loses the union member that was actually selected.

### 5.3 Inbound conversion

Inbound decoding is capability restoration, not structural reconstruction:

```rust
match tagged.kind {
    TaggedHeapHandleKind::RuntimeValue => {
        let rooted = self.handles.resolve(tagged.heap_handle)?;
        self.assert_same_engine(&rooted)?;
        Ok(rooted.as_vm_value())
    }
    TaggedHeapHandleKind::FunctionSpec => {
        let rooted = self.handles.resolve(tagged.heap_handle)?;
        self.validate_function_spec(&rooted)?;
        Ok(rooted.as_vm_value())
    }
    // Callable and Stream use their existing validation.
}
```

Never use `tagged.ty` to select a generated typemap decoder. In particular, a dynamic class may intentionally have the same display name as a statically compiled class; the rooted object is authoritative.

### 5.4 Wire and handle-table discriminants

The bridge's handle enum should have stable, explicit cases:

```proto
enum BamlHandleType {
  // Existing values retain their numbers.
  ADT_FUNCTION_SPEC = 17;
  ADT_RUNTIME_VALUE = 18;
}
```

The handle table performs a total mapping:

```rust
fn handle_type(kind: TaggedHeapHandleKind) -> BamlHandleType {
    match kind {
        TaggedHeapHandleKind::Callable => BamlHandleType::AdtCallable,
        TaggedHeapHandleKind::Stream => BamlHandleType::AdtStream,
        TaggedHeapHandleKind::FunctionSpec => BamlHandleType::AdtFunctionSpec,
        TaggedHeapHandleKind::RuntimeValue => BamlHandleType::AdtRuntimeValue,
    }
}
```

Generated protobuf clients across SDKs are expected mechanical changes. Regenerate them once, in a dedicated commit, after the hand-written protocol is final.

### 5.5 Python runtime wrapper

The public Python wrapper stays intentionally opaque:

```python
class BamlRuntimeValue:
    __slots__ = ("_handle",)

    def __init__(self, handle: BamlPyHandle) -> None:
        self._handle = handle

    @classmethod
    def _from_pyhandle(cls, handle: BamlPyHandle) -> "BamlRuntimeValue":
        return cls(handle)

    def _to_pyhandle(self) -> BamlPyHandle:
        return self._handle

    def to_data(self) -> Any:
        return _call_sync(
            function_name="baml.json.from",
            args={"value": self},
        )
```

`to_data()` is explicit because structural projection can lose live nominal identity. The proxy itself must round-trip into BAML unchanged and must remain alive across major GC while the host retains it.

Equivalent SDK wrappers should follow local lifetime conventions: RAII/drop in Rust/C++, finalizers or safe handles where needed, and no eager structural decode.

## 6. FunctionSpec capability

`ai.FunctionSpec<Out>` crosses FFI as `TaggedHeapHandleKind::FunctionSpec` and is represented by a host proxy with exactly one type parameter:

```python
TOut = TypeVar("TOut")

class BamlFunctionSpec(Generic[TOut]):
    def call(self, **kwargs: Any) -> TOut: ...
    async def call_async(self, **kwargs: Any) -> TOut: ...

    def parse(self, json: str) -> TOut: ...
    async def parse_async(self, json: str) -> TOut: ...

    def client_id(self) -> str: ...
    async def client_id_async(self) -> str: ...
```

`FunctionSpec` is **not** `FunctionSpec<Partial, Final>`. Any two-argument special case in Python type translation should be removed. Streaming is represented by `ai.stream.Stream<Partial, Final>`; spec is a bound recipe returning only `Out`.

The proxy methods call ordinary BAML method FQNs with the capability as `self`:

```python
def parse(self, json: str) -> TOut:
    return _call_sync(
        function_name="ai.FunctionSpec.parse",
        args={"self": self, "json": json},
    )

def call(self, **kwargs: Any) -> TOut:
    return _call_sync(
        function_name="ai.FunctionSpec.call",
        args={"self": self, **kwargs},
    )

def client_id(self) -> str:
    return _call_sync(
        function_name="ai.FunctionSpec.client_id",
        args={"self": self},
    )
```

This keeps `call`, `parse`, `client_id`, `name`, `arguments`, `output_type`, `prompt`, `tools`, and `build_request` without making them SDK-generated function companions.

## 7. Prompt and media portability

Prompt and media are data values, not same-engine capabilities. Keep their inbound/outbound bridge representation:

```proto
message InboundValue {
  oneof value {
    // Existing scalar/composite cases...
    MediaValue media_value = 15;
    PromptAstValue prompt_ast_value = 16;
  }
}
```

Required behavior:

- `FunctionSpec.prompt()` may return a prompt through FFI.
- The host can inspect text/messages using its SDK wrapper.
- The prompt can be encoded inbound again.
- Images/audio/media embedded in prompts preserve mime type, source kind, URL/file/base64 or byte payload as defined by the portable schema.
- No live heap pointer or engine-local handle is serialized as prompt/media data.

Because the inbound proto imports outbound prompt/media types, import order must keep
`baml_type.proto` before `baml_outbound.proto`. ProtobufJS emits definitions in
import-traversal order; reversing those imports moves thousands of unchanged
generated lines. Keep this order stable so the protocol diff contains only the
new fields and messages.

## 8. Streaming efficiency and typing

### 8.1 Efficiency invariants

The implementation should satisfy these invariants rather than introduce another streaming abstraction:

1. The host stream binding calls `Fn@stream` directly. It does not first fetch `Fn@spec` across FFI and then call back into the engine to create a stream.
2. A host stream owns one rooted stream handle for its lifetime.
3. Each pull reuses the same engine stream state and parse cache.
4. Runtime-created partial classes/enums cross as rooted `RuntimeValue` handles, avoiding repeated deep encode/decode and preserving identity.
5. Handles are cloned only where ownership crosses an async/FFI boundary; temporary clones are released deterministically.
6. Dropping/disposal/finalization of a stream releases its root and cancels outstanding work according to the SDK's existing rules.
7. Cancellation is forwarded for initial stream creation, `next`/pull, and final-result retrieval.

The compiler-generated stream body remains the single place that creates the BAML stream and its parse cache. Do not reconstruct a spec or parser for every chunk.

### 8.2 Python stream types

Python needs three distinct generic positions:

```python
TNext = TypeVar("TNext")    # the exact result of next(), including None/Done
TYield = TypeVar("TYield")  # values yielded by async iteration
TFinal = TypeVar("TFinal")  # final()

class BamlStream(Generic[TNext, TYield, TFinal]):
    async def next(self) -> TNext: ...

    def __aiter__(self) -> AsyncIterator[TYield]: ...

    async def final(self) -> TFinal: ...
```

For an LLM function returning `Answer`, the generated annotation is conceptually:

```python
BamlStream[
    AnswerPartial | None | Done,  # next()
    AnswerPartial,                # async for item
    Answer,                       # final()
]
```

The async iterator filters end-of-stream/null control values before yielding:

```python
async def _iterate(self) -> AsyncIterator[TYield]:
    while True:
        item = await self.next()
        if isinstance(item, Done):
            return
        if item is None:
            continue
        yield cast(TYield, item)
```

The Rust generator's type translation should remove only the top-level null/control alternative for `TYield`. It must preserve nullability nested inside partial fields.

### 8.3 Cancellation and GC

Keep the cancellation decoder and host-specific cancellation behavior already covered by tests. Python's pattern is:

```python
call_id = new_function_call()
try:
    raw = await runtime.call_function(encoded, ...)
except asyncio.CancelledError:
    try:
        cancel_function_call(call_id)
    except Exception:
        # Cancellation remains primary even if native cleanup reports an error.
        pass
    raise
return _decode_call_result_async(raw)
```

The same cancellation-aware decoder must be used by stream, FunctionSpec, prompt, and runtime-value async capability methods.

GC tests must cover:

- a runtime value survives a forced major collection while held by the host;
- a stream survives collection between pulls;
- dropping the last host wrapper eventually releases its handle root;
- cancellation racing completion does not double-release or leak the call entry;
- an authored dynamic class whose name collides with a static class still resolves by handle identity.

## 9. Optional enum match correctness

The optimized enum jump table may read a discriminant only when every possible scrutinee value belongs to the same enum. `Status?` includes `null`, so it must fall back to the normal comparison chain.

Keep the closed predicate:

```rust
/// Whether every admitted value has a discriminant for `enum_name`.
fn runtime_ty_is_enum_only(ty: &RuntimeTy, enum_name: &TypeName) -> bool {
    match ty {
        RuntimeTy::Enum(name, _) | RuntimeTy::EnumVariant(name, _, _) => {
            name == enum_name
        }
        RuntimeTy::Union(members, _) => members
            .iter()
            .all(|member| runtime_ty_is_enum_only(member, enum_name)),
        _ => false,
    }
}
```

Before emitting `Rvalue::Discriminant`:

```rust
if let Some(SwitchKind::EnumDiscriminant(enum_name)) = &switch_kind
    && !runtime_ty_is_enum_only(&self.builder.local_ty(scrutinee), enum_name)
{
    return false; // use the comparison-chain lowering
}
```

Regression test:

```baml
enum Status {
  Active
  Inactive
}

function classify_nullable_status(s: Status?) -> string {
  match (s) {
    Status.Active => "active"
    _ => "other"
  }
}

test "match_nullable_enum_null_uses_wildcard" {
  assert.equal(classify_nullable_status(null), "other")
}
```

Also retain or add a mixed-enum union case so a value from another enum reaches `_` rather than being interpreted with the first enum's discriminant table.

## 10. Python name projection and reporting

### 10.1 Separate wire identity from Python spelling

BAML names are wire identities. Python identifiers are projections. Definitions, references, type annotations, Pydantic aliases, runtime argument encoding, stubs, and function registries must all read from one allocation table.

The allocator should record structured changes:

```rust
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum IdentifierRenameReason {
    PythonKeyword,
    InvalidIdentifier,
    HostControl,
    FrameworkProtected,
    Collision,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct IdentifierRename {
    pub kind: String,
    pub fqn: String,
    pub original: String,
    pub generated: String,
    pub reason: IdentifierRenameReason,
}

pub struct GeneratedPythonSdk {
    pub files: HashMap<PathBuf, String>,
    pub renames: Vec<IdentifierRename>,
}
```

The exact internal data structures can be simplified during replay, but allocation must remain deterministic and global enough to prevent two projected names from overwriting one another.

### 10.2 Print renames once at the generator boundary

Do not insert warning comments into every generated source file. Return structured renames, sort and deduplicate them, and let the CLI/generation driver print a stable report:

```rust
let GeneratedPythonSdk { files, mut renames } = generate_python_sdk(...)?;

renames.sort();
renames.dedup();

for rename in renames {
    eprintln!(
        "warning: renamed Python {} `{}` from `{}` to `{}` ({})",
        rename.kind,
        rename.fqn,
        rename.original,
        rename.generated,
        rename.reason,
    );
}

write_generated_files(files)?;
```

Example output:

```text
warning: renamed Python field `user.Invoice.model_dump` from `model_dump` to `model_dump_` (framework-protected spelling)
warning: renamed Python parameter `user.Extract.class` from `class` to `class_` (Python keyword)
warning: renamed Python function `user.Extract@spec` from `Extract_spec` to `Extract_spec_` (collision after Python projection)
```

The report must be stable across runs and must not contain absolute output paths. It should only report public/user-visible projections, not harmless private temporaries.

### 10.3 Pydantic and runtime alias invariants

For a renamed field or enum member, host and wire names stay distinct:

```python
class Invoice(BaseModel):
    model_dump_: str = Field(alias="model_dump")

    model_config = ConfigDict(populate_by_name=True)
```

Encoding uses the raw BAML name (`model_dump`); Python access uses `model_dump_`. The same applies to callable parameters: generated Python keyword names must be mapped back to raw BAML argument names before bridge encoding.

Keep tests for Python keywords, invalid identifiers, Pydantic-protected names, companion collisions, module collisions, enum raw-value encoding, parameter aliases, generics, and `.pyi`/runtime agreement.

## 11. What to keep, replay, or reset

Desired and undesired work was interleaved in several files. The implementation
used whole-file restores only where a file contained operation dispatch alone,
then replayed mixed files surgically using the slices below.

### 11.1 Keep/replay

| Slice | Principal paths | Notes |
|---|---|---|
| Optional enum fix | `crates/baml_compiler2_mir/src/lower.rs`, `crates/baml_tests/baml_src/ns_match_types/` | Small independent patch |
| RuntimeValue typetag | `crates/bex_external_types/`, `crates/bex_engine/src/conversion.rs`, bridge handle table/proto/adapters, SDK codecs/wrappers | Keep focused kind/handle logic and tests |
| FunctionSpec capability | `crates/baml_builtins2/baml_std/ai/spec.baml`, conversion/handle kind, SDK wrappers/codecs | Keep methods; remove operation dispatch |
| Prompt/media portability | inbound/outbound value proto, bridge encode/decode, SDK prompt/media wrappers and tests | Regenerate protocol artifacts once |
| Python stream types | Python `_stream.py`, `.pyi`, generator type translation, static typing fixture | Preserve three generic positions |
| Python name projection | Python generator allocator/aliases/report data and tests | Remove dependence on global operation roles |
| Cancellation/GC | bridge call contexts, async decoders, host lifetime owners, focused tests | Preserve independently from companions |
| SDK end-to-end tests | `sdk_tests` spec/stream/runtime/prompt tests | Rewrite calls only where they mention operation dispatch |

### 11.2 Reset/remove

| Slice to remove | Principal paths/symbols | Replacement |
|---|---|---|
| Authored-function operation metadata | `baml_codegen_types::FunctionOperations`, `SpecOperation`, `StreamOperation` | Ordinary companion symbols/signatures |
| Symbol-pool projection reconstruction | operation maps and derived control args in `baml_ide/src/symbol_pool.rs` | filter by `FunctionOrigin`; generator reads companion symbol |
| Wire operation selector | `FunctionOperation` and `CallFunctionArgs.operation` | exact `function_name` FQN |
| Engine semantic dispatcher | `FunctionOperation::{Direct,Spec,Stream}`, `*_operation` entry switching | ordinary function lookup/call |
| Project operation APIs | `call_function_operation`, `call_callable_operation`, operation parameter lookup | existing call-by-name/callable APIs |
| Cross-SDK shared binding roles | generator-wide Direct/Spec/Stream fan-out metadata | per-generator companion filtering and naming |
| A distinct `STREAM_EXPR` or special `@stream` execution mode | parser/lowering operation-specific nodes | ordinary companion-reference syntax lowering to `Fn@stream` |

### 11.3 Files requiring surgical replay

These cannot safely be checked out wholesale because they contain both good and unwanted changes:

- `crates/bex_engine/src/lib.rs`: keep cancellation/lifetime and capability behavior; remove operation dispatch.
- `crates/bex_engine/src/conversion.rs`: keep RuntimeValue, FunctionSpec, prompt/media, and union-selection fixes; discard unrelated cleanup.
- `crates/bridge_ctypes/types/**`: keep handle kinds and portable values; omit `FunctionOperation`.
- every SDK codec: keep new handle/prompt/media cases; omit operation fields.
- every SDK generator: keep local spec/stream binding emission and relevant type fixes; omit centralized operation metadata.
- Python generator: keep allocator, aliases, type fixes, and rename report; reshape companion allocation around actual companion symbols.

## 12. Recommended replay sequence

Use one conceptual concern per commit so any later regression can be dropped without resetting a directory:

1. **Optional enum match correctness.** Add the MIR guard and regression tests.
2. **RuntimeValue core.** Add the tagged kind, conversion rules, same-engine restoration, and engine tests.
3. **RuntimeValue bridge.** Add the stable handle discriminant, CFFI/WASM adapters, host codecs/wrappers, and dynamic stream round-trip tests.
4. **Prompt/media portability.** Add hand-written protocol/codec behavior, then regenerate protocol clients in a separate mechanical commit.
5. **FunctionSpec core.** Keep the BAML methods and tagged capability conversion, without any function-operation API.
6. **Internal companion rename.** Rename callable `$...` FQNs to `@...`, lower each supported source companion postfix—including `@spec` and `@stream`—to its ordinary exact FQN, and synthesize `Fn@stream`.
7. **Per-SDK spec/stream bindings.** Each generator exposes only direct + spec + stream and invokes exact companion FQNs.
8. **Python typing.** Apply `BamlStream[TNext, TYield, TFinal]`, one-argument `BamlFunctionSpec[TOut]`, aliases, and static type fixtures.
9. **Python rename reporting.** Return, sort, deduplicate, and print structured public rename records.
10. **Cancellation and GC.** Replay any remaining lifetime/cancellation fixes with focused tests.
11. **Snapshots/generated output.** Regenerate once after semantics stabilize; isolate mechanical changes.

Before replaying a slice, inspect it rather than trusting directory history:

```bash
git diff origin/canary...codex/function-spec-streaming-redesign -- <path>
git log --oneline origin/canary..codex/function-spec-streaming-redesign -- <path>
```

The initial unrelated unstaged formatter/lint pass was discarded before this
replay. Generated protobuf and snapshot output was regenerated only after the
hand-written architecture stabilized.

## 13. Tests to retain or recreate

### 13.1 Compiler/engine

- nullable enum `match` with `null` reaches wildcard;
- mixed enum union reaches wildcard;
- `Fn@spec` and `Fn@stream` exist as ordinary compiled functions;
- `$spec`/callable `$stream` no longer exist;
- partial type `$stream` names remain unchanged;
- `Fn@spec` returns a rooted `FunctionSpec` handle;
- dynamic enum/class outbound returns `RuntimeValue` kind;
- runtime value inbound resolves the same heap object;
- static-name collision cannot change dynamic identity;
- union metadata selects the dynamic arm before opaque lifting;
- wrong handle kind is rejected;
- cross-engine capability use is rejected cleanly.

Existing tests named around `function_operations` should not be copied verbatim. Preserve their assertions but rewrite setup to call `Ask@spec` or `Ask@stream` by exact name.

### 13.2 Python

Retain the behavior covered by:

- `sdk_tests/crates/python_pydantic2/llm_functions/customizable/test_dynamic_runtime_streaming.py`
- `sdk_tests/crates/python_pydantic2/llm_functions/customizable/stream_accessors_static.py`
- prompt portability tests under the same SDK fixture;
- `sdks/python/tests/test_identifier_aliases.py`;
- `sdks/python/tests/test_decode_handle.py` cancellation cases.

Add/retain assertions that generated modules expose direct/spec/stream but not render-prompt/build-request/parse companions.

Static checks should prove:

```python
reveal_type(await stream.next())       # Partial | None | Done
async for partial in stream:
    reveal_type(partial)               # Partial
reveal_type(await stream.final())      # Final
reveal_type(await fn_spec.call_async()) # Final
reveal_type(fn_spec.parse("{}"))       # Final
```

### 13.3 Other SDKs

Keep or recreate the cross-language spec/stream tests already added in:

- C++ `test_spec_stream.cc`;
- TypeScript customizable/streaming tests;
- Rust streaming end-to-end tests;
- Go companion/spec tests;
- Java main and streaming end-to-end tests;
- Swift main and streaming end-to-end tests;
- C# streaming fixture.

Each SDK test should check behavior and that language's chosen public spelling. It must not assert a universal spelling shared by all SDKs.

### 13.4 Portable values

For both prompt and media:

- outbound decode into host wrapper;
- host inspection;
- inbound encode and BAML round trip;
- nested media in prompt messages;
- URL/file/base64 or byte variants supported by the schema;
- cancellation of async access where applicable.

## 14. Validation commands

Run all Cargo commands from `baml_language/`. Narrow while iterating.

Compiler/engine examples:

```bash
cargo nextest run -p baml_compiler2_mir -E 'test(match)'
cargo nextest run -p bex_engine -E 'test(runtime_type_escape)'
cargo nextest run -p bex_project -E 'test(spec) | test(stream)'
```

Snapshot suites must use insta through nextest:

```bash
cargo insta test --test-runner nextest --dnd -p baml_tests -- match_nullable_enum
```

Before merging, mirror the two complementary CI jobs:

```bash
cargo insta test --test-runner nextest -p baml_tests -p baml_cli -p baml_lsp2_actions --all-features --unreferenced=reject

cargo nextest run --all-features --workspace \
  --exclude baml_tests \
  --exclude baml_cli \
  --exclude baml_lsp2_actions \
  --exclude "sdk_test_*" \
  --exclude baml_bridge
```

Then run the affected SDK test crates and Python static checker using their existing harness commands. Confirm generation is idempotent and leaves no `.snap.new` files or unstaged generated output.

### 14.1 Current validation record

The reset implementation was validated with:

- the full BAML corpus: 3,517/3,517 cases passed on both the honest no-cache
  path and a fresh normal cache path;
- focused compiler/IDE companion suites: 39/39 and 29/29 passed;
- cancellation, GC, dynamic-type, and stream engine coverage: 45/45 passed;
- all generator unit suites: 465/465 passed, with one pre-existing skip;
- Python `llm_functions`: 29/29 passed, including sync/async dynamic
  RuntimeValue streaming and prompt portability;
- TypeScript bridge: 120/120 passed;
- bridge/codec suites: 128/128 passed, with one pre-existing skip.

The C++ SDK integration setup could not run in this environment because
`cmake` is not installed. Its Rust generator tests pass and the C++ fixture was
updated to assert exact `@spec`/`@stream` FQNs with no operation selector.

## 15. Acceptance checklist

- [x] No wire/codegen/project/engine `FunctionOperation` axis or call-request operation field remains; a host-bridge-local selector is permitted.
- [x] No engine/project `*_operation` call API is needed for spec or stream.
- [x] Internal callable companions use `@`; partial type names keep `$stream`.
- [x] BAML source `@spec` lowers to the ordinary `Fn@spec` symbol.
- [x] SDKs expose direct + their existing stream spelling + a new spec spelling.
- [x] SDKs do not expose standalone render-prompt/build-request/parse companions.
- [x] Every SDK calls `Fn@spec`/`Fn@stream` as exact FQNs.
- [x] `FunctionSpec<Out>` has one generic and supports `call`, `parse`, and `client_id`.
- [x] Dynamic enum/class values cross streaming FFI as `RuntimeValue` handles.
- [x] RuntimeValue and FunctionSpec kinds are validated independently of `RuntimeTy`.
- [x] Prompt/media round-trip portably.
- [x] Python `next`, async iteration, and final types are distinct and correct.
- [x] Python public renames are deterministic, alias-correct, and printed once.
- [x] Optional-enum wildcard behavior is covered.
- [x] Cancellation and major-GC tests pass for live capabilities and streams.
- [x] Generated protocol/snapshot churn is separated from hand-written architecture in the diff.

## 16. Explicit non-goals

- A new universal cross-SDK naming policy.
- Renaming partial `$stream` types.
- A general function-projection/operation protocol.
- Reconstructing spec/stream signatures in the IDE symbol pool.
- Making live RuntimeValue or FunctionSpec handles portable across processes.
- Exposing render-prompt/build-request/parse as generated top-level functions.
- Special execution semantics for source `@stream`. The supported postfix is
  only an ordinary companion reference and carries no extra operation semantics.
- Mixing formatter, clippy, boxing, or unrelated cleanup into the replay.
