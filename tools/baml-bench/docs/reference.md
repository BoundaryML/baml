# API reference

Consolidated index of every function, method, and class (private,
underscore-prefixed helpers included), with a one-line summary. Generated
from the in-code docstrings / JSDoc (run
`python docs/_gen_reference.py` to refresh). Full parameter/return detail
lives in the docstrings themselves.

## `libs/bench_core` (Python)

### `libs/bench_core/cursor_client.py`

- **`launch_agent(api_key, prompt_text, repo_url, ref, auto_create_pr, model, timeout)`** - Launch a Cursor cloud agent to work a fix on a GitHub repo.

### `libs/bench_core/jsonl.py`

- **`_scan(s, start)`** - Find the brace-balanced object that opens at a given index.
- **`extract_first_json_object(s)`** - Parse and return the first top-level JSON object found in a string.
- **`extract_last_json_object(s)`** - Parse and return the last top-level JSON object found in a string.

### `libs/bench_core/notion_client.py`

- **`_chunks(text, size)`** - Split text into chunks each no larger than a size limit.
- **`_paragraph(text)`** - Build a Notion paragraph block wrapping the given text.
- **`class NotionClient`** - Minimal Notion REST client for creating and updating issue pages.
    - `__init__(token)` - Build the auth, version, and content-type headers for Notion requests.
    - `create_issue_page(database_id, title, status_name, body, evidence_links, suggestion, category)` - Create a Notion issue page with a title, status, and chunked body.
    - `set_status(page_id, status_name)` - Update the Status select property on an existing issue page.

### `libs/bench_core/prices.py`

- **`_load()`** - Lazily load and memoize the model rate table from prices.toml.
- **`prices_for(model)`** - Look up the rate card for a model.
- **`compute_cost(input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, prices)`** - Compute the USD cost of a token breakdown at the given per-million rates.

### `libs/bench_core/processor.py`

- **`class Processor`** - Event-driven claim loop: a subclass declares its queue and implements process().
    - `__init__(service)` - Bind the service client and mint a unique worker id for this process.
    - `process(item)` - Handle one claimed item; subclasses implement the stage's work.
    - `_claim_one()` - Claim a single item using the subclass's queue config.
    - `_drain()` - Claim and run items until the queue is empty, or just one in batch mode.
    - `_run_one(item)` - Run process() for one item under a heartbeat task, failing it on error.
    - `_heartbeat(item_id)` - Periodically extend the item's lease until the task is cancelled.
    - `_poll_backstop()` - Periodically drain the queue to backstop dropped SSE wake-ups.
    - `run()` - Run the main loop: drain on startup, then drain on each SSE wake-up.
- **`run_processor(proc_factory)`** - Run a processor to completion from a synchronous entry point.

### `libs/bench_core/proxy_client.py`

- **`class ProxyClient`** - Load-spreading client for a pool of stateless claude-proxy instances.
    - `__init__(urls, token)` - Initialize the client from a pool of proxy URLs and a bearer token.
    - `from_env()` - Build a client from the CLAUDE_PROXY_URLS and CLAUDE_PROXY_TOKEN env vars.
    - `run_agent(req, timeout)` - Run an agent by POSTing /run-agent to a randomly chosen proxy.
    - `check_baml(req, timeout)` - Check a baml repro by POSTing /check-baml to a randomly chosen proxy.

### `libs/bench_core/schemas.py`

- **`class Prices`** - Per-million-token USD rate card for a single model.
- **`class RunAgentRequest`** - Request to spawn a claude agent against a cell on the version-cached baml CLI.
- **`class CheckBamlRequest`** - Request to run a baml command against a minimal repro on the version-cached CLI.
- **`class CheckBamlResult`** - Outcome of a CheckBamlRequest: exit code, timeout flag, and output tails.
- **`class AgentResult`** - Result of a run-agent invocation: status, token/cost metrics, and posted files.
- **`class Metrics`** - Aggregated trophy metric bag rolled up across a run's invocations.
- **`class EvidenceAnchor`** - Pointer back to the trophy and transcript location that a finding cites.
- **`class Finding`** - A single skill or language issue the worker agent surfaced in a run, with its transcript anchor.

### `libs/bench_core/service_client.py`

