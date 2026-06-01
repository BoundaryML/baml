# API reference

Consolidated index of every public function, method, and class, with a
one-line summary. Generated from the in-code docstrings / JSDoc (run
`python docs/_gen_reference.py` to refresh). Full parameter/return detail
lives in the docstrings themselves.

## `libs/bench_core` (Python)

### `libs/bench_core/cursor_client.py`

- **`launch_agent(api_key, prompt_text, repo_url, ref, auto_create_pr, model, timeout)`** — Launch a Cursor cloud agent to work a fix on a GitHub repo.

### `libs/bench_core/jsonl.py`

- **`_scan(s, start)`** — Find the brace-balanced object that opens at a given index.
- **`extract_first_json_object(s)`** — Parse and return the first top-level JSON object found in a string.
- **`extract_last_json_object(s)`** — Parse and return the last top-level JSON object found in a string.

### `libs/bench_core/notion_client.py`

- **`_chunks(text, size)`** — Split text into line-aligned chunks each no larger than a size limit.
- **`_paragraph(text)`** — Build a Notion paragraph block wrapping the given text.
- **`class NotionClient`** — Minimal Notion REST client for creating and updating issue pages.
    - `__init__(token)` — Build the auth, version, and content-type headers for Notion requests.
    - `create_issue_page(database_id, title, status_name, body, evidence_links, suggestion, category)` — Create a Notion issue page with a title, status, and chunked body.
    - `set_status(page_id, status_name)` — Update the Status select property on an existing issue page.

### `libs/bench_core/prices.py`

- **`_load()`** — Lazily load and memoize the model rate table from prices.toml.
- **`prices_for(model)`** — Look up the rate card for a model.
- **`compute_cost(input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, prices)`** — Compute the USD cost of a token breakdown at the given per-million rates.

### `libs/bench_core/processor.py`

- **`class Processor`** — Event-driven claim loop: a subclass declares its queue and implements process().
    - `__init__(service)` — Bind the service client and mint a unique worker id for this process.
    - `process(item)` — Handle one claimed item; subclasses implement the stage's work.
    - `_claim_one()` — Claim a single item using the subclass's queue config.
    - `_drain()` — Claim and run items until the queue is empty, or just one in batch mode.
    - `_run_one(item)` — Run process() for one item under a heartbeat task, failing it on error.
    - `_heartbeat(item_id)` — Periodically extend the item's lease until the task is cancelled.
    - `_poll_backstop()` — Periodically drain the queue to backstop dropped SSE wake-ups.
    - `run()` — Run the main loop: drain on startup, then drain on each SSE wake-up.
- **`run_processor(proc_factory)`** — Run a processor to completion from a synchronous entry point.

### `libs/bench_core/proxy_client.py`

- **`class ProxyClient`** — Load-spreading client for a pool of stateless claude-proxy instances.
    - `__init__(urls, token)` — Initialize the client from a pool of proxy URLs and a bearer token.
    - `from_env()` — Build a client from the CLAUDE_PROXY_URLS and CLAUDE_PROXY_TOKEN env vars.
    - `run_agent(req, timeout)` — Run an agent by POSTing /run-agent to a randomly chosen proxy.
    - `check_baml(req, timeout)` — Check a baml repro by POSTing /check-baml to a randomly chosen proxy.

### `libs/bench_core/schemas.py`

- **`class Prices`** — Per-million-token USD rate card for a single model.
- **`class RunAgentRequest`** — Request to spawn a claude agent against a cell on the version-cached baml CLI.
- **`class CheckBamlRequest`** — Request to run a baml command against a minimal repro on the version-cached CLI.
- **`class CheckBamlResult`** — Outcome of a CheckBamlRequest: exit code, timeout flag, and output tails.
- **`class AgentResult`** — Result of a run-agent invocation: status, token/cost metrics, and posted files.
- **`class Metrics`** — Aggregated trophy metric bag rolled up across a run's invocations.
- **`class EvidenceAnchor`** — Pointer back to the trophy and transcript location that a finding cites.
- **`class Finding`** — A single skill or language issue the worker agent surfaced in a run, with its transcript anchor.

### `libs/bench_core/service_client.py`

