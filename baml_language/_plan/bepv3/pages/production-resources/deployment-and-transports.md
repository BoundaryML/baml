# Deployment and transports

Endpoints, headers, authentication, and injected transports belong to provider
configuration.

## Utilities used

| Utility | Purpose |
| --- | --- |
| Provider fields | Configure production deployment |
| Function-valued transport | Injects HTTP, proxy, or test behavior |
| Request middleware | Transforms provider requests |

## Example

```baml
let ProductionModel = ai.OpenAi.new(
  model = "gpt-5.6-luna",
  api_key = () -> string {
    baml.env.get_or_panic("OPENAI_API_KEY")
  },
  base_url = "https://llm-gateway.example.com/v1",
  headers = {
    "X-Application": "support",
  },
)

class Resolution {
  reply: string,
}

function ResolveTicket(message: string) -> Resolution {
  provider: ProductionModel
  prompt: `
    Resolve this support ticket.

    ${message}

    ${ctx.output_format}
  `
}

let resolution = ResolveTicket("Where is order-42?")
```

The task stays unchanged across local, gateway, proxy, and test deployments.
Secrets captured by function fields do not become task arguments or
model-visible data.

[Back to production resources](../production-resources.md)
