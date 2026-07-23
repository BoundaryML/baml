# Compiler and Runtime Expansion

This page shows what the framework generates. The exact compiler IR may differ, but the observable signatures and evaluation order are normative.

## Source declaration

```baml
class Invoice {
  vendor: string,
  total: float,
}

function ExtractInvoice(document: pdf, locale: string) -> Invoice {
  client: InvoiceModel
  prompt: `
    ${role("system")}
    Extract invoices using locale ${locale}.

    ${role("user")}
    ${document}

    ${ctx.output_format}
  `
}
```

The parser records:

- original parameters: `document`, `locale`;
- return type: `Invoice`;
- default client expression: `InvoiceModel`;
- prompt tagged-template body;
- function identity and source map.

## Generated public signatures

The compiler exposes a fixed family:

```baml
function ExtractInvoice(
  document: pdf,
  locale: string,
  client: baml.ai.Provider = InvoiceModel,
) -> Invoice

function ExtractInvoice$request(
  document: pdf,
  locale: string,
  client: baml.ai.Provider = InvoiceModel,
) -> baml.ai.LlmRequest<Invoice>

function ExtractInvoice$stream(
  document: pdf,
  locale: string,
  client: baml.ai.Provider = InvoiceModel,
) -> baml.llm.Stream<Invoice$stream, Invoice>

function ExtractInvoice$render_prompt(
  document: pdf,
  locale: string,
  client: baml.ai.Provider = InvoiceModel,
) -> baml.llm.PromptAst

function ExtractInvoice$parse(raw: string) -> Invoice
```

Only `$request` is the general execution extension point. `$stream`, `$render_prompt`, and `$parse` remain standard conveniences/tooling surfaces.

## Lowered `$request`

Conceptually:

```baml
function ExtractInvoice$request(
  document: pdf,
  locale: string,
  client: baml.ai.Provider = InvoiceModel,
) -> baml.ai.LlmRequest<Invoice> {
  let context = baml.ai.build_prompt_context<Invoice>(client)

  let render = prompt`
    ${role("system")}
    Extract invoices using locale ${locale}.

    ${role("user")}
    ${document}

    ${ctx.output_format}
  `

  let ast = render(context)

  baml.ai.LlmRequest<Invoice> {
    provider: client,
    prompt: ast,
    identity: baml.ai.LlmFunctionIdentity {
      package: "app",
      namespace: "root",
      name: "ExtractInvoice",
    },
    arguments: {
      "document": document,
      "locale": locale,
    },
    options: baml.ai.RequestOptions {},
    tags: {},
  }
}
```

Important evaluation rules:

1. User arguments evaluate left to right once.
2. The client/default evaluates once.
3. Prompt context is built for that selected client and `Invoice`.
4. Interpolations evaluate once in source order.
5. `${ctx.output_format}` is rendered from `Invoice`.
6. No provider I/O occurs.

## How the tagged template works

The source:

```baml
prompt`hello ${name} ${ctx.output_format}`
```

is conceptually a call to a tagged function. The parser separates literal slices and expressions:

```text
parts  = ["hello ", " ", ""]
values = [name, ctx.output_format]
```

The tag produces a closure:

```baml
(ctx: baml.llm.Context) -> baml.llm.PromptAst
```

Role markers such as `${role("system")}` create structural message boundaries. Media values stay typed values in the AST. They are not stringified.

The closure is invoked only after the request knows its provider and output type. This is why `prompt` is preferable to a raw backtick string for manual requests.

## `PromptAst` to provider messages

`LlmRequest.messages()` uses the one host/runtime leaf conversion:

```text
PromptAst
  -> ChatMessage[]
     - role
     - ordered MessagePart[]
       - text
       - image
       - audio
       - pdf
       - video
```

Everything after this conversion can be written in BAML. A provider chooses how to encode semantic messages into its wire protocol.

The request retains the `PromptAst` even after producing messages because inspection, provider-specific prompt lowering, and future prompt transformations may need structural information.

## Lowered main function

```baml
function ExtractInvoice(
  document: pdf,
  locale: string,
  client: baml.ai.Provider = InvoiceModel,
) -> Invoice {
  baml.ai.run<Invoice>(
    ExtractInvoice$request(document, locale, client = client),
  )
}
```

The main function does not know about HTTP, provider names, fallback strategies, or custom capabilities.

## `run` dispatch

```baml
function run_with_meta<T>(request: LlmRequest<T>) -> LlmResponse<T> {
  match (request.provider) {
    let provider: Generate => provider.generate<T>(request),
    _ => throw baml.errors.Unsupported {
      capability: "baml.ai.Generate",
      provider: request.provider_name(),
    },
  }
}

function run<T>(request: LlmRequest<T>) -> T {
  run_with_meta<T>(request).value
}
```

If the design accepts “drain a stream as generation,” that is an explicit second match arm with documented metadata/error semantics. It is not implicit duck typing.

## Provider execution

For an HTTP provider:

```text
Generate.generate<T>(LlmRequest<T>)
  -> request.messages()
  -> provider wire encoding
  -> baml.http.send
  -> provider wire response decoding
  -> baml.sap.parse<T>(model text)
  -> normalize ResponseMeta
  -> LlmResponse<T>
```

For a local provider:

```text
Generate.generate<T>(LlmRequest<T>)
  -> request.messages()
  -> host local-inference primitive
  -> baml.sap.parse<T>
  -> LlmResponse<T>
```

The public capability is the same because the semantic operation is the same.

## Lowered stream companion

```baml
function ExtractInvoice$stream(
  document: pdf,
  locale: string,
  client: baml.ai.Provider = InvoiceModel,
) -> baml.llm.Stream<Invoice$stream, Invoice> {
  baml.ai.stream<Invoice$stream, Invoice>(
    ExtractInvoice$request(document, locale, client = client),
  )
}
```

