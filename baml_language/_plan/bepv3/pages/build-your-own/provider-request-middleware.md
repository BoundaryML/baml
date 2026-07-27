# Add provider request middleware

Typed middleware may transform a concrete provider request before transport.

## Utilities used

| Utility | Purpose |
| --- | --- |
| Typed function field | Transforms one request type |
| Default function argument | Supplies identity middleware |

## Example

```baml
class Answer {
  text: string,
}

function AnswerQuestion(question: string) -> Answer {
  provider: AcmeModel
  prompt: `
    Answer this question.

    ${question}

    ${ctx.output_format}
  `
}

function add_tenant_header(
  request: AcmeRequest,
) -> AcmeRequest {
  request.headers.set("X-Tenant", "tenant-42");
  request
}

let provider = acme_provider(
  model = "acme-large",
  get_api_key = load_acme_key,
  send = acme_http_send,
  modify_request = add_tenant_header,
);

let answer = AnswerQuestion(
  "What is our return policy?",
  $provider = provider,
)
```

Middleware stays provider-specific and typed. Cross-provider lifecycle policy,
such as semantic retry or fallback, remains a runner.

[Back to build your own](../build-your-own.md)