- **`class ServiceClient`** - Async HTTP client for the baml-bench service's CRUD, queue, and blob endpoints.
    - `__init__(base_url, token, timeout)` - Open a bearer-authenticated httpx client against the service base URL.
    - `aclose()` - Close the underlying httpx client and release its connections.
    - `create(table, doc)` - Insert a document into a table via the service's POST /{table} endpoint.
    - `get(table, id)` - Fetch a single document by id.
    - `list(table, **query)` - List documents in a table, filtered by the supplied query params.
    - `update(table, id, patch)` - Apply a partial update to a document.
    - `remove(table, id)` - Delete a document by id.
    - `claim(table, worker_id, lease_ms, value, claimed_value, field, index)` - Atomically claim one queued document, flipping its field and stamping a lease.
    - `transition(table, id, to, field, patch, release_claim)` - Transition a claimed document's field to a new value and release its lease.
    - `heartbeat(table, id, lease_ms)` - Extend a claimed document's lease so a long-running job keeps its claim.
    - `events(table, value, field, index)` - Stream the claimable-document count over SSE, yielding on each change.
    - `put_transcript(table, id, text)` - Upload a transcript blob for a document and return its storage id.
    - `get_transcript(storage_id)` - Fetch the text of a previously uploaded transcript blob.
    - `baml_current()` - Fetch the currently pinned baml build.
    - `baml_update()` - Trigger the service to refresh the pinned baml build.
    - `put_baml_binary(build_id, data)` - Upload the compiled baml CLI binary for a build.

### `libs/bench_core/slack_client.py`

- **`post_message(token, channel, text, thread_ts, blocks)`** - Post a message to a Slack channel via chat.postMessage.
- **`verify_signature(signing_secret, timestamp, body, signature, max_skew)`** - Verify a Slack request signature using the v0 HMAC-SHA256 scheme.

## `services` (Python)

### `services/api/__main__.py`


### `services/api/app.py`

- **`require_bearer(authorization)`** - Enforce bearer-token auth on a request.
- **`create_app()`** - Build and return the configured FastAPI application.

### `services/api/blobs.py`

- **`_path(storage_id)`** - Resolve a storage id to an absolute path inside the blob directory.
- **`put_text(kind, key, text)`** - Write a text blob to the volume and return its storage id.
- **`get_text(storage_id)`** - Read and return a stored text blob.
- **`put_binary(kind, key, data)`** - Write a binary blob to the volume and return its identifying metadata.
- **`get_binary(storage_id)`** - Read and return a stored binary blob.
- **`exists(storage_id)`** - Report whether a blob exists for the given storage id.

### `services/api/convex_gateway.py`

- **`class ConvexGateway`** - Async client for Convex's HTTP function API (query/mutation/action).
    - `__init__(url, admin_key, poll_interval)` - Initialize the gateway and its underlying HTTP client.
    - `_run(kind, name, args)` - Invoke a Convex function over the HTTP API and return its value.
    - `query(name, args)` - Run a Convex query function.
    - `mutation(name, args)` - Run a Convex mutation function.
    - `action(name, args)` - Run a Convex action function.
    - `subscribe_counts(name, args)` - Poll a count query and yield whenever the count changes.
- **`gateway_from_env()`** - Construct a ConvexGateway from environment variables.

### `services/api/routers/baml_builds.py`

- **`_gh_headers(accept)`** - Build GitHub API request headers, adding auth when available.
- **`_resolve_alpha(slug)`** - Resolve the latest baml-language alpha pre-release from GitHub.
- **`make_baml_router(convex)`** - Build the baml version registry and build-coordination router.

### `services/api/routers/table.py`

- **`class ClaimBody`** - Request body for claiming the next item off a table's queue.
- **`class TransitionBody`** - Request body for transitioning an item to a new status.
- **`class HeartbeatBody`** - Request body for extending a claimed item's lease.
- **`make_router(table, convex)`** - Build a CRUD + queue + SSE router for a single Convex table.

### `services/baml_builder/__main__.py`

- **`class BamlBuilder`** - Processor that fetches a queued BAML alpha build and marks it ready.
    - `process(item)` - Download the alpha release binary for a claimed build and store it.

### `services/baml_builder/build.py`

- **`_gh_headers()`** - Build the GitHub API request headers, adding auth when a token is set.
- **`_platform_triple()`** - Return the glibc Linux target triple for this machine's architecture.
- **`_extract_baml(targz)`** - Extract the baml binary from a gzipped release tarball.
- **`fetch_baml(tag)`** - Download the alpha release `tag` asset for this platform; return the baml binary bytes.

### `services/baml_dedup/__main__.py`

