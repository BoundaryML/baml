# Provider-owned tools

Some tools run inside the provider service rather than inside the BAML
application.

## Utilities used

| Utility | Purpose |
| --- | --- |
| Provider tool configuration | Enables hosted search or code execution |
| `CompletionProvider` | Runs one bounded provider-owned operation |
| `tools:` | Separately declares application functions |

## Example

```baml
let ResearchModel = ai.OpenAi {
  model: "gpt-5.6-luna",
  api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
  base_url: null,
  extra_headers: null,
  extra_body: null,
  provider_tools: [
    ai.OpenAiWebSearch {},
  ],
}

class Report {
  summary: string,
  sources: string[],
}

function Research(topic: string) -> Report {
  provider: ResearchModel
  prompt: `
    Research this topic using hosted web search.

    ${topic}

    ${ctx.output_format}
  `
}

let report: Report = Research("recent battery recycling methods")
```

OpenAI executes the web search. BAML sees a bounded provider completion and
the final `Report`.

If the function also declares a BAML function in `tools:`, the BAML Agent loop
handles that application tool while the provider remains responsible for its
hosted tools. The two rosters never share an execution owner.

[Back to tools and agents](../tools-and-agents.md)
