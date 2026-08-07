# Models

Model access goes through clients. This page explains what a client is
and what it does with your journal.

## Design status

The client boundary on this page is provisional. Research into pi's API
adapters and the OpenAI Responses and Conversations APIs exposed unresolved
differences between canonical content, exact same-API replay data, remote
response cursors, and durable remote conversations. Do not implement the
interface on this page without reviewing
`../05_appendix/03_client_replay_and_continuations.md`.

This page records the current BEP and reference implementation. The appendix
records the problem, external evidence, candidate replacements, required test
scenarios, and open decisions for the redesign.

## What a client is

A client is a `client<llm>` declaration — the same one BAML has today:

```baml
client<llm> Planner {
    provider: openai,
    options: { model: "gpt-5.2", api_key: env.OPENAI_API_KEY },
}
```

This BEP defines what a client *is* underneath: a stateless codec plus
transport for one provider wire format. `provider:` names the wire format
the client speaks. A client holds configuration — model, key, retry
policy. It never holds conversation state. Every request is built fresh
from the journal.

## The three duties

```baml
interface Client {
    function id(self) -> string    // e.g. "openai/gpt-5.2"; stamped on events it produces

    // 1. RENDER: journal -> provider-native request (the transcript)
    function render(self, j: Journal, tb: Toolbox, output_schema: string) -> ProviderRequest

    // 2. TRANSPORT: one stateless call. Auth, retries, streaming live here.
    function invoke(self, req: ProviderRequest) -> ProviderResponse

    // 3. INGEST: provider response -> events, raw payload preserved
    function ingest(self, resp: ProviderResponse) -> Event[]
}
```

The runner drives one model turn as: `render` → `invoke` → `ingest` →
append the events → hand them to the policy.

## Same-provider fidelity

`ingest` currently records the assistant message twice on the same event:
canonical fields (`content`, tool calls) and the raw provider payload
(`raw_json`), plus the producing client's ID.

When `render` encounters an `AssistantMessage`:

- Produced by this client → replay `raw_json` verbatim. Reasoning blocks,
  tool call IDs, and API-specific structures survive exactly.
- Produced by another client → rebuild from the canonical fields.

Some providers require verbatim replay for correctness (signed reasoning
blocks, response item IDs). This rule makes that automatic, and makes
cross-provider rendering possible at the same time.

This representation is under reconsideration. A complete HTTP response is not
necessarily valid model input, and `content: string` cannot canonically retain
structured tool calls or reasoning blocks. The candidate redesign separates
canonical content blocks, a narrow replay capsule, and an optional remote
continuation checkpoint. See
`../05_appendix/03_client_replay_and_continuations.md`.

## Switching providers mid-session

```baml
s.set_client(anthropic_client);
```

Nothing happens to the data. The next `render` walks the same journal and
lowers canonically wherever the producing client differs. Switching is a
rendering decision, not a migration.

A client may use a server-held response chain or prompt cache as a transport
optimization, with one obligation: it must always be able to rebuild the full
request from the journal alone. The journal remains the source of truth.

A durable remote conversation is a different storage mode. It can outlive the
local process and can contain state not recoverable from a response cursor.
Its lifecycle, branching, synchronization, retention, and failure semantics
are unresolved. The current client contract does not define this mode.

## Writing a client

Most users never write one. To support a new provider, implement the three
functions. `render` is a fold over journal entries into the provider's
message format; `ingest` parses the response into events. Neither holds
state, so a client is testable with a literal journal in and events out.
Custom events and unknown built-ins must be skipped in `render` (unless
`Promptable`) and never produced by `ingest`.