- **`class BamlDedup`** - Batch queued trophies through the classify/dedup agent and upsert the issues DB.
    - `__init__(service)` - Initialize the processor and build a proxy client from the environment.
    - `process(first)` - Dedup a window of queued trophies and upsert the resulting issues.
    - `_open_issues()` - Fetch the currently-open issues to give the agent dedup context.
    - `_parse_issues(result)` - Extract the issue list the agent produced from its run result.
    - `_upsert(it)` - Create a new issue or merge evidence into an existing one.
    - `_merge_evidence(old, new)` - Append new evidence entries, skipping ones already present.

### `services/baml_dedup/prompts.py`


### `services/baml_dedup/render.py`

- **`render_reports_md(trophies)`** - Render a batch of trophies into the reports.md document fed to the agent.
- **`render_open_issues_json(issues)`** - Render open issues into the open_issues.json document fed to the agent.

### `services/baml_worker/__main__.py`

- **`_load_skill()`** - Load and cache the BAML skill text injected into the agent's workspace.
- **`_derive_outcome(agent_status, task_completed)`** - Map the agent's run status and self-report into a trophy outcome.
- **`_mine_baml_errors(turn_log)`** - Mine baml-attributable tool errors from the turn log as tentative findings.
- **`class BamlWorker`** - Claim a task, run the BAML agent, verify its repros, and create a trophy.
    - `__init__(service)` - Bind the service client and build a proxy client from the environment.
    - `process(item)` - Run one task end to end and persist its trophy.
    - `_verify_repros(findings, baml_version, task_id)` - Verify each finding's repro through the proxy's baml, in place.
    - `_parse_trophy_json(result)` - Parse the agent's self-reported trophy.json from the run result.
    - `_render_report_md(analysis, metrics, outcome)` - Render a fallback markdown report when the agent supplied none.
    - `_notify(item, outcome, summary, metrics, findings, trophy_id)` - Post the run result to Slack, linking the trophy on the dashboard.

### `services/baml_worker/prompts.py`


### `services/claude_proxy/__main__.py`


### `services/claude_proxy/app.py`

- **`_safe_staging(prefix, raw, field)`** - Resolve a staging directory for a caller-supplied id, rejecting traversal.
- **`_get_api_key()`** - Return the Anthropic API key.
- **`_require_bearer(authorization)`** - Reject the request unless its bearer token matches the proxy token.
- **`_ensure_baml(sha)`** - Return the dir containing the `baml` binary for `sha`, caching on miss.
- **`healthz()`** - Liveness probe that always reports the service is up.
- **`run_agent(req, authorization)`** - Run a Claude Code agent over staged files and return transcript + metrics.
- **`check_baml(req, authorization)`** - Compile/run a minimal repro with the version-cached baml on PATH (no claude). Used by the worker to verify a finding's repro reproduces.

### `services/claude_proxy/runner.py`

- **`validate_relative_path(rel)`** - Reject paths that are empty, absolute, or contain parent traversal.
- **`materialize_files(staging, files)`** - Write each file's content into the staging directory, creating parents.
- **`spawn_claude(claude_bin, cwd, prompt, model, max_turns, system_prompt, baml_bin_dir, timeout_secs, anthropic_api_key)`** - Run `claude -p -` and return (stdout, stderr, exit_code).
- **`run_command(cwd, command, baml_bin_dir, timeout_secs)`** - Run a shell command (e.g. `baml build`) in cwd with the version-cached baml on PATH. Returns (stdout, stderr, exit_code, timed_out). exit_code -9 signals a wall-clock timeout.
- **`parse_claude_session(stdout)`** - Extract the final JSON summary line of `claude -p --output-format json`.
- **`session_log_path(staging, session_id)`** - Compute the path to claude's session jsonl log for a staging dir.
- **`_preview(s, is_error)`** - Truncate a string to a preview, keeping head and tail for errors.
- **`_result_text(c)`** - Extract the text payload from a tool_result content block.
- **`parse_turn_log(jsonl)`** - Parse claude's session jsonl into per-assistant-turn structured rows.
- **`compute_cost(session, prices)`** - Compute the USD cost of a session from its token counts and prices.
- **`host_metadata()`** - Capture OS, architecture, timestamp, and optional hostname of the host.
- **`collect_post_files(staging, patterns, max_file_bytes, max_total_bytes)`** - Collect text files under staging matching glob patterns within size caps.

### `services/cron/__main__.py`

- **`_tasks()`** - Return the prompt(s) to enqueue this cycle.
- **`_cycle(service)`** - Run one cron cycle: refresh the baml build, then enqueue the day's task(s).
- **`_amain()`** - Run the cron loop: one cycle on start, then every ``INTERVAL`` seconds.

### `services/ingress/__main__.py`