- **`class ServiceClient`** — Async HTTP client for the baml-bench service's CRUD, queue, and blob endpoints.
    - `__init__(base_url, token, timeout)` — Open a bearer-authenticated httpx client against the service base URL.
    - `aclose()` — Close the underlying httpx client and release its connections.
    - `create(table, doc)` — Insert a document into a table via the service's POST /{table} endpoint.
    - `get(table, id)` — Fetch a single document by id.
    - `list(table, **query)` — List documents in a table, filtered by the supplied query params.
    - `update(table, id, patch)` — Apply a partial update to a document.
    - `remove(table, id)` — Delete a document by id.
    - `claim(table, worker_id, lease_ms, value, claimed_value, field, index)` — Atomically claim one queued document, flipping its field and stamping a lease.
    - `transition(table, id, to, field, patch, release_claim)` — Transition a claimed document's field to a new value and release its lease.
    - `heartbeat(table, id, lease_ms)` — Extend a claimed document's lease so a long-running job keeps its claim.
    - `events(table, value, field, index)` — Stream the claimable-document count over SSE, yielding on each change.
    - `put_transcript(table, id, text)` — Upload a transcript blob for a document and return its storage id.
    - `get_transcript(storage_id)` — Fetch the text of a previously uploaded transcript blob.
    - `baml_current()` — Fetch the currently pinned baml build.
    - `baml_update()` — Trigger the service to refresh the pinned baml build.
    - `put_baml_binary(build_id, data)` — Upload the compiled baml CLI binary for a build.

### `libs/bench_core/slack_client.py`

- **`post_message(token, channel, text, thread_ts, blocks)`** — Post a message to a Slack channel via chat.postMessage.
- **`verify_signature(signing_secret, timestamp, body, signature, max_skew)`** — Verify a Slack request signature using the v0 HMAC-SHA256 scheme.

## `convex` (TypeScript)

### `convex/bamlBuilds.ts`

- **`get`** — Fetch one baml build by id.
- **`list`** — List baml builds newest-first, optionally filtered by an index field/value.
- **`countClaimable`** — Count baml builds in a claimable state for queue-depth gauges.
- **`create`** — Insert a new baml build row.
- **`update`** — Patch fields on a baml build.
- **`remove`** — Delete a baml build.
- **`claim`** — Atomically claim the oldest queued baml build for a worker.
- **`transition`** — Move a baml build to a new status and release its claim.
- **`heartbeat`** — Extend a claimed baml build's lease so a live worker isn't reaped.

### `convex/issues.ts`

- **`get`** — Fetch one issue by id.
- **`list`** — List issues newest-first, optionally filtered by an index field/value.
- **`countClaimable`** — Count issues in a claimable state for queue-depth gauges.
- **`create`** — Insert a new issue row.
- **`update`** — Patch fields on an issue.
- **`remove`** — Delete an issue.
- **`claim`** — Atomically claim the oldest queued issue for a worker.
- **`transition`** — Move an issue to a new status and release its claim.
- **`heartbeat`** — Extend a claimed issue's lease so a live worker isn't reaped.

### `convex/maintenance.ts`

- **`reap`** — Cron entry point that sweeps every queue rule for expired leases.
- **`reapNow`** — Public wrapper around the reaper for ops/testing ("force a reap now").

### `convex/tasks.ts`

- **`get`** — Fetch one task by id.
- **`list`** — List tasks newest-first, optionally filtered by an index field/value.
- **`countClaimable`** — Count tasks in a claimable state for queue-depth gauges.
- **`create`** — Insert a new task row.
- **`update`** — Patch fields on a task.
- **`remove`** — Delete a task.
- **`claim`** — Atomically claim the oldest queued task for a worker.
- **`transition`** — Move a task to a new status and release its claim.
- **`heartbeat`** — Extend a claimed task's lease so a live worker isn't reaped.

### `convex/trophies.ts`

- **`get`** — Fetch one trophy by id.
- **`list`** — List trophies newest-first, optionally filtered by an index field/value.
- **`countClaimable`** — Count trophies in a claimable state for queue-depth gauges.
- **`create`** — Insert a new trophy row.
- **`update`** — Patch fields on a trophy.
- **`remove`** — Delete a trophy.
- **`claim`** — Atomically claim the oldest queued trophy for a worker.
- **`transition`** — Move a trophy to a new status and release its claim.
- **`heartbeat`** — Extend a claimed trophy's lease so a live worker isn't reaped.