The compiler supplies both type arguments. The stdlib driver dispatches:

```baml
function stream<TPartial, T>(request: LlmRequest<T>) -> baml.llm.Stream<TPartial, T> {
  match (request.provider) {
    let provider: Streaming => provider.stream<TPartial, T>(request),
    _ => throw baml.errors.Unsupported {
      capability: "baml.ai.Streaming",
      provider: request.provider_name(),
    },
  }
}
```

## Lowered render helper

```baml
function ExtractInvoice$render_prompt(...) -> baml.llm.PromptAst {
  ExtractInvoice$request(...).prompt
}
```

There is one prompt-rendering implementation. Preview, execution, streaming, background submission, and custom modes all consume the same AST.

## Lowered manual request

User code:

```baml
let request = baml.ai.request<Invoice>(
  provider,
  prompt`Extract ${document}. ${ctx.output_format}`,
)
```

Conceptually:

```baml
let template: (baml.llm.Context) -> baml.llm.PromptAst = prompt`...`
let context = baml.ai.build_prompt_context<Invoice>(provider)

let request = baml.ai.LlmRequest<Invoice> {
  provider: provider,
  prompt: template(context),
  identity: null,
  arguments: {},
  options: {},
  tags: {},
}
```

Manual requests have no LLM-function identity unless the caller supplies one. They still retain `T` and structured prompt content.

## Custom capability dispatch

User code:

```baml
run_reviewed(
  ExtractInvoice$request(document, "en-US", client = ReviewedAcme),
  "finance-v3",
)
```

Compiler work ends after `$request`. `run_reviewed` is type-checked and lowered like an ordinary generic BAML function:

```text
run_reviewed<Invoice>(LlmRequest<Invoice>, string)
  -> match provider as ReviewedGeneration
  -> provider.generate_reviewed<Invoice>(request, policy)
```

Adding `ReviewedGeneration` does not cause the compiler to revisit every LLM function.

## Background dispatch and resource construction

```text
ReviewRepository$request(...) -> LlmRequest<RepositoryReview>
submit_background<RepositoryReview>(request, options)
  -> match provider as Background
  -> provider.submit<RepositoryReview>
  -> OpenAiResponseJob<RepositoryReview>
  -> erase concrete class to Job<RepositoryReview>
```

Later:

```text
Job<RepositoryReview>.poll()
  -> dynamic dispatch to OpenAiResponseJob.poll
  -> GET provider response ID
  -> map status
  -> on success SAP parse RepositoryReview
  -> JobSucceeded<RepositoryReview>
```

The provider match occurs once at submission. Polling dynamically dispatches on the resource implementor, so the application never repeats capability negotiation.

## Client strategies

Fallback and round-robin need to choose whether request rendering occurs before or after member selection.

The normative rule is:

- if prompt context is provider-independent, a wrapper may render once and forward the request;
- if `${ctx.client...}` or provider prompt specialization is used, the wrapper MUST choose a member first and render for that member;
- a request bound to member A cannot be sent to member B without an explicit `request.for_provider(B)` re-render operation.

Therefore `LlmRequest.for_provider` cannot merely replace one field. It rebuilds provider-sensitive prompt context from the retained template recipe. An implementation stores a private render closure/recipe in the request for this purpose.

## Internal representation

A practical runtime representation may be:

```text
LlmRequest {
  provider_value,
  final_type,
  prompt_ast,
  render_recipe?,
  function_definition_id?,
  captured_arguments,
  options,
  tags,
  source_map,
}
```

`render_recipe` is private because it contains a closure and captured values that may not cross host/process boundaries. `prompt_ast` is the concrete rendered form used for execution.

## Host code generation

For each LLM function, generators expose:

- the normal typed call;
- the normal stream call;
- a typed request builder;
- render/parse debug helpers as appropriate.

They do not generate one method per installed capability. A capability package exposes host helpers that accept the generated request wrapper.

The request wrapper needs an opaque runtime handle because `PromptAst`, provider values, closures, and BAML `type` values are not plain JSON. It may be serializable only after an explicit conversion to a durable task envelope.

## Name resolution and diagnostics

`$request` is compiler-owned. User declarations with the same generated name are errors.

Useful diagnostics include:

```text
E0XXX: `ReviewRepository` requires `baml.ai.Generate`, but provider `VoiceOnly`
does not implement it.

help: use a provider implementing `Generate`, call `ReviewRepository$request(...)`
with a different capability driver, or add a `Generate` wrapper.
```

For custom drivers, the ordinary `match` exhaustiveness/type diagnostics apply. No custom marker diagnostics are required.

`baml describe ReviewRepository` SHOULD show:

- source signature and prompt;
- injected provider parameter;
- generated `$request` signature;
- standard companions;
- return and stream-partial types;
- default provider;
- links to `LlmRequest`, `Generate`, and `Streaming`.

## Compilation and dead code

The generated function count is `O(number of LLM functions)` with a small constant. It does not depend on the number of user capability libraries in the package graph.

Ordinary driver functions participate in normal reachability and dead-code elimination. An unused custom capability has no per-LLM-function bytecode cost.

## Trace structure

A trace SHOULD distinguish:

```text
LLM task span: ExtractInvoice
  request render span
  capability driver span: baml.ai.run
  provider attempt span: OpenAi.generate
    HTTP span
    parse span
```

For background execution:

```text
LLM task span: ReviewRepository
  request render span
  background submit span

later trace, linked by job token/remote ID:
  background poll span
  parse final RepositoryReview span
```

The semantic LLM-function identity remains stable even though execution spans multiple processes.
