# Writing Custom Providers and Capabilities

This page answers three different extension questions:

1. “How do I call my own model endpoint?” Implement `Generate`.
2. “How do I add policy or telemetry around an existing model?” Write a provider wrapper.
3. “How do I add a genuinely new interaction shape?” Declare a capability and an ordinary driver.

The distinction matters. Most customizations are cases 1 or 2 and should not create a new capability.

## Part I: A complete custom provider

Assume Acme exposes this HTTP API:

```text
POST https://llm.acme.test/v1/generate
Authorization: Bearer <key>

{
  "model": "acme-2",
  "messages": [{ "role": "user", "content": "..." }],
  "schema": { ... JSON Schema ... }
}

{
  "id": "req_123",
  "text": "{\"answer\": ...}",
  "usage": { "input": 100, "output": 40 },
  "finish_reason": "stop"
}
```

### Step 1: Define private wire types

These types are details of Acme's API. Application code should not depend on them.

```baml
class AcmeMessageWire {
  role: string,
  content: string,
}

class AcmeUsageWire {
  input: int,
  output: int,
}

class AcmeResponseWire {
  id: string,
  text: string,
  usage: AcmeUsageWire?,
  finish_reason: string?,
}

class AcmeHttpError {
  status: int,
  body: string,

  implements baml.errors.GenerateError {
    function retryable(self) -> bool {
      self.status == 429 || self.status >= 500
    }
    function commit_state(self) -> baml.ai.CommitState {
      if (self.status == 429) {
        baml.ai.CommitState.NotCommitted
      } else {
        baml.ai.CommitState.Unknown
      }
    }
    function is_rate_limit(self) -> bool { self.status == 429 }
    function is_refusal(self) -> bool { false }
    function is_parse_error(self) -> bool { false }
  }
}

class AcmeParseError {
  stage: string,
  data: unknown,

  implements baml.errors.GenerateError {
    function retryable(self) -> bool { false }
    function commit_state(self) -> baml.ai.CommitState {
      baml.ai.CommitState.Committed
    }
    function is_rate_limit(self) -> bool { false }
    function is_refusal(self) -> bool { false }
    function is_parse_error(self) -> bool { true }
  }
}
```

### Step 2: Define the provider class

```baml
class Acme {
  model: string,
  api_key: string,
  base_url: string,

  implements baml.ai.Provider {}
}
```

Implementing `Provider` only says “this object can participate in provider dispatch.” It does not claim any operation yet.

### Step 3: Convert semantic messages to the wire format

`LlmRequest<T>` contains the structured `PromptAst`; `request.messages()` gives a provider-neutral message representation with roles and media preserved.

```baml
function acme_messages(messages: baml.ai.ChatMessage[]) -> AcmeMessageWire[]
  throws baml.errors.UnsupportedPayload {
  messages.map((message) -> {
    let content = match (message.text()) {
      let text: string => text,
      null => throw baml.errors.UnsupportedPayload {
        provider: "acme",
        feature: "non-text message part",
        detail: "Acme /v1/generate accepts text only",
      },
    }

    AcmeMessageWire {
      role: message.role,
      content: content,
    }
  })
}
```

This simple endpoint supports text only. A production implementation MUST reject image/audio parts with a typed unsupported-payload error instead of silently dropping them.

### Step 4: Implement `Generate`

```baml
implements baml.ai.Generate for Acme {
  function generate<T>(
    self,
    request: baml.ai.LlmRequest<T>,
  ) -> baml.ai.LlmResponse<T>
    throws baml.errors.GenerateError | baml.errors.UnknownError {
    let body = baml.json.to_string({
      "model": self.model,
      "messages": acme_messages(request.messages()),
      "schema": baml.json.parse(
        baml.schema.json_schema(reflect.type_of<T>(), true),
      ),
    })

    let http_response = baml.http.send(baml.http.Request {
      method: "POST",
      url: `${self.base_url}/v1/generate`,
      headers: {
        "authorization": `Bearer ${self.api_key}`,
        "content-type": "application/json",
      },
      body: body,
    }) catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["Acme generate transport failed"],
      },
    }

    let raw = http_response.text() catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["Acme response read failed"],
      },
    }

    if (!http_response.ok()) {
      throw AcmeHttpError {
        status: http_response.status_code,
        body: raw,
      }
    }

    let wire = baml.json.from_string<AcmeResponseWire>(raw) catch_all (e) {
      _ => throw AcmeParseError { stage: "wire response", data: e },
    }
    let value = baml.sap.parse<T>(wire.text) catch_all (e) {
      _ => throw AcmeParseError { stage: "typed model output", data: e },
    }

    baml.ai.LlmResponse<T> {
      value: value,
      meta: baml.ai.ResponseMeta {
        provider: "acme",
        model: self.model,
        request_id: wire.id,
        finish_reason: wire.finish_reason,
        usage: wire.usage.map((u) -> {
          baml.ai.Usage {
            input_tokens: u.input,
            output_tokens: u.output,
          }
        }),
        attributes: {},
        raw: baml.json.parse(raw),
      },
    }
  }
}
```

