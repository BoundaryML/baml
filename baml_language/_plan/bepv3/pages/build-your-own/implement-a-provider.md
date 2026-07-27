# Implement a provider

A provider implements common identity plus the capability interfaces it can
honestly execute.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.Provider` | Prompt rendering and stable identity |
| `GenerationProvider` | Exactly one model interaction |
| `ToolCallingProvider` | Provider turns for the BAML Agent |

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

class AcmeProvider {
  model: string,
  api_key: () -> string,
  send: (AcmeRequest) -> AcmeResponse throws AcmeError,

  implements ai.Provider {
    function descriptor(self) -> ai.ProviderDescriptor {
      ai.ProviderDescriptor {
        family: "acme",
        model: self.model,
      }
    }
  }

  implements ai.GenerationProvider {
    function generate<T>(
      self,
      task: ai.Task<T>,
    ) -> ai.Response<T> {
      acme_generate(self, task)
    }
  }
}

let answer = AnswerQuestion(
  "Why is the sky blue?",
  $provider = AcmeProvider {
    model: "acme-large",
    api_key: () -> string {
      baml.env.get_or_panic("ACME_API_KEY")
    },
    send: acme_http_send,
  },
)
```

Implementing generation does not claim streaming, realtime, background work,
or application tool turns. Add those interfaces only when the adapter
implements their full contracts.

[Back to build your own](../build-your-own.md)
