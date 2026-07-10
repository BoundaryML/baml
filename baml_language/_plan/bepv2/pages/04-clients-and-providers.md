# 4. Clients and Providers

A provider is an ordinary BAML class value. That single fact powers this
whole page: providers are declared with `let`, derived with struct-update
spread, computed by functions, wrapped by other classes, and swapped per
call — because values already do all of those things.

## Declaring providers

```baml
let Fast = baml.ai.OpenAi {
  model: "gpt-5",
  api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
}

let Careful = baml.ai.Anthropic {
  model: "claude-sonnet-5",
  api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
  max_tokens: 8_192,
}

let Local = baml.ai.OpenAiCompatible {
  base_url: "http://localhost:8000/v1",
  model: "local-model",
  auth: baml.ai.NoAuth {},
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
  client: Fast
  prompt: `Extract this invoice: ${document}. ${ctx.output_format}`
}
```

## Dynamic clients: deriving variants

The question every real codebase hits: *"same client, but change one thing,
for this call only."* Because providers are class values, the language's
struct-update spread already answers it:

```baml
// same key, same headers, same everything — different model:
let invoice = ExtractInvoice(doc, client = baml.ai.OpenAi { ...Fast, model: "gpt-5-mini" })

// a reusable derived client:
let FastMini = baml.ai.OpenAi { ...Fast, model: "gpt-5-mini" }

// several overrides at once; later entries win:
let Debugging = baml.ai.OpenAi {
  ...Fast,
  base_url: "http://localhost:4000/v1",     // proxy for capture
  extra_headers: { "x-trace": run_id },
}
```

Spread is **same-class only**, and that is a feature, not a limitation:
"`Fast` but on Anthropic" is not a field tweak — Anthropic has different
fields, different auth, different option semantics. Cross-vendor moves are a
new declaration, which is exactly the review-visibility they deserve:

```baml
let CarefulVariant = baml.ai.Anthropic { ...Careful, model: "claude-haiku-5" } // ok: same class
```

For families of variants, write a function — providers compose with plain
code:

```baml
function openai_at(tier: string) -> baml.ai.Provider {
  match (tier) {
    "fast"  => baml.ai.OpenAi { ...Fast, model: "gpt-5-mini" },
    "best"  => Fast,
    _       => baml.ai.OpenAi { ...Fast, model: "gpt-5-nano" },
  }
}

let invoice = ExtractInvoice(doc, client = openai_at(tenant.tier))
```

## The `client:` field accepts expressions

The task's `client:` field takes a provider expression: a name, a zero-arg
function call, or a full literal:

```baml
function Route() -> baml.ai.Provider {
  if (baml.env.get("REGION") == "eu") { EuGateway } else { Fast }
}

function Summarize(text: string) -> Summary {
  client: Route()                      // resolved per call
  prompt: `Summarize: ${text}. ${ctx.output_format}`
}

function Research(q: string) -> Answer {
  client: baml.ai.OpenAi { ...Fast, model: "gpt-5-mini" }   // inline literal
  prompt: `Research ${q}. ${ctx.output_format}`
}
```

The `client =` call-site override always wins over the field; both are just
the injected parameter and its default (page 2, step 1).

## Capability interfaces

`Provider` is a marker. What a provider can *do* is stated by which
capability interfaces it implements — and only those:

```baml
class OpenAi {
  ...
  implements baml.ai.Provider {}
  implements baml.ai.Generate { ... }
  implements baml.ai.Streaming { ... }
  implements baml.ai.Tools { ... }
}

class OpenAiResponses {
  ...
  implements baml.ai.Provider {}
  implements baml.ai.Generate { ... }
  implements baml.ai.Background { ... }    // submit/poll lifecycle
  implements baml.ai.Sessions { ... }      // server-stored continuations
}

class OpenAiRealtime {
  ...
  implements baml.ai.Provider {}
  implements baml.ai.Realtime { ... }      // and NOT Generate — no fake call path
}
```

A capability that is absent is absent honestly: calling `.stream` against a
non-streaming client is a typed `baml.errors.Unsupported` naming both the
provider and the capability. There is no universal `call` that everything
must fake.

When you hold a concrete provider, capability methods are directly callable
— no negotiation:

```baml
let m = baml.ai.OpenAi { ...Fast }
let r = m.generate<Invoice>(ExtractInvoice.request(doc, client = m))
let s = m.open_session(baml.ai.SessionOptions {})   // if m implements Sessions
```

When you hold an existential `baml.ai.Provider`, either pass it as a
`client =` (letting the task's driver negotiate) or demand the capability in
your own signature:

```baml
// this function REQUIRES streaming; say so in the type, not at runtime:
function consume(p: baml.ai.Streaming, req: baml.ai.Request<Report>) -> Report {
  p.stream<PartialReport, Report>(req).final()
}
```

## Writing your own provider

Any class that implements `Provider` plus at least one capability is a
provider. A complete, working test fake:

```baml
class FixtureProvider {
  reply: string,

  implements baml.ai.Provider {}

  implements baml.ai.Generate {
    function generate<T>(self, request: baml.ai.Request<T>) -> baml.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      baml.ai.Response<T> {
        value: baml.sap.parse<T>(self.reply),
        meta: baml.ai.Meta { provider: "fixture", model: null, finish_reason: "stop",
                             usage: null, attributes: {}, raw: null },
      }
    }
  }
}

test "extracts the vendor" {
  let r = ExtractInvoice(doc, client = FixtureProvider {
    reply: "{\"vendor\": \"ACME\", \"total\": 12.5, \"currency\": \"USD\"}",
  })
  assert.equal(r.vendor, "ACME")
}
```

A real HTTP provider is the same shape with `baml.http.send` in the middle —
see page 2 step 5. A gateway is the same shape with your company's endpoint.
None of them require compiler changes.

## Wrappers: policy as a provider

Middleware — moderation, tracing defaults, redaction — is a provider that
holds another provider:

```baml
class Guarded {
  inner: baml.ai.Generate,
  policy: Policy,

  implements baml.ai.Provider {}

  implements baml.ai.Generate {
    function generate<T>(self, request: baml.ai.Request<T>) -> baml.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      self.policy.check_input(request.messages())
      let r = self.inner.generate<T>(request.for_provider(self.inner))
      self.policy.check_output(r.value)
      r
    }
  }
}

let invoice = ExtractInvoice(doc, client = Guarded { inner: Fast, policy: Strict })
```

Two rules keep wrappers honest. First, `request.for_provider(self.inner)`
rebinds the request before delegation, so provider-sensitive prompt context
re-renders for the provider that will actually run. Second, a wrapper
implements only the capabilities it can genuinely forward — a `Generate`
wrapper is not automatically `Streaming` or `Background`; claiming a
capability you cannot police through is how policy silently evaporates.
(Reliability wrappers — retry, fallback — have extra rules; page 8.)

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

**Intersection types for requirements** (`p: Provider & Streaming &
Tools`). Desirable, and nothing here precludes it — a signature can already
demand *one* capability (`p: baml.ai.Streaming`). Multi-capability
requirements remain a type-system-level future; until then the pattern is
demand-the-narrowest, negotiate the rest.
