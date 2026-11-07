# Telemetry benchmarks: research log

This log records every experiment, in order, with its date, setup, result and the file that holds the raw data. Times are Pacific unless marked UTC. All model replies and payloads are synthetic unless an entry says it used a live provider.

## Setup

- 2026-11-06. BAML checkout: this worktree, `canary` at `d6ab63cd02`. BCS: a detached worktree of `origin/main` at `0c1735ab` in `~/projects/bcs-main`. The user's `~/projects/bcs` checkout (branch `studio-open-on-dashboards`) is left untouched.
- BAML CLI 0.25.2, SHA-256 `672167c2…636f` (the same binary the earlier trace-lab run used), in an isolated `env/baml-home`. `bin/baml` wraps it.
- Python 3.13 venv in `env/py`: ddtrace 4.16.3, logfire 5.4.0, opentelemetry-sdk 1.45.2, baml-bridge 0.25.2.
- Credentials live in `.env`, which git ignores.

### What each credential can do (checked 2026-11-06)

| Credential | Works for | Refused |
| --- | --- | --- |
| Datadog API key | Intake (`/api/v1/validate` → valid) | — |
| Datadog PAT (app key) | `spans/events/search`, `spans/analytics/aggregate` | metrics query, `usage/hourly_usage`, `current_user` (403) |
| Datadog admin API key + PAT | same as above | same |
| Logfire `pylf_v2` token | OTLP write, `/v1/query` SQL, `/api/v1/projects` | — |
| BCS `bdry_secret` key | Uploads (heartbeat, prepare, PUT) | `baml query` against the cloud (403 `ACCESS_REQUIRED`) |

So vendor-side usage counters for Datadog and BCS come from their web dashboards, read before and after each controlled run.

## Earlier work this builds on

An earlier session on 2026-11-06 left two bodies of results, which this work reuses and cites rather than repeats:

- `~/projects/trace-lab` (`RESEARCH_LOG.md`): BAML wire bytes per workload, a BCS Billing-page delta for one 10 KiB capture (5 spans, +21.0 kB), Python exporter bytes, Logfire scrubbing cost, Python crash-loss.
- `~/projects/bcs/benchmarks/matched-work` (untracked): one matched stock-agent in BAML and Python; Datadog truncates any span tag longer than 25,000 bytes to 2,500 bytes; OTel Python drops span events past 128.

## Experiment plan

The list changes as results come in. Status: planned, running, done, dropped.