### `services/ingress/app.py`

- **`_is_duplicate(event_id)`** - Record an event id and report whether it was already seen.
- **`healthz()`** - Liveness probe.
- **`_create_slack_task(event, text, eid)`** - Create a Slack-sourced task off the request path.
- **`slack_events(request, background_tasks, x_slack_signature, x_slack_request_timestamp, x_slack_retry_num)`** - Handle the Slack Events API callback (URL verification + app mentions).
- **`notion_webhook(request, x_notion_signature)`** - Approve the issue a Notion webhook points at (the fix dispatcher claims it).
- **`_toggle_uuid_hyphens(value)`** - Return the alternate hyphenation of a Notion id.
- **`bug_trigger(payload)`** - Create a task from a bug report.

### `services/notion_fixer/__main__.py`

- **`class NotionPush`** - Claim loop that mirrors dirty issues onto the Notion board.
    - `__init__(service)` - Build the processor and its Notion client.
    - `_db_for(kind)` - Return the Notion database id for an issue kind.
    - `process(issue)` - Sync one claimed issue to Notion and mark it synced.
    - `_confirm(issue_id, issue)` - Promote a just-boarded ``open`` issue to ``confirmed``.
    - `_map_status(status)` - Map an internal issue status to its Notion board status label.
- **`class FixDispatch`** - Claim loop that dispatches approved issues to Cursor for a fix.
    - `__init__(service)` - Build the processor and its Notion client.
    - `process(issue)` - Dispatch a fix for one claimed approved issue.
- **`_amain()`** - Run the NotionPush and FixDispatch claim loops together until cancelled.

### `services/notion_fixer/fixer.py`

- **`choose_repo(kind)`** - Return the GitHub ``org/repo`` that owns issues of this kind.
- **`repo_url(kind)`** - Return the full GitHub URL for the repo that owns this issue kind.
- **`_pr_instructions()`** - Return the boilerplate telling the agent how to write the pull request.
- **`cursor_prompt(issue)`** - Build the instruction text for a Cursor cloud agent.
- **`evidence_links(issue)`** - Build dashboard URLs for each trophy cited as evidence on an issue.

## `convex` (TypeScript)

### `convex/bamlBuilds.ts`

- **`get`** - Fetch one baml build by id.
- **`list`** - List baml builds newest-first, optionally filtered by an index field/value.
- **`countClaimable`** - Count baml builds in a claimable state for queue-depth gauges.
- **`create`** - Insert a new baml build row.
- **`update`** - Patch fields on a baml build.
- **`remove`** - Delete a baml build.
- **`claim`** - Atomically claim the oldest queued baml build for a worker.
- **`transition`** - Move a baml build to a new status and release its claim.
- **`heartbeat`** - Extend a claimed baml build's lease so a live worker isn't reaped.

### `convex/issues.ts`

- **`get`** - Fetch one issue by id.
- **`list`** - List issues newest-first, optionally filtered by an index field/value.
- **`countClaimable`** - Count issues in a claimable state for queue-depth gauges.
- **`create`** - Insert a new issue row.
- **`update`** - Patch fields on an issue.
- **`remove`** - Delete an issue.
- **`claim`** - Atomically claim the oldest queued issue for a worker.
- **`transition`** - Move an issue to a new status and release its claim.
- **`heartbeat`** - Extend a claimed issue's lease so a live worker isn't reaped.

### `convex/lib.ts`

- **`getDoc`** - Fetch a single row by id.
- **`listDocs`** - List rows newest-first, optionally filtered to a single index field/value.
- **`countClaimable`** - Count rows currently in a claimable state for queue-depth gauges.
- **`createDoc`** - Insert a row with default attempts and timestamps.
- **`updateDoc`** - Patch a row and bump its updatedAt timestamp.
- **`removeDoc`** - Delete a row by id.
- **`claimDoc`** - Atomically claim the oldest claimable row for a table.
- **`transitionDoc`** - Move a row to a new status and, unless told otherwise, release its claim.
- **`heartbeatDoc`** - Extend a claimed row's lease so a live worker isn't reaped.

### `convex/maintenance.ts`

- **`reap`** - Cron entry point that sweeps every queue rule for expired leases.
- **`reapNow`** - Public wrapper around the reaper for ops/testing ("force a reap now").

### `convex/tasks.ts`

