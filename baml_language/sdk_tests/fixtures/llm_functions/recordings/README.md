# LLM streaming replay recordings

Checked-in test data for the streaming replay harness. These files let the keyless replay tests (including Python, TypeScript, Rust, and Java siblings) drive the full bridge → BAML LLM client → HTTP → SSE → stream-consumption pipeline against a recorded provider response — **no `OPENAI_API_KEY` required at test time**.

Everything here is **insta-managed**, written by the
`sdk_test_llm_recordings` crate
(`sdk_tests/harness/llm_recordings/`). Nothing in this directory is
hand-edited. The committed `.snap.sse` payloads are **real `curl` captures**
against gpt-5.4-nano (refresh them with the re-record flow below).

## File roles

For each recording `<name>` (`replay_extract_string`, `replay_extract_doc`):

| File | Role |
|------|------|
| `<name>.snap` | insta binary-snapshot metadata for the SSE payload. |
| `<name>.snap.sse` | The raw `curl`-captured SSE response body, verbatim. **This is the load-bearing payload** the replay server streams back. |

`*.snap.new` / `*.pending-snap` are transient `cargo insta review` artifacts and
are git-ignored.

## Recording requires a key; replaying never does

Whether the recorder hits the network is decided **only by insta state**:
a capture runs when a `.snap.sse` payload is missing, or when `INSTA_UPDATE`
forces an update. In every other state the recorder validates the checked-in
payload offline. So a normal run — even under `infisical run --` with a key in
the environment — makes **no** network calls and dirties **no** snapshots.

## Re-recording

Two flows, both plain insta workflows. Both need a real key, so run them under
`infisical run --`.

```bash
# Reviewable flow: delete, re-capture, review the diff.
rm sdk_tests/fixtures/llm_functions/recordings/replay_extract_doc.snap*
infisical run -- cargo insta test -p sdk_test_llm_recordings
cargo insta review

# One-shot flow: force in-place update via insta's own env var.
INSTA_UPDATE=always infisical run -- cargo nextest run -p sdk_test_llm_recordings
```

The keyless SDK tests assert that the stream yields at least 10 partials and well-typed values; the current Python and TypeScript suites do not pin exact recorded content. A re-record needs no host-test change as long as the response still streams and satisfies the declared result shape, but rerun each participating SDK suite.

## Redaction guarantee

Only the provider's SSE response body is snapshotted; request headers are passed directly to `curl` and are never written to these files. Insta also filters OpenAI-style key-shaped text from snapshot payloads as a defensive measure. Review new `.snap.sse` data before accepting it; there is no separate repository-wide CI grep that replaces that review.
