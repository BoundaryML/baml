# Inject a provider transport

Function fields make authentication and transport replaceable without a
second provider hierarchy.

## Utilities used

| Utility | Purpose |
| --- | --- |
| Function fields | Capture typed behavior |
| Closures | Capture credentials without exposing them to the model |
| `baml.AnyFunction` | Erases only where heterogeneous callables are required |

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

let production = acme_provider(
  model = "acme-large",
  get_api_key = () -> string {
    baml.env.get_or_panic("ACME_API_KEY")
  },
  send = acme_http_send,
);

let fake = acme_provider(
  model = "fake",
  get_api_key = () -> string { "test-key" },
  send = (request: AcmeRequest) -> AcmeResponse {
    AcmeResponse.fake_answer({ "text": "Rayleigh scattering." })
  },
);

let answer = AnswerQuestion(
  "Why is the sky blue?",
  $provider = fake,
)
```

The fake replaces one function instead of reimplementing the full provider
protocol. The interface remains useful for generic multi-operation behavior;
function values handle simple injected policy.

[Back to build your own](../build-your-own.md)
