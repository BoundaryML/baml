# Configure a provider

The provider value selected by an LLM function owns provider-specific
configuration.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.OpenAi` | Configured OpenAI provider value |
| `provider:` | Selects the provider for an LLM function |

## Example

```baml
let SupportModel = ai.OpenAi {
  model: "gpt-5.6-luna",
  api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
  base_url: null,
  extra_headers: {
    "X-Application": "support",
  },
  extra_body: null,
}

class Resolution {
  reply: string,
  resolved: bool,
}

function ResolveTicket(message: string) -> Resolution {
  provider: SupportModel
  prompt: `
    Resolve this support ticket.

    ${message}

    ${ctx.output_format}
  `
}

let resolution = ResolveTicket("Where is order order-42?")
```

The provider owns the model, endpoint, authentication, headers, and native
provider settings. The LLM function owns its arguments, prompt, return type,
and default application tools.

Factories may use default function arguments to make optional provider
settings concise. Class fields themselves do not have defaults; every
constructed provider value is complete.

[Back to tasks and runners](../tasks-and-runners.md)
