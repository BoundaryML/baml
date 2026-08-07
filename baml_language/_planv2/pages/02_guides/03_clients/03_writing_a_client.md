# Writing a client

## The anatomy

A client is a configuration class, a pure render function, a shared
transport call, and a pure parse function:

```baml
class MyLabClient {
    model: string,
    api_key: string,
    base_url: string?,

    implements Client {
        function id(self) -> string { `mylab/${self.model}` }
        function invoke(self, input: ModelTurnInput) -> ModelTurn {
            let req: baml.http.Request = mylab_render(self, input);
            let resp: MyLabResponse = ai.wire.send_as<MyLabResponse>(req, "mylab");
            mylab_parse(resp)
        }
    }
}
```

`mylab_render` and `mylab_parse` are ordinary functions. Neither
touches the network, so both are testable with literal inputs.
`MyLabResponse` is an envelope class you declare for the fields your
parse reads; open-ended parts stay `json` fields, and `send_as<json>`
returns the undecoded body.

## Rendering the prompt

The prompt renders with the output-format text your client chooses:

```baml
function mylab_render(c: MyLabClient, input: ModelTurnInput) -> baml.http.Request {
    let schema_text = ai.wire.render_output_format(input.output_type);
    // render_text suffices for a text-only wire API; a client that
    // accepts media arguments walks prompt.render's parts instead
    let instructions: string = input.prompt.render_text(schema_text);
    // the system message first, then the lowered journal
    let messages = [{ "role": "system", "content": instructions }]
        .concat(mylab_transcript(input.journal));
    baml.http.Request {
        method: "POST",
        url: `${c.base_url ?? "https://api.mylab.ai"}/v1/chat`,
        headers: {
            "authorization": `Bearer ${c.api_key}`,
            "content-type": "application/json",
        },
        body: baml.json.stringify({
            "model": c.model,
            "messages": messages,
            "tools": input.toolbox.list().map((t) -> { t.input_schema }),
        }),
    }
}
```

A client that places the contract on the wire instead passes an empty
string to `render` and sets its API's schema field in the body. On a
first turn the journal is empty; if the wire API requires a user
message, lower the instructions as the sole user message on that turn.

## Lowering tools

Each `Tool` carries its `input_schema`, derived from the signature
through `baml.schema.json_schema` when the tool is constructed. Your
render function reads the field and wraps it in the wire API's shape. If the API constrains schemas —
closed objects, all-required properties — apply the shared rewrites
(`wire.closed_schema`, `wire.strict_schema`) rather than writing a
walker.

## Lowering the journal

The transcript function folds journal events into native messages:

- `RunStarted` and other journal-only events are skipped.
- An `AssistantMessage` lowers from its content blocks: `Text` blocks
  as assistant text, `ToolUse` blocks in the API's tool-call shape.
  Blocks produced by a different client lower the same way; canonical
  content is client-neutral.
- `ToolCompleted` and `ToolFailed` lower as the API's tool-result
  shape, correlated by call id. They are results for calls already
  present in an `AssistantMessage`, so they must not be duplicated as
  call items.
- `ToolRequested` is a projection of an `AssistantMessage` block and
  lowers to nothing.

If your wire API sends no call ids, synthesize deterministic ids from
the journal position when parsing the response, and translate back
when lowering results.

## The wire library

`ai.wire` holds what every client needs and no client should
rewrite:

| Helper | Behavior |
|---|---|
| `send_as<T>(req, provider)` | `baml.http.send`, classify non-2xx via `classify_http`, decode the body as `T`, throw `ParseFailed` on garbage; `T = json` skips typing |
| `render_output_format(t)` | the schema as prompt text in the standard dialect |
| `closed_schema(s)` | `additionalProperties: false` recursively, `required` preserved |
| `strict_schema(s)` | closed plus every property required |

The helpers sit on `baml.schema.json_schema` and `baml.http`; a client
that needs different behavior drops to those primitives. What remains
per-client is exactly what differs by design: the wire shapes, the
role mapping, and the API's structured-output mechanism.

## Testing a client

Test the pure functions with literal inputs — a journal in, a request
out; a response body in, a turn out. No network and no loop:

```baml
test mylab_lowers_tool_results {
    let j = journal_with(
        assistant_turn([ToolUse { id: "call_1", name: "search_flights", args: {...} }]),
        ToolCompleted { id: "call_1", output: [...] },
    );
    let req = mylab_render(test_client(), input_with(j));
    assert.contains(req.body, "\"tool_result\"");
}
```
