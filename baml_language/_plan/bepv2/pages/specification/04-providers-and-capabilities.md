# 4. Providers and Capabilities

A provider is a configured model adapter stored in an ordinary BAML value.
Because it is a value, code can construct it with `let`, return it from a
function, wrap it with policy, or replace it for one call. Capability
interfaces state which operations it supports.

## Declaring providers

```baml
let Fast = ai.OpenAi {
  model: "gpt-5",
  api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
}

let Careful = ai.Anthropic {
  model: "claude-sonnet-5",
  api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
  max_tokens: 8_192,
}

let Local = ai.OpenAiCompatible {
  base_url: "http://localhost:8000/v1",
  model: "local-model",
  auth: ai.NoAuth {},
}
```

There is no options blob and no provider enum: `Anthropic` has `max_tokens`
because Anthropic has it; `OpenAiCompatible` has `auth` because compatible
endpoints vary. Vendor options are typed fields on the vendor's class, with
`extra_headers` / `extra_body` as the escape hatch for options BAML has not
typed yet. Secrets are ordinary expressions (`baml.env.get_or_panic`) in
field position — no hidden resolution.

A task names its default:

```baml
function ExtractInvoice(document: pdf) -> Invoice {
  provider: Fast
  prompt: `Extract this invoice: ${document}. ${ctx.output_format}`
}
```

## Dynamic providers: deriving variants

The question every real codebase hits: *"same provider, but change one thing,
for this call only."* Because providers are class values, the language's
struct-update spread already answers it:

```baml
// same key, same headers, same everything — different model:
let invoice = ExtractInvoice(doc, $provider = ai.OpenAi { ...Fast, model: "gpt-5-mini" })

// a reusable derived provider:
let FastMini = ai.OpenAi { ...Fast, model: "gpt-5-mini" }

// several overrides at once; later entries win:
let Debugging = ai.OpenAi {
  ...Fast,
  base_url: "http://localhost:4000/v1",     // proxy for capture
  extra_headers: { "x-trace": run_id },
}
```

Normatively, spread is **same nominal class only**, with compatible generic
arguments. That is a feature, not a limitation: "`Fast` but on Anthropic" is
not a field tweak — Anthropic has different fields, different auth, and
different option semantics. Cross-vendor moves are a new declaration, which
is exactly the review visibility they deserve:

```baml
let CarefulVariant = ai.Anthropic { ...Careful, model: "claude-haiku-5" } // ok: same class
```

The compiler validates every ordinary class-spread operand against the
destination's full nominal type, including generic arguments.

## Construct configuration atomically

Configuration values are complete at construction time. Do not construct a
provider, hook policy, task, or options value and then immediately assign its
fields. Derive an exact-class variant with typed spread, where later fields
win:

```baml
let hooks = AddToolHooks { tool_to_add: lookup_account }

let options = ai.AgentOptions.new(
  tool_registry = registry,
  hooks = hooks,
  observers = [observer],
)
```

A configuration class should put its named defaults on `Type.new(...)`, rather
than adding a separate free function such as `agent_options(...)`. This keeps
construction discoverable from the type and still lets callers name only the
fields they change. Homogeneous maps such as `map<string, T>` are not a
substitute for a heterogeneous typed configuration class.

Mutation is reserved for types whose contract is stateful: `ToolRegistry`,
provider transcripts, sessions/resources, counters, and observer/event
buffers. Configuration classes are init-only by design even where today's
language does not yet enforce immutability.

For families of variants, write a function — providers compose with plain
code:

```baml
function openai_at(tier: string) -> ai.DriveProvider {
  match (tier) {
    "fast"  => ai.OpenAi { ...Fast, model: "gpt-5-mini" },
    "best"  => Fast,
    _       => ai.OpenAi { ...Fast, model: "gpt-5-nano" },
  }
}

let invoice = ExtractInvoice(doc, $provider = openai_at(tenant.tier))
```

## The `provider:` field accepts expressions

