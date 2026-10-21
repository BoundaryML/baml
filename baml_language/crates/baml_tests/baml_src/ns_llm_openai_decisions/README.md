# OpenAI Decisions

Use `client: "openai-decisions/gpt-6-luna"` or `openai.DecisionsClient.new()` to call `POST /v1/decisions`. The client resolves `OPENAI_API_KEY` at invocation time; `api_key`, `base_url`, `timeout`, and `capture_wire` can be overridden in the constructor.

```baml
function Urgent(ticket: string) -> bool {
    client: "openai-decisions/gpt-6-luna"
    prompt: `
        ${role("instructions")}
        Does this ticket require immediate attention?
        ${role("user")}
        ${ticket}
    `
}
```

The output mapping follows `typesafeai`: `bool` uses a predicate probability threshold of `>= 0.5`, `float` returns the predicate probability unchanged, enums and finite literal unions use choices, and classes become one named question per leaf. Enum and field descriptions supply question instructions, aliases supply choice values and contextual field labels, and skipped nullable fields are reconstructed as null. Choices are sent as strings, including numeric literals and the `<null>` sentinel, then decoded to the original BAML values. A float represents a probability; the API's ordered-rubric `score` questions have no automatic BAML output mapping.

Question instructions come from `role("instructions")` and `@description`; other prompt messages and journal text supply shared evidence in order. Inputs support text and images. Remote images are downloaded and sent as inline base64 data URLs when invoking; rendering a request with a remote image raises `ai.errors.PreviewUnsupported` without downloading it. Audio, video, PDF, tools, unbounded output types, and streaming are unsupported. Model refusals raise `ai.errors.Refused`; malformed or missing answers fail instead of producing partial results.

The tests use the local provider mock and require no OpenAI credentials. See the [official Decisions guide](https://developers.openai.com/api/docs/guides/decisions) for the endpoint's question types and input requirements.