| # | Question | Status |
| --- | --- | --- |
| E1 | Payload-shape matrix: what does BCS bill for hello world, a bool, a class, a 1 KB and a 10 KB string, a 100 KB string? Read the Billing page before and after each. | planned |
| E2 | Same matrix through Python + Logfire and Python + ddtrace: request bytes on the wire and what each vendor stores (readback). | planned |
| E3 | Lines of code: how much instrumentation each stack needs to capture the same fields. | planned |
| E4 | Live-model overhead: with a real LLM call, what share of the request is tracing? | planned |
| E5 | Startup and memory: import time and RSS of ddtrace, logfire, OTel, the BAML bridge and the BAML CLI. | planned |
| E6 | Crash durability for BAML: kill -9 mid-run; what reaches BCS? (Python's answer is known: 0/40 queued spans.) | planned |
| E7 | Concurrency: 100 parallel calls through BAML `spawn` vs Python asyncio + OTel; per-call overhead and trace shape. | planned |
| E8 | Large values: where each stack truncates or drops (Datadog 25 KB cliff, OTel attribute limits, Logfire, BCS). | planned |
| E9 | Time to searchable: send one span, poll each vendor's query API until it shows up. | planned |
| E10 | Cost model: measured bytes and span counts × published prices, per million LLM calls. | planned |

## 2026-11-06: Published pricing (desk research, pages fetched today)

| Vendor | Unit | Price | Free | Retention |
| --- | --- | --- | --- | --- |
| Datadog APM | host/mo | $31 (annual) | 150 GB ingested + 1M indexed spans per host | — |
| Datadog ingested spans | GB | $0.10 | 150 GB/host | 15 d (unverified) |
| Datadog indexed spans | 1M spans | $1.70 (15 d, annual) | 1M/host | 7/15/30 d |
| Datadog LLM Obs | 10K LLM spans | $160/mo base for 100K, then $3.50 per 10K (15 d) | "40K free" (unclear) | 15 d |
| Logfire | 1M records | $2 | 10M/mo | 30 d |
| Langfuse | 100K units | $8 → $6 tiered | 50K (Hobby) | 30 d+ |
| Braintrust | GB | $4 (Starter), $3 (Pro, over 5 GB) | 1 GB | 14 d |
| Honeycomb | events | $150 / 50M | 20M | — |
| BCS (internal `config/bcs.toml`, not public) | logical captured GB; spans never billed | $4/GB Starter, $3/GB Pro ($250/mo, 5 GB) | 1 GB | undecided |

Limits: Datadog tag value 25,000 chars; ddtrace-py cuts anything longer to 2,500 (`_limits.py`). Logfire 10 MB per span, names 512 B.

## 2026-11-06: How BCS bills and what BAML records (read from source)

Code read from bcs `0c1735ab` and baml `d6ab63cd02`. These are hypotheses for the experiments to test, not measurements.

- BCS bills only captured logical bytes (`logical_bytes_approx_v1`). This is a semantic size: an 8 B type id per value, the string's UTF-8 length, 8 B per int, and so on. Compression, CAS layout and definitions do not count. Spans are never billed (`bcs.toml:217`, `projector/billing.rs:48-123`, `btel_snapshot/src/logical_bytes.rs`).
- Storage dedups identical blobs. Billing does not: a value referenced three times is billed three times (`billing.rs` test at :139).
- `BAML_TELEMETRY`: off/low/medium/high, with medium the default. At medium, plain functions add only timing aggregates. LLM functions capture inputs, output and error. Network spans keep request and response bodies (headers allowlisted, others hashed). Explicit capture: `f(x, $trace = trace.span(inputs = true))`. `@trace.redact` puts a marker in place of a field.
- Likely double billing for an LLM call: the prompt counts once as function input and again inside the HTTP request body. To test in E1.
- Upload: files seal at 1 MiB or 1 s, and envelopes are raw protobuf. I found no compression code (absence of evidence). On exit it drains for at most 5 s. No disk spool in cloud mode, so kill -9 should lose the unsent tail. To test in E6.
- Limits: HTTP bodies over 8 MiB are recorded as truncated. Captured values are cut only above 1 GiB. Recording file 8 MiB. Server blob max 64 MiB (quarantined, bills 0). No retention policy yet. No quota enforcement.

## 2026-11-07 03:24 UTC: BCS Billing baseline (before any hosted run in this work)

Firefox, cloud.boundaryml.com → aarons-prod-org1 → Billing, "Last 30 days · UTC, Oct 8 – Nov 7, 2026": estimated cost $0.00, **2,799 spans, 4.5 MB data volume**. The page shows data volume to 0.1 MB, so a validation batch has to add several MB to be readable. The earlier session's single 10 KiB probe was readable only because the total was then below 1 MB.

## 2026-11-07: First local wire numbers (smoke runs, one call, 1 KiB input)

| Stack | App | Telemetry bytes sent | Spans |
| --- | --- | ---: | ---: |
| BAML CLI → BCS (local stand-in) | classify | 59,482 | 7 |
| Python → BAML bridge → BCS | classify | 58,901 | 6 |
| Python + Logfire | classify | 1,362 | 2 |
| Python + Datadog LLM Obs | classify | 4,191 | 2 |
| Python + OTel + openai-v2 | classify | 1,402 | 2 |

BCS billed 3,892 logical bytes for that call. Most of the 59 KB on the wire is a one-time per-process upload (runtime definitions; see `trace-lab` wire-layout: about 46 KB). The full matrix with 10-call batches checks how it amortizes.

Behavior noticed along the way:
- `@logfire.instrument()` records arguments but not return values unless `record_return=True`.
- The OpenAI SDK retries a 503 internally (`max_retries`), so Logfire, Datadog and OTel each show one model span per logical call, and the failed attempt is invisible. BAML's `ai.clients.Retry` makes two HTTP requests that are both recorded as network spans (to confirm in readback).
- `opentelemetry-instrumentation-openai-v2` 2.4b0 fails to import with the newest `opentelemetry-util-genai` (1.3b0), and 2.3b0 fails with wrapt 2.x. Working pair: 2.4b0 + util-genai 1.1b0. Even with `OTEL_INSTRUMENTATION_GENAI_CAPTURE_MESSAGE_CONTENT=true`, prompts and replies were not span attributes: this version emits them as log events, which need a separate log exporter.

## 2026-11-07 ~03:50 UTC: Hosted wire matrix done (`results/wire-hosted.json`)

120 runs, 0 errors. BCS answered every heartbeat 204 and every prepare/PUT 200. Logfire answered 200 and Datadog LLM Obs 202, with no rejections. The matrix is 4 stacks (BAML CLI, Python→BAML bridge, Logfire, Datadog LLM Obs) × 4 apps × {1, 10, 100 KiB} × {1 call, 10 varied, 10 identical}. Every run used a fresh seed, so no BCS object was already stored (`already_available` = 0).

**Prediction for the BCS Billing page**, computed from the exact upload bodies with the projector's billing rules (`lab/logical_usage.py`): the 60 BCS runs add **+2,658 spans and +29,552,599 bytes (29.6 MB)**. Expected page: about **5,457 spans and about 34.1 MB** (baseline 2,799 spans, 4.5 MB). No sizes were missing.

Logfire readback (SQL): the 100 KiB inputs are stored in full (102,403-character `text`/`ticket` attributes, 102,537-character `gen_ai.input.messages`), with no `logfire.truncated` marker. Pending spans are not stored as records.

## 2026-11-07: Logfire readback found a scrubbed input (`lab/logfire_sql.py`)

Logfire had all 525 completed spans from the hosted matrix (the relay also counted 108 "pending span" records, which Logfire does not store). For 10 of the `classify` calls, the stored `text` argument was the 23-character string `[Scrubbed due to 'Jwt']` instead of the 100 KiB document, with `logfire.scrubbed = [{"path":["attributes","text"],"matched_substring":"Jwt"}]`. These were the ten identical calls of one run, so one input. The random document happened to contain the letters "Jwt".

Cause: Logfire 5.4.0 scrubs any attribute value that matches its default patterns (`password`, `secret`, `auth` (not "author"), `session`, `cookie`, `credential`, `api key`, `credit card`, `jwt` and others; `logfire/_internal/scrubbing.py:38`). It replaces the *whole* value, not just the match. The `gen_ai.input.messages` and `gen_ai.output.messages` keys are on a safe list, so the model prompt in the same trace kept the full text. Function arguments recorded by `@logfire.instrument()` are not on that list.

Follow-up (E12): send realistic support tickets through each stack and see which ones survive.

## 2026-11-07: E4/E5 overhead (`experiments/overhead.py --reps 12`, `results/overhead.json`)

Each process made 51 `classify` calls (1 KiB input) against the loopback mock (0 ms model delay), exporting to a loopback stand-in. There were 12 fresh processes per stack in a shuffled order, and every process completed. The table shows medians. "warm" is the median of the 50 calls after the first, pooled across processes.

| stack | process wall | peak RSS | first call | warm call | p95 warm | telemetry bytes |
|---|---:|---:|---:|---:|---:|---:|
| Python, no tracing (`off`) | 744 ms | 88 MB | 25.6 ms | 0.491 ms | 0.734 ms | 0 |
| Python + OTel | 822 ms | 91 MB | 11.0 ms | 0.551 ms | 0.721 ms | 5,152 |
| Python + Logfire | 857 ms | 93 MB | 23.2 ms | 0.725 ms | 1.259 ms | 8,272 |
| Python + Datadog LLM Obs | 1,930 ms | 135 MB | 13.4 ms | 0.676 ms | 0.906 ms | 212,110 |
| Python → BAML bridge, BAML_TELEMETRY=off | 838 ms | 128 MB | 2.7 ms | 0.376 ms | 0.529 ms | 428,290 (see below) |
| Python → BAML bridge | 829 ms | 128 MB | 2.6 ms | 0.372 ms | 0.579 ms | 427,738 |
| BAML CLI, BAML_TELEMETRY=off | 70 ms | 56 MB | 1.4 ms | 0.273 ms | 0.364 ms | 0 |
| BAML CLI | 150 ms | 61 MB | 1.3 ms | 0.312 ms | 0.425 ms | 393,462 |

Takeaways:
- Per-call tracing cost over the untraced baseline of the same runtime: BAML +0.04 ms, OTel +0.06 ms, Datadog +0.19 ms, Logfire +0.23 ms (p95 +0.53 ms). All of these are well under 1% of a real model call (hundreds of ms).
- A whole BAML CLI process (start, 51 calls, upload, exit) takes 150 ms. Python takes 744 ms just to import the OpenAI SDK and run untraced. Datadog adds about 1.2 s of startup and 47 MB.
- Turning BAML tracing on adds 80 ms of wall time per process: the upload of the runtime definitions plus the drain at exit.
- BAML's first call is fast (1.3 ms vs 11–26 ms) because the Python OpenAI client builds its HTTP stack lazily on the first request.
- Bytes: BAML sends far more because it records full HTTP request and response bodies for each call (the 1 KiB prompt is about 7.7 KB per call once the request, response and events are included). OTel's openai-v2 instrumentation records no message content on spans by default, which is why it is so small.

### The Python bridge ignores `BAML_TELEMETRY`

`bridge-off` uploaded as much as `bridge`. Running the bridge with `BAML_TELEMETRY=off`, `low` and `medium` gave identical uploads (14 spans, 4,648 billed bytes for 3 calls). Cause: in bridge and pack artifacts the level variable is a publisher setting (`ArtifactTelemetry::recording_level_envvar`, `btel_settings/src/artifact.rs:106`). It is `None` unless baml.toml sets `[bridge.env_var_names] BAML_TELEMETRY = "BAML_TELEMETRY"`, and `None` means "use the build-time level". So a Python developer who sets `BAML_TELEMETRY=off` still uploads. The only runtime way to stop it is to unset `BOUNDARY_API_KEY`. This is by design ("ambient runtime values are never baked in"), but it is surprising, and the BAML CLI behaves differently.

## 2026-11-07: E9 time to searchable (`experiments/searchable.py --trials 6`, `results/searchable.json`)

The clock starts when the flushed process exits and stops when the vendor's query API first returns the marker.

| vendor | trials | median | range |
|---|---:|---:|---|
| Logfire (SQL API, 0.5 s poll) | 6 | 15.2 s | 8.4–15.7 s |
| Datadog APM spans (spans search, 2 s poll) | 6 | 37.8 s | 34.9–54.7 s |
| BCS | — | not measured | `baml query` needs a key with read access (`BOUNDARY_QUERY_KEY`) |

Logfire's results cluster at 15.2 s, which suggests a fixed ingest or flush tick of about 15 s.

## 2026-11-07: E12 Logfire default scrubbing on ordinary tickets (`experiments/scrub.py`, `results/scrub.json`)

Ten short support tickets went through a `@logfire.instrument()` `classify(ticket_id, text)` function that calls the model with the instrumented OpenAI SDK. The spans were sent to the real Logfire project and read back with SQL.

**8 of 10 ordinary tickets were stored as `[Scrubbed due to '…']`** in the function-argument span. Matches were `password`, `session`, `credit card`, `cookie`, `API key`, `auth` (from "authentication"), `secret` (from "secret santa") and `private key` (from "private key ring"). The control ticket ("Where is my package?") was kept, and so was "author name", which the `auth(?!ors?\b)` rule excludes on purpose. In all 10 traces, the model-call span kept the full prompt text, because `gen_ai.*` message keys are on the safe list.

What this means:
- The redaction is a whole-value replacement. Searching or filtering on the input of a scrubbed span finds nothing, and the span looks empty in the UI.
- The same ticket text stays visible in the child LLM span, so the default does not keep the data out of Logfire. It only hides it from the function span. That makes it both too broad (it erases harmless text) and incomplete (as a privacy measure).
- You can turn it off (`scrubbing=False`) or add callbacks. But the default is on, and a support-ticket or auth-related LLM app hits it all the time.
- For comparison: BAML/BCS never redacts unless you mark a field `@trace.redact`. Datadog LLM Obs does not scrub inputs by default; its tag truncation at 25,000 characters applies to APM span tags, not to LLM Obs I/O.

## 2026-11-07: E11 `http_bodies = false` (`wire.py --stacks bridge bridge-nobodies`, `results/wire-nobodies.json`)

Both stacks run the same Python bridge app with local BCS stand-ins and 54 shuffled runs; there were 0 errors. `bridge-nobodies` is generated from a baml.toml with `[bridge] http_bodies = false`.

| app, input | default billed | `http_bodies=false` billed | saving |
|---|---:|---:|---:|
| classify, 1 KiB, 1 call | 3,892 B | 2,430 B | 38% |
| classify, 10 KiB, 1 call | 22,326 B | 11,646 B | 48% |
| classify, 100 KiB, 1 call | 206,648 B | 103,806 B | 50% |
| agent, 100 KiB, 10 calls | 2,088,102 B | 1,052,348 B | 50% |

This confirms the "prompt billed twice" reading: once as the function input and once inside the HTTP request body. Without bodies, a 100 KiB classify bills 103,806 B, which is the input once plus about 1.4 KB of fixed overhead. The saving reaches 50% for large inputs. For small inputs it is lower because of fixed per-call costs (timings, ids, the output).

The wire shrinks less than the bill for repeated identical calls, because CAS dedup already skips repeated bodies (100 KiB × 10 identical: 328 KB → 221 KB). For varied calls the wire roughly halves too (2.19 MB → 1.15 MB).

What you give up is the raw provider request and response (headers, the exact rendered JSON body, token usage fields) in the trace. The function input and output stay.

## 2026-11-07: E13 non-LLM pipeline, many small spans (`experiments/pipeline.py`, `results/pipeline.json`)

The app (`apps/baml_pipeline`, plus the same code in Python) serves 200 requests. Each request runs `Request`, which calls `Validate` and `Price` 20 times, so 41 function calls per request and 8,200 in total, in a burst of under a second. Python stacks decorate every function. Every stack exports to loopback stand-ins.

| stack | spans delivered (of 8,200) | bytes on the wire | BCS billed bytes |
|---|---:|---:|---:|
| BAML, `BAML_TELEMETRY=medium` (default) | 0 (timing aggregates only) | 2,947 | 445 (process context) |
| BAML, `BAML_TELEMETRY=high` | 8,201 (+ `Pipeline`) | 436,631 (53 B/span) | 445 |
| Python + Logfire | 2,560 completed (+16 pending) | 74,851 | — |
| Python + OTel SDK (gzip) | 2,560 | 48,525 | — |
| Python + ddtrace (OTLP) | 8,200 | 2,753,393 (336 B/span) | — |

**OTel and Logfire silently dropped 69% of spans.** The OTel SDK's `BatchSpanProcessor` has a 2,048-span queue (`OTEL_BSP_MAX_QUEUE_SIZE`). When the program makes spans faster than the exporter sends them, it drops new spans and prints a single line, "Queue full, dropping Span.", to stderr. Re-running OTel gave exactly 2,560 again. With 40 requests (1,640 spans) OTel kept all of them, but Logfire already kept only 1,280. ddtrace kept all 8,200, and so did BAML at `high`. Raising the queue size fixes it, but the default fails silently.

**BCS bills spans at zero.** 8,200 spans cost 445 billed bytes, the same as 0 spans, because no values were captured. At the published prices, per million requests of this app (41 spans each):
- Logfire: 41M records, $82 (if nothing is dropped)
- Datadog APM: 41M indexed spans, $70, plus about 13.8 GB ingested ($1.40) and per-host APM fees
- BCS: about $0 beyond the per-process 445 B

This is the case where BCS pricing wins by far: many spans that carry little data.

BAML's default `medium` level records nothing per call for plain functions. You need `high`, or an explicit `trace.span()` at the call site, to see these steps in a trace.

## 2026-11-07: E6 crash (`experiments/crash.py --trials 3`, `results/crash.json`)

Each trial runs a loop of `classify` calls against a mock model that takes 50 ms per reply, and kills the process at a random 3–9 s. "Lost" means completed model calls whose model span never reached the local backend stand-in. Totals are over 3 trials.

| stack | kill -9: lost / completed | SIGTERM: lost / completed |
|---|---:|---:|
| BAML CLI → BCS | 10 / 358 (2.8%) | 0 / 257 (0%) |
| Python → BAML bridge | 19 / 325 (5.8%) | 17 / 303 (5.6%) |
| Python + Logfire | 10 / 317 (3.2%) | 13 / 258 (5.0%) |
| Python + Datadog LLM Obs | 49 / 133 (36.8%) | 22 / 301 (7.3%) |
| Python + OTel SDK | 128 / 303 (42.2%) | 115 / 290 (39.7%) |

- BAML's CLI drains its upload on SIGTERM (0 lost). On kill -9 it loses about the last half-second of calls. There is no disk spool, so nothing is recovered later.
- In Python, SIGTERM is as bad as kill -9 unless the app installs a handler. Python's default SIGTERM action ends the process without running `atexit`, so no SDK gets to flush. That is why the bridge, Logfire, Datadog and OTel all lose data on SIGTERM too. This is what happens on every `docker stop` or Kubernetes rollout of a Python worker that doesn't handle SIGTERM.
- OTel's `BatchSpanProcessor` exports every 5 s, so a kill loses up to the whole window: one trial lost all 53 calls (killed at 3.7 s, before the first export).
- Datadog's LLM Obs writer flushes every few seconds and lost 37% on kill -9, with large spread.