The task's `provider:` field takes a `DriveProvider` expression: a name, a zero-arg
function call, or a full literal. `DriveProvider` is required because the LLM function
itself must remain directly callable:

```baml
function Route() -> ai.DriveProvider {
  if (baml.env.get("REGION") == "eu") { EuGateway } else { Fast }
}

function Summarize(text: string) -> Summary {
  provider: Route()                      // resolved per call
  prompt: `Summarize: ${text}. ${ctx.output_format}`
}

function Research(q: string) -> Answer {
  provider: ai.OpenAi { ...Fast, model: "gpt-5-mini" }   // inline literal
  prompt: `Research ${q}. ${ctx.output_format}`
}
```

The `$provider =` call-site override always wins over the field; both are just
the injected parameter and its default
([LLM-function desugaring](./02-desugaring.md)).

## Capability interfaces

`Provider` is the non-I/O base contract. It supplies the information needed
to bind and render a task for that provider, but it does not promise that the
provider can execute the task:

```baml
class ProviderDescriptor {
  family: string,
  model: string?,
  label: string?,
}

interface Provider {
  function descriptor(self) -> ProviderDescriptor throws never
  function prompt_context(self, output_type: type) -> baml.llm.Context throws never
}
```

`prompt_context` is provider-sensitive because output instructions and wire
formats can differ. It is pure task preparation: it MUST NOT perform network
I/O. `descriptor` is for logs, diagnostics, and UI. It is not provider
identity; two separately configured providers may have identical descriptors.

What a provider can *do* is stated by which capability interfaces it
implements — and only those:

### The `Provider` / `DriveProvider` mental model

Keep these two promises separate:

```text
Provider      = can bind and render Task<T>; does not imply execution
DriveProvider = can finish Task<T> through a direct MyFunction(...) call
```

`Provider` deliberately does not define `drive<T>`. Binding a provider to a
task records its implementation and renders the task with that provider's
prompt context. It does not promise a completion-oriented, value-producing
default lifecycle.

A `RealtimeProvider` is not prohibited from also implementing
`DriveProvider`. A concrete provider may implement both when it offers both
interaction shapes. But `RealtimeProvider` alone cannot imply `drive<T>`:

- realtime execution accepts an instruction-only `Task<null>` and returns a
  long-lived `LiveSession` resource, while `drive<T>` must terminate with one
  `Response<T>`;
- a live session may contain many user turns and many assistant responses, so
  there is no intrinsic single turn whose output is the function's `T`;
- normal termination may be the caller closing the connection, a network
  disconnect, or cancellation, none of which defines a completed `T`; and
- realtime requires caller-owned interaction policy: a channel, event
  consumption, interruption handling, and resource cleanup. A direct call has
  nowhere to supply or expose those decisions.

A realtime-only provider can therefore be used honestly through its explicit
resource operation:

```baml
function Talk(message: string) -> null {
  provider: RealtimeOnly
  prompt: `Talk with the user about: ${message}`
}

let task = Talk.task("Hello")
let live_session = ai.open_live(task, channel)

// Compile error: RealtimeOnly does not implement DriveProvider.
// let reply = Talk("Hello", $provider = RealtimeOnly)
```

**Normative usage rule:** `ai.open_live` accepts `Task<null>`, because a live
session has no single final application value. The task supplies instructions,
arguments, tools, and provider selection. The caller retains the `Channel` for
ongoing input/output and the returned `LiveSession` resource for events,
interruptions, and cleanup. Opening the raw resource does not implicitly
execute application tools.

`null` describes the absence of a task result; it does not define when the live
session ends. A realtime-only provider MUST NOT implement `DriveProvider`
merely to make the direct `Talk(...)` form compile. Returning `null`
immediately would discard the live resource, while waiting for disconnect
would hide the controls the caller needs.

To implement `DriveProvider`, that concrete provider would need to define an
additional bounded policy: create or accept a channel, decide which response
is final, parse that response as `T`, handle disconnects and interruptions,
and close the resource. That can be a legitimate convenience, but it is new
behavior—not something implied by supporting realtime.