- **`get`** - Fetch one task by id.
- **`list`** - List tasks newest-first, optionally filtered by an index field/value.
- **`countClaimable`** - Count tasks in a claimable state for queue-depth gauges.
- **`create`** - Insert a new task row.
- **`update`** - Patch fields on a task.
- **`remove`** - Delete a task.
- **`claim`** - Atomically claim the oldest queued task for a worker.
- **`transition`** - Move a task to a new status and release its claim.
- **`heartbeat`** - Extend a claimed task's lease so a live worker isn't reaped.

### `convex/trophies.ts`

- **`get`** - Fetch one trophy by id.
- **`list`** - List trophies newest-first, optionally filtered by an index field/value.
- **`countClaimable`** - Count trophies in a claimable state for queue-depth gauges.
- **`create`** - Insert a new trophy row.
- **`update`** - Patch fields on a trophy.
- **`remove`** - Delete a trophy.
- **`claim`** - Atomically claim the oldest queued trophy for a worker.
- **`transition`** - Move a trophy to a new status and release its claim.
- **`heartbeat`** - Extend a claimed trophy's lease so a live worker isn't reaped.

## `ui` (TypeScript)

### `ui/app/api/state/route.ts`

- **`GET`** - GET /api/state - returns the current live snapshot as JSON for the polling dashboard.

### `ui/app/db/[table]/db-view.tsx`

- **`DbView`** - Client component rendering a live table view of tasks, trophies, or issues with

### `ui/app/db/[table]/page.tsx`

- **`Page`** - Server component for the "/db/[table]" route. Validates the table slug against

### `ui/app/graph-view.tsx`

- **`GraphView`** - Client component rendering the interactive Cytoscape pipeline graph. Nodes link

### `ui/app/layout.tsx`

- **`RootLayout`** - Root server-component layout wrapping every page in the html/body shell and

### `ui/app/lib/data.ts`

- **`Turn`** - One Claude Code transcript turn (a single API call) in a trophy's turn log.
- **`Metrics`** - Aggregate run metrics (turns, tokens, cost, wall clock) recorded on a trophy.
- **`Finding`** - A single skill- or language-level finding surfaced by a run, optionally anchored to a transcript call.
- **`Trophy`** - A completed run record: its outcome, metrics, report, findings, and full turn log.
- **`Task`** - A benchmark task queued for an agent to attempt (from Slack, cron, etc.).
- **`Issue`** - A deduplicated issue aggregated from run findings, tracked through to Notion/Cursor.
- **`issueStatusLabel`** - Maps an issue's raw status to the label shown in the UI.
- **`RunRow`** - A flattened run row joining a trophy to its task, sized for the dashboard runs table.
- **`DashboardData`** - The full payload rendered by the static dashboard: runs, open issues, and headline totals.
- **`loadDashboardData`** - Loads and reshapes trophies, tasks, and issues into the static dashboard payload.
- **`Build`** - A BAML canary build row from the bamlBuilds registry (sha, channel ref, status).
- **`Inflight`** - A unit of work an agent is actively processing right now (task, trophy, issue, or build).
- **`TaskRow`** - A task row enriched for the live view, including its claiming worker and resolved report id.
- **`LiveState`** - The full live-polled snapshot driving the graph, db tables, and live dashboard.
- **`loadState`** - Loads the live snapshot: status tallies, in-flight work, runs, issues, builds, and agent counts.
- **`RunDetail`** - A run-detail bundle for the run page: the trophy, its task, and a readable baml version.
- **`loadRun`** - Loads a single run's detail: its trophy, the originating task, and a readable baml label.
- **`loadTask`** - Loads a task plus the id of any trophy it has produced (used to redirect to the result).

### `ui/app/lib/format.ts`

- **`ago`** - Formats an elapsed duration in milliseconds as a compact relative age.

### `ui/app/live-dashboard.tsx`

- **`usePolledState`** - Hook that polls /api/state every 3s and exposes the latest live snapshot,
- **`LiveDashboard`** - Client component rendering the live dashboard: the tasks -> trophies -> issues

### `ui/app/page.tsx`

- **`Page`** - Server component for the dashboard home route ("/"). Loads the initial live

### `ui/app/runs/[id]/call-scroller.tsx`

- **`CallScroller`** - Client component that reads ?call=N from the URL and, on mount, opens the matching

### `ui/app/runs/[id]/expand-all.tsx`

- **`ExpandAll`** - Client component button that opens or closes every <details> in the transcript

### `ui/app/runs/[id]/page.tsx`

- **`RunPage`** - Server component for the "/runs/[id]" route rendering a trophy's full detail:

### `ui/app/tasks/[id]/page.tsx`

- **`TaskPage`** - Server component for the "/tasks/[id]" route. Loads the task; 404s if not found,