The provider owns request construction, transport, wire decoding, SAP parsing, and metadata normalization. Those are implementation details of `Generate`, not methods imposed on every wrapper or combinator.

### Step 5: Use it with an ordinary LLM function

```baml
let AcmeModel = Acme {
  model: "acme-2",
  api_key: env.ACME_API_KEY,
  base_url: "https://llm.acme.test",
}

function ExtractPerson(text: string) -> Person {
  client: AcmeModel
  prompt: `Extract a person from ${text}. ${ctx.output_format}`
}

let person = ExtractPerson("Ada Lovelace wrote the first algorithm")
```

No compiler plugin, provider registry, or `client<llm>` parser change is required.

### Step 6: Add streaming only if Acme has a real streaming protocol

```baml
implements baml.ai.Streaming for Acme {
  function stream<TPartial, T>(
    self,
    request: baml.ai.LlmRequest<T>,
  ) -> baml.llm.Stream<TPartial, T> {
    // Build Acme's SSE request, decode events, and feed the shared SAP
    // stream accumulator. This method is independent of Generate.generate.
  }
}
```

After that, every existing LLM function can use `$stream` with `client = AcmeModel`. Until then, autocomplete and type matching should not claim Acme is a streaming provider.

## Part II: A wrapper provider

Use a wrapper when the interaction shape stays `LlmRequest<T> -> LlmResponse<T>`.

### Example: input and output moderation

```baml
class Guarded {
  inner: baml.ai.Generate,
  policy: Policy,

  implements baml.ai.Provider {}

  implements baml.ai.Generate {
    function generate<T>(
      self,
      request: baml.ai.LlmRequest<T>,
    ) -> baml.ai.LlmResponse<T> {
      self.policy.check_messages(request.messages())
      let response = self.inner.generate<T>(request.for_provider(self.inner))
      self.policy.check_value(response.value)
      response
    }
  }
}
```

Usage:

```baml
let guarded = Guarded { inner: AcmeModel, policy: StrictPolicy }
let person = ExtractPerson(input, client = guarded)
```

There is no `Moderated` capability because callers do not need a different operation. They want normal generation with a policy.

### Example: default settings

```baml
class WithDefaults {
  inner: baml.ai.Generate,
  temperature: float,
  tags: map<string, string>,

  implements baml.ai.Provider {}

  implements baml.ai.Generate {
    function generate<T>(self, request: baml.ai.LlmRequest<T>) -> baml.ai.LlmResponse<T> {
      self.inner.generate<T>(request
        .with_options(request.options.merge(baml.ai.RequestOptions {
          temperature: self.temperature,
        }))
        .with_tags(self.tags)
        .for_provider(self.inner))
    }
  }
}
```

### Capability forwarding is explicit

If `Guarded` can correctly moderate every streamed partial and the final value, it may separately implement `Streaming`. If it only checks final outputs, it MUST NOT forward `Streaming` as though the stream were guarded.

The same principle applies to background jobs and sessions: forwarding them requires wrapping the returned resource so later `poll`, `run`, or `cancel` operations remain covered by the policy.

## Part III: A genuinely new capability

Assume a vendor offers a special “reviewed generation” endpoint. It returns the usual typed value plus a signed reviewer attestation. That is a different result contract shared by multiple vendor implementations, so a capability is justified.

### Step 1: Define the result and error

```baml
class Attestation {
  reviewer: string,
  policy_version: string,
  signature: string,
}

class Reviewed<T> {
  response: baml.ai.LlmResponse<T>,
  attestation: Attestation,
}

class ReviewError {
  message: string,
  retryable: bool,
}
```

### Step 2: Define the provider-side capability

```baml
interface ReviewedGeneration requires baml.ai.Provider {
  function generate_reviewed<T>(
    self,
    request: baml.ai.LlmRequest<T>,
    policy: string,
  ) -> Reviewed<T> throws ReviewError | baml.errors.UnknownError
}
```

The interface is narrow. It does not include unrelated health, model-listing, or cache methods.

### Step 3: Implement it