A future bounded realtime driver may instead return `LiveRun<T>` with an
explicit `final() -> Response<T>`. Until that completion boundary exists,
making `LiveSession` generic would only hide the ambiguity.

Putting a default `drive<T>` on `Provider` would make the direct form compile,
then force realtime-only, background-only, and other specialized providers to
invent such a policy or fail with `Unsupported` at runtime. The separate
`DriveProvider` capability keeps that promise statically honest.

`drive<T>` is also execution policy, not merely transport. A basic provider
may implement it as one `GenerationProvider.generate` call, while an `Agent`
may implement it as an entire model/tool loop that eventually produces `T`.
If a value is intended to support direct LLM-function calls, it implements
`DriveProvider`; otherwise callers create a task value with `.task(...)` and
choose an explicit driver matching the provider's actual capability.

### Naming rule

Every public interface that represents a provider interaction shape uses the
`*Provider` suffix:

| Interaction shape | Capability interface |
| --- | --- |
| provider-default completion | `DriveProvider` |
| one model interaction | `GenerationProvider` |
| incremental model output | `StreamingProvider` |
| provider turns with tool calls | `ToolCallingProvider` |
| deferred work | `BackgroundProvider` |
| batch submission | `BatchProvider` |
| provider-stored conversation | `SessionProvider` |
| live connection | `RealtimeProvider` |
| provider-managed context cache | `ManagedCacheProvider` |

The suffix tells readers that the value can occupy a provider slot and that
the interface is capability evidence for a safe driver. It does not apply to:

- data/view interfaces such as `Messages` and `Transcript`;
- resource interfaces such as `Job`, `Session`, and `LiveSession`;
- policies and hooks such as `RetryPolicy` and `AgentHooks`; or
- syntax-only extension interfaces such as `ProviderSugar`.

Concrete providers and provider compositions keep concise noun names:
`OpenAi`, `Anthropic`, `Agent`, `Retry`, `Fallback`, and `Traced`. A test double
may still use an explicit name such as `FakeProvider` when that improves local
clarity.

`ToolCallingProvider` is deliberately not `Tools` or `AgentProvider`.
`Tools` sounds like a collection, while this capability means the provider can
perform model turns that request tools. It still does not own dispatch, hooks,
budgets, or loop termination. An `Agent` is a concrete provider composition
that packages a `ToolCallingProvider` with that execution policy.

```baml
class OpenAi {
  ...
  implements ai.Provider {}
  implements ai.DriveProvider { ... }       // direct MyFunction(...) behavior
  implements ai.GenerationProvider { ... }
  implements ai.StreamingProvider { ... }
  implements ai.ToolCallingProvider { ... }
}

class OpenAiResponses {
  ...
  implements ai.Provider {}
  implements ai.DriveProvider { ... }
  implements ai.GenerationProvider { ... }
  implements ai.BackgroundProvider { ... }    // submit/poll lifecycle
  implements ai.SessionProvider { ... }      // server-stored continuations
}

class OpenAiRealtime {
  ...
  implements ai.Provider {}
  implements ai.RealtimeProvider { ... }      // and NOT DriveProvider — no direct-call lie
}
```

A capability that is absent is absent honestly: passing a concrete provider
without `StreamingProvider` to the safe stream driver is a compile error. If
its type was erased, `drivers.unsafe.stream` returns a typed
`baml.errors.Unsupported`. There is no universal `call` that everything must
fake.

When you hold a concrete provider, capability methods are directly callable
— no negotiation:

```baml
let m = ai.OpenAi { ...Fast }
let r = m.generate<Invoice>(ExtractInvoice.task(doc, $provider = m))
let s = m.open_session(ai.SessionOptions {})   // if m implements SessionProvider
```

When you hold an existential `ai.DriveProvider`, it remains valid as the
direct call's `$provider`. A fully erased `ai.Provider` is valid on
`.task(...)` but not on the direct form; pass that task to the appropriate
`drivers.unsafe.*` function. Alternatively, demand the capability in your own
signature:

```baml
// this function REQUIRES streaming; say so in the type, not at runtime:
function consume(
  p: ai.StreamingProvider,
  task: ai.StreamTask<Report, PartialReport>,
) -> Report {
  p.stream<PartialReport, Report>(task).final()
}
```

## Writing your own provider

Any class that implements `Provider` plus at least one capability is a
provider. A complete, working test fake:

```baml
class FakeProvider {
  reply: string,

  implements ai.Provider {}

  implements ai.DriveProvider {
    function drive<T>(self, task: ai.Task<T>) -> ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      self.generate<T>(task)
    }
  }

  implements ai.GenerationProvider {
    function generate<T>(self, task: ai.Task<T>) -> ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      ai.Response<T> {
        value: baml.sap.parse<T>(self.reply),
        meta: ai.Meta { provider: "fake", model: null, finish_reason: "stop",
                             usage: null, attributes: {}, raw: null },
      }
    }
  }
}

test "extracts the vendor" {
  let r = ExtractInvoice(doc, $provider = FakeProvider {
    reply: "{\"vendor\": \"ACME\", \"total\": 12.5, \"currency\": \"USD\"}",
  })
  assert.equal(r.vendor, "ACME")
}
```

A real HTTP provider is the same shape with `baml.http.send` in the middle —
see [LLM-function desugaring](./02-desugaring.md). A gateway is the same shape
with your company's endpoint.
None of them require compiler changes.

## Wrappers: policy as a provider

Middleware — moderation, tracing defaults, redaction — is a provider that
holds another provider:

```baml
class Guarded {
  inner: ai.DriveProvider,
  policy: Policy,

  implements ai.Provider {}

  implements ai.DriveProvider {
    function drive<T>(self, task: ai.Task<T>) -> ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      self.policy.check_input(task.messages())
      let r = self.inner.drive<T>(task.with_provider(self.inner))
      self.policy.check_output(r.value)
      r
    }
  }
}

let invoice = ExtractInvoice(doc, $provider = Guarded { inner: Fast, policy: Strict })
```

Two rules keep wrappers honest. First, `task.with_provider(self.inner)`
rebinds the task before delegation, so provider-sensitive prompt context
re-renders for the provider that will actually run. Second, a wrapper
implements only the capabilities it can genuinely forward — a `DriveProvider`
wrapper is not automatically `StreamingProvider` or `BackgroundProvider`;
claiming a capability you cannot police through is how policy silently
evaporates.
(Reliability wrappers such as retry and fallback have extra rules; see
[Reliability and errors](./09-reliability-and-errors.md).)

## Alternatives considered

**A provider enum with an options map** (`provider "openai"` + untyped
`options { ... }`). Rejected: closed to user providers, untyped at exactly
the surface users touch most, and every new vendor knob is a stringly-keyed
convention. Typed classes make `baml describe` and the LSP authoritative
about what a provider accepts.

**Per-provider builder methods** (`Fast.with_model("gpt-5-mini")`).
Rejected: N fields × M providers of boilerplate to reach parity with one
existing language feature (spread), and builders invite chains that
reconstruct spread poorly. Spread also composes with review culture: the
diff shows the base and the delta.

**A universal `model:` override on the call**
(`ExtractInvoice(doc, model = "gpt-5-mini")`). Rejected: `model` is only
universal until it isn't (deployment names on Azure, versioned IDs on
Bedrock); the call-site option set becomes a shadow provider schema. The
spread spelling is two tokens longer and stays fully typed.

**Everything on `Provider`** (one interface with `call`/`stream`/`submit`
/...). Rejected: unsupported operations become statically valid and fail at
runtime everywhere; realtime and background providers implement fake
methods. Capability interfaces keep the contract per interaction shape.

**Intersection types for requirements** (`p: Provider & StreamingProvider &
ToolCallingProvider`). Desirable, and nothing here precludes it — a signature can already
demand *one* capability (`p: ai.StreamingProvider`). Multi-capability
requirements remain a type-system-level future; until then the pattern is
demand-the-narrowest, negotiate the rest.
