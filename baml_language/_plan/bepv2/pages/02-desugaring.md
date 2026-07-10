# 2. What a Task Desugars To

Nothing in a task is magic. This page removes every layer of sugar, in
order, until you can see the wire. An SDE1 should be able to debug a task
call from this page alone.

## The declaration

```baml
class Invoice {
  vendor: string,
  total: float,
  currency: string,
}

function ExtractInvoice(document: pdf) -> Invoice {
  client: AccurateModel
  prompt: `
    ${role("system")} You extract invoices precisely.
    ${role("user")} Extract this invoice: ${document}
    ${ctx.output_format}
  `
}
```

Three declarative fields. `client` names a default provider. `prompt` is a
template. The return type `Invoice` will be used twice: as the type of the
call expression and as the schema rendered by `${ctx.output_format}`.

## Step 1 — the injected `client` parameter

The compiler appends a synthetic trailing parameter to the task and every
modifier:

```baml
function ExtractInvoice(
  document: pdf,
  client: baml.ai.Provider = AccurateModel,
) -> Invoice
```

That is the whole mechanism behind call-site swapping: `ExtractInvoice(doc,
client = Cheap)` is passing an ordinary named argument. The parameter's type
is the existential marker `baml.ai.Provider`, so anything implementing it —
a built-in provider, your own class, a wrapper — is accepted. (The name
`client` is reserved on tasks; declaring your own parameter with that name
is an error.)

## Step 2 — the prompt is a lazy template

The backtick template does **not** evaluate where it is written. `prompt`
templates have the conceptual type:

```baml
type PromptTemplate = (baml.llm.Context) -> baml.llm.PromptAst
```

They are lazy for two reasons:

1. `${ctx.output_format}` depends on the return type `T` — the runtime
   renders the schema string from `Invoice` and hands it to the template
   through `ctx`.
2. Prompt context can be provider-sensitive; the template must render
   *after* the provider for this attempt is chosen (this matters for
   fallback, page 8).

`PromptAst` is the rendered result: an opaque structural value that
preserves roles and media parts. It is never sent to a provider directly;
providers view it as messages (step 4).

You can hold a template yourself; this is the manual layer under tasks:

```baml
let template = prompt`
  ${role("user")} Extract this invoice: ${document}
  ${ctx.output_format}
`
// template : (baml.llm.Context) -> baml.llm.PromptAst
```

## Step 3 — the plain call is `run` over `.request`

The body of a task lowers to two calls:

```baml
// what you wrote:
let invoice = ExtractInvoice(scan)

// what runs:
let invoice = baml.ai.run<Invoice>(
  ExtractInvoice.request(scan, client = AccurateModel),
)
```

`ExtractInvoice.request(...)` renders the template with a context built from
`Invoice` and the chosen provider, and packages the result:

```baml
baml.ai.Request<Invoice> {
  provider: AccurateModel,
  prompt:   <rendered PromptAst>,
  identity: TaskIdentity { name: "ExtractInvoice", ... },
  options:  RequestOptions { ... },
  tags:     {},
}
```

No I/O has happened. A request is "one invocation that has not run" — you
can log it, inspect `request.messages()`, hand it to a session, or discard
it.

Every modifier lowers the same way, differing only in the driver:

```baml
ExtractInvoice.stream(scan)      ==>  baml.ai.stream<PartialInvoice, Invoice>(ExtractInvoice.request(scan, ...))
ExtractInvoice.with_meta(scan)   ==>  baml.ai.run_with_meta<Invoice>(ExtractInvoice.request(scan, ...))
ExtractInvoice.background(scan)  ==>  baml.ai.submit_background<Invoice>(ExtractInvoice.request(scan, ...), options)
ExtractInvoice.agent(scan)       ==>  baml.ai.run_agent<Invoice>(ExtractInvoice.request(scan, ...))
```

One seam, many consumers. This is why a custom execution mode needs no
compiler support — it is just one more consumer of the same value.

**One branch in the lowering:** if the task declares a `tools:` field, the
request carries the roster (`request.tools`) and the *plain call* lowers
through `run_agent` in graceful-finish mode instead of `run` — a tool task's
plain call runs the loop. Which modifiers are valid on a tool task, and with
what semantics, is defined by the normative matrix on page 5; the lowerings
above are the no-tools column.

## Step 4 — the driver negotiates capabilities

`baml.ai.run` is an ordinary stdlib function, roughly:

```baml
function run<T>(request: Request<T>) -> T
    throws baml.errors.CallError | baml.errors.UnknownError {
  match (request.provider) {
    let g: Generate => g.generate<T>(request).value,
    _ => throw baml.errors.Unsupported {
      message: "client cannot generate: " + request.provider_name(),
    },
  }
}
```

This `match` is the *only* place capability negotiation happens for a plain
call. If the provider implements `Generate`, its method runs; if not, you
get a typed `Unsupported` naming the provider and the missing capability —
not a crash, not a silent degrade.

Drivers are the third layer of the surface rule (README): when application
code holds a *concrete* provider it can skip the driver and call
`g.generate<T>(req)` directly, because the capability is statically known.

## Step 5 — the provider does the wire work

A provider's `generate` is ordinary BAML. The built-in OpenAI provider,
abridged:

```baml
class OpenAi {
  model: string,
  api_key: string,
  base_url: string?,
  extra_headers: map<string, string>?,
  extra_body: map<string, unknown>?,

  implements baml.ai.Provider {}

  implements baml.ai.Generate {
    function generate<T>(self, request: baml.ai.Request<T>) -> baml.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      let messages = request.messages()                 // structural view: roles + media
      let schema = baml.llm.render_output_format(request.output_type())
      let http = self._build_chat_request(messages, schema)   // private codec helper
      let body = baml.http.send(http)                          // host transport
      let value: T = baml.sap.parse<T>(self._content_of(body)) // schema-aligned parse
      baml.ai.Response<T> { value: value, meta: self._meta_of(body) }
    }
  }
}
```

Note what lives where:

- `request.messages()` crosses the one bridge from `PromptAst` to
  `ChatMessage[]` — roles and media survive structurally; nothing is
  flattened to a string.
- Schema strategy is the provider's choice: this one renders a schema
  string; a strict provider sends native JSON-schema `response_format`; a
  constrained decoder compiles a grammar. All are `Generate`, because the
  observable shape is unchanged.
- `_build_chat_request` / `_content_of` / `_meta_of` are private helpers.
  HTTP codecs are implementation details, not capabilities — a wrapper or a
  local model implements `generate` without them.

## Step 6 — the response comes back up

`generate` returns `Response<T> { value, meta }` — metadata is produced
exactly once, on every call. The plain call's driver drops `meta`;
`.with_meta` keeps it. Nothing ever issues a second model call just to read
usage.

## The complete trace, end to end

```text
ExtractInvoice(scan, client = Cheap)
  = baml.ai.run<Invoice>(ExtractInvoice.request(scan, client = Cheap))
      ExtractInvoice.request:
        ctx     = Context { output_format: render_output_format(Invoice), ... }
        prompt  = template(ctx)                        # lazy template renders HERE
        request = Request<Invoice> { provider: Cheap, prompt, identity, ... }
      baml.ai.run:
        match provider → Generate                      # negotiation, once
        Cheap.generate<Invoice>(request)
          messages = request.messages()                # PromptAst → ChatMessage[]
          http     = build + send                      # provider-private codec
          value    = sap.parse<Invoice>(content)       # response → typed value
          Response { value, meta }
        .value                                         # run drops meta
```

Six steps, each one an ordinary function you can read in the stdlib source.
`baml describe ExtractInvoice` lists the task, its modifiers, the request
signature, and the default client's capability interfaces.

## Alternatives considered

**Lower the plain call directly to a provider method** (skip the request).
Rejected: then streaming, metadata, background, sessions, and every custom
mode each need their own private rendering path, and "the same rendered
invocation" stops being guaranteed across modes. One seam keeps the
prompt-render, schema, and identity provably identical however the task
runs.

**Eager prompt rendering at the call site.** Rejected: `${ctx.output_format}`
needs `T` and provider-sensitive context needs the chosen provider; eager
rendering either forbids fallback re-rendering or renders wrong.

**Flatten prompts to strings at the provider boundary.** Rejected: roles and
media must survive to the wire; a text-only provider must *reject* an image
part with a typed error, which it can only do if the part still exists.

**A provider interface of codec stages** (`build_request` / `send` / `parse`
as the capability). Rejected: the stages are meaningless for wrappers, local
models, and test fixtures, which then implement fake methods; one semantic
`generate` keeps the contract at the level every provider actually shares.
Codec stages survive as optional shared helpers for HTTP providers.
