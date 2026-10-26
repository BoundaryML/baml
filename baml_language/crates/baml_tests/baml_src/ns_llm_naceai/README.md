# nace.ai Drex

Use `client: "naceai/drex-max"` or `naceai.Client.new()` to call `POST /v1/systemone` on nace.ai's console. The client resolves `NACE_API_KEY` at invocation time; `model`, `api_key`, `base_url`, `timeout`, and `capture_wire` can be overridden in the constructor. `drex-max` reads text and images; `drex-v2.0` is the text-only model.

```baml
function Urgent(ticket: string) -> bool {
    client: "naceai/drex-max"
    prompt: `
        ${role("instructions")}
        Does this ticket require immediate attention?
        ${role("user")}
        ${ticket}
    `
}
```

Drex speaks the same System One protocol as TypeSafe, so the output mapping follows `typesafeai`: `bool` thresholds a `noul` probability at `>= 0.5`, `float` returns the probability unchanged, enums and finite literal unions become `choice` questions whose `criteria` map option keys to descriptions (or null), and classes become one named question per leaf. Enum and field descriptions supply question instructions, aliases supply option keys and contextual field labels, and skipped nullable fields are reconstructed as null. A request carries at most 16 questions; larger outputs are rejected before HTTP. Drex's ordered-rubric `score` questions have no automatic BAML output mapping.

Question instructions come from `role("instructions")` and `@description`; other prompt messages and journal text are joined into the shared `state` in order. Images are sent as a top-level `images` array of data URLs (at most 8) and cited from the state as `Image 1`, `Image 2`, ... in request order. Remote images are downloaded and inlined when invoking; rendering a request with a remote image raises `ai.errors.PreviewUnsupported` without downloading it. Audio, video, PDF, tools, unbounded output types, and streaming are unsupported. The decoder reads only `type`, `noul`, and `choice`, so the `confidence`, `probabilities`, and array-valued `legend` fields of a reply are ignored; malformed or missing answers fail instead of producing partial results.

The tests use the local provider mock and require no nace.ai credentials. See the [Drex Max guide](https://console.nace.ai/docs/drex/guides/drex-max) for the endpoint's question types, limits, and input requirements.