```baml
implements ReviewedGeneration for Acme {
  function generate_reviewed<T>(
    self,
    request: baml.ai.LlmRequest<T>,
    policy: string,
  ) -> Reviewed<T> {
    // Call the vendor's reviewed endpoint using request.messages() and
    // reflect.type_of<T>(), parse T, verify the signature, and return both.
  }
}
```

The same provider may also implement normal `Generate`. Capability implementations are independent.

### Step 4: Write the application-facing driver

```baml
function run_reviewed<T>(
  request: baml.ai.LlmRequest<T>,
  policy: string,
) -> Reviewed<T>
  throws ReviewError | baml.errors.Unsupported | baml.errors.UnknownError {
  match (request.provider) {
    let provider: ReviewedGeneration => {
      provider.generate_reviewed<T>(request, policy)
    },
    _ => throw baml.errors.Unsupported {
      capability: "acme.reviewed-generation",
      provider: request.provider_name(),
    },
  }
}
```

This function is the whole “registration” story. It is an ordinary exported function with an ordinary signature.

### Step 5: Use any LLM function

```baml
let reviewed = run_reviewed(
  ExtractPerson$request(input, client = AcmeModel),
  policy = "pii-strict-v3",
)

verify(reviewed.attestation)
save(reviewed.response.value)
```

The custom capability inherits the LLM function's prompt, roles, media, output type, identity, arguments, tags, and default/overridden provider.

### Step 6: Test the pieces separately

```baml
class FakeReviewed {
  implements baml.ai.Provider {}

  implements ReviewedGeneration {
    function generate_reviewed<T>(
      self,
      request: baml.ai.LlmRequest<T>,
      policy: string,
    ) -> Reviewed<T> {
      // Return a fixture parsed as T and a deterministic attestation.
    }
  }
}

test "reviewed driver dispatches" {
  let result = run_reviewed(
    ExtractPerson$request("Ada", client = FakeReviewed {}),
    "test-policy",
  )
  assert.equal(result.attestation.policy_version, "test-policy")
}

test "reviewed driver reports missing capability" {
  let result = run_reviewed(
    ExtractPerson$request("Ada", client = PlainFake {}),
    "test-policy",
  ) catch (e) {
    let _: baml.errors.Unsupported => "unsupported",
    _ => "wrong error",
  }
  assert.equal(result, "unsupported")
}
```

## Part IV: Direct provider APIs without a capability

Not every operation needs a reusable framework abstraction.

### One application, one provider

```baml
let result = AcmeModel.generate_with_experimental_decoder<Person>(
  ExtractPerson$request(input, client = AcmeModel),
  decoder = "acme-beam-v2",
)
```

This is acceptable. The LLM function still owns the task, and the provider owns the experimental operation. Promote it to a capability only when a shared contract becomes valuable.

### No LLM task at all

```baml
let health = AcmeModel.health()
let models = AcmeModel.list_models()
let upload = AcmeModel.upload_training_file(path)
```

Do not create fake `LlmRequest<Health>` values for provider administration.

## Static APIs versus dynamic APIs

If your library only works with reviewed providers, say so in its signature:

```baml
function audited_extract(
  provider: ReviewedGeneration,
  input: string,
) -> Reviewed<Person> {
  provider.generate_reviewed<Person>(
    ExtractPerson$request(input, client = provider),
    "audit-v1",
  )
}
```

If the provider is chosen at runtime, accept `Provider` through `LlmRequest<T>` and let the driver return typed `Unsupported`.

Use static capability types at stable internal boundaries. Use dynamic drivers at application routing boundaries.

## Naming guidance

Capability interfaces should be nouns or adjectives describing behavior: `Streaming`, `Background`, `ReviewedGeneration`.

Provider methods should describe the primitive operation: `stream`, `submit`, `generate_reviewed`.

Drivers should describe the user action: `run`, `submit_background`, `run_reviewed`.

Resource methods should describe lifecycle transitions: `poll`, `cancel`, `fork`, `compact`, `close`, `token`, `cleanup`.

Avoid capability names that are really products (`AcmeV2`), settings (`Temperature`), or transport details (`JsonPost`).

## Checklist for a provider pull request

- Does the class implement only capabilities actually supported?
- Does it preserve roles and media, or reject unsupported content explicitly?
- Does it use `reflect.type_of<T>()` for native schemas and SAP for the typed result?
- Does it normalize common metadata without discarding provider-specific data?
- Are transport and parse errors classified separately?
- Does every stateful result retain its provider owner?
- Are resource tokens free of credentials?
- Is replay safety declared per operation?
- Are there offline wire-shape tests?
- Are live tests opt-in and keyed by provider/model?
- Can an ordinary LLM function use the provider with a client override?
