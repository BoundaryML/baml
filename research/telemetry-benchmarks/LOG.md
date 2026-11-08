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

## 2026-11-07: BCS Billing page confirms the billing replay

After a reload, the Billing page (Oct 8 – Nov 7, 2026) shows **5,457 spans** and **34 MB**. The prediction from replaying the billing rules over the hosted matrix's upload bodies was 2,799 + 2,658 = **5,457 spans** (exact) and 4.5 MB + 29.55 MB ≈ **34.1 MB** (matches at the page's 1 MB display precision). All later experiments used local stand-ins and sent nothing to BCS. So `lab/logical_usage.py` reproduces BCS billing for these workloads, and every BCS cost figure here rests on measured, not estimated, billed bytes.

## 2026-11-07: E1 rerun with independent documents (`results/wire-distinct.json`)

**Flaw in the earlier "10v" batches:** the ten "varied" calls used one document with ` #0`…` #9` appended. Logfire and OTel gzip each export batch, and gzip's 32 KB window let calls 2–10 refer back to call 1. At 1 and 10 KiB their per-call bytes were therefore understated by about 4–8×. BAML and BCS were not affected, because content hashing matches only whole identical values and the suffix already made every value different. Datadog doesn't compress, so it wasn't affected either. At 100 KiB the previous document is outside gzip's window, so those rows were already honest.

Fix: `wire.py` now passes ten independent random documents joined by `|`, and both `py_apps.py` and BAML's `Many` take document *i* for call *i*. Rerun locally for classify/extract/agent × 1/10/100 KiB × 6 stacks, 54 runs, 0 errors. The 100 KiB arguments (about 1 MB) fit under macOS `ARG_MAX` (1 MiB).

Bytes on the wire per call, classify, 10 independent documents:

| stack | 1 KiB old → new | 10 KiB old → new | 100 KiB old → new |
|---|---|---|---|
| Logfire | 328 → 1,282 | 2,211 → 15,934 | 155,335 → 147,692 |
| OTel | 226 → 930 | 1,041 → 7,892 | 77,201 → 77,199 |
| Datadog | 4,165 → 4,161 | 22,600 → 22,594 | 206,923 → 206,917 |
| BAML CLI | 21,185 → 21,196 | 39,644 → 39,639 | 223,571 → 218,426 |

BCS billed bytes didn't change (for example, 10 KiB classify: 219,406 → 219,346). Uncompressed, Logfire carries the prompt about 2.4× per call (function argument, chat message, and the pending span) and OTel 1× (only the decorator's `input` attribute; openai-v2 puts no message text on spans). gzip then shrinks this random text to about 65%. Real English text compresses further.

Corrected comparison at 10 KiB: BAML sends 2.5× Logfire, 5× OTel and 1.75× Datadog per call. That includes about 5.5 KB per call of the one-time definitions upload spread over the 10 calls. Earlier it looked like 18× Logfire. The site and FINDINGS.md now use the rerun rows for every "10v" figure.

## 2026-11-07: E9 BCS time to searchable (read/write key as `BOUNDARY_QUERY_KEY`)

The user supplied a key with read access. `baml query --project aarons-prod-org1/test --environment Production` works. Its first answer is another billing cross-check: `spans` holds 1,309 network + 745 future + 3,403 function = **5,457 spans**, matching the Billing page.

Method (`experiments/searchable.py --vendors bcs`): the BAML CLI runs `Classify` once against the mock model. The upload goes through the recording relay to the real BCS, and the relay learns the process ID. After the process exits, the script polls `select count(*) from spans where process_id = '…'` every 0.5 s; each query also takes about 0.4–0.6 s, so the resolution is about 1 s.

| vendor | trials | median | range |
|---|---:|---:|---|
| BCS (`baml query`) | 6 | 5.8 s | 1.8–6.7 s |
| Logfire (SQL API) | 6 | 15.2 s | 8.4–15.7 s |
| Datadog APM (spans search) | 6 | 37.8 s | 34.9–54.7 s |

A filter on a captured value (`input_args['text'] like 'abc%'`) took 6.3 s over the whole project because it loads values from content-addressed storage (336 loads, 6.2 MB read). Filters on span columns such as `process_id` answer in about 0.3–0.6 s.

## 2026-11-07: E14 Logfire payload anatomy (`experiments/logfire_anatomy.py`, `results/logfire-anatomy.json`, `results/wire-english.json`)

Question: why do Logfire's bytes per call jump around so much (2.2 KB vs 15.9 KB for the same 10 KiB prompt, and 1.6× the prompt at 100 KiB)? Method: the Python `classify` app with Logfire sends to a local sink that keeps every export body. Each body is gunzipped, decoded as OTLP, and split into categories by exact protobuf size (`KeyValue.ByteSize()`, `Span.ByteSize()`). The decoded bodies are then recompressed with gzip level 6 (which reproduces the SDK's bytes exactly) and with lzma (8 MB window) as a counterfactual. Sweep: 10 calls × {random, English} × {independent, suffix, identical} documents × {1, 4, 10, 32, 100} KiB, 30 runs.

English documents are slices of unique docstring lines from the standard library and the venv's packages (`lab/corpus.py`, 5 MB, lines matching Logfire's scrub patterns removed), so gzip shrinks them to 31–50% instead of 75% for random characters. A first English run sliced a 491 KB corpus at random offsets, so ten 100 KiB documents overlapped; it was discarded and rerun with disjoint slices.

**Where the bytes go before compression** (per call, random independent documents):

| document | prompt as function argument | prompt in model span | other attributes | span ids, names, times | envelope | total |
|---:|---:|---:|---:|---:|---:|---:|
| 1 KiB | 1,140 | 1,287 | 1,665 | 284 | 144 | 4,519 |
| 10 KiB | 10,255 | 11,425 | 1,676 | 284 | 144 | 23,783 |
| 100 KiB | 102,414 | 102,548 | 1,562 | 260 | 145 | 206,929 |

The prompt is carried twice: as the `text` argument on `Calling __main__.classify` and inside `gen_ai.input.messages` on the model span. Fixed metadata is about 2 KB per call (mostly `logfire.json_schema`, `logfire.msg_template`, tags, code location). Pending spans add a third copy only for spans still open when an export fires: 2 of 22 spans in most runs, 0 at 100 KiB.

**Why gzip removes one copy in some runs and not others.** The OTLP body groups spans by instrumentation scope: all ten `logfire.openai` model spans first, then all ten `logfire` function spans. So the two copies of the same document sit about ten spans apart. gzip can only refer back 32 KB.

| pattern | 1 KiB | 4 KiB | 10 KiB | 32 KiB | 100 KiB |
|---|---:|---:|---:|---:|---:|
| random, independent: sent per call | 1,281 | 6,365 | 16,715 | 55,796 | 155,351 |
| random, suffix (old 10v): sent per call | 330 | 1,128 | 2,212 | 55,799 | 186,420 |
| random, identical: sent per call | 559 | 1,122 | 2,154 | 55,839 | 155,364 |
| random, identical: lzma per call | 517 | 1,000 | 1,904 | 5,348 | 15,833 |
| median gap between copies, identical | 2.5 KB | 5.6 KB | 11.7 KB | 34.2 KB | 103 KB |

- Independent documents: the copies are 16–930 KB apart, beyond the window, so gzip pays for both. At 100 KiB, 155 KB sent = 2.02 × the 76.9 KB of one gzipped document. lzma, which can see the earlier copy, sends 77.5 KB, one copy.
- Identical or near-identical documents: up to 10 KiB the previous call's copy is within 32 KB, so each call after the first costs about 2 KB. At 32 KiB the gap reaches 34 KB and the saving disappears in one step (2.2 KB → 55.8 KB per call). This is the cliff that made the first E1 run look 4–8× too small at 1 and 10 KiB.

**Content matters as much as size.** English independent documents: 846 / 2,568 / 7,821 / 25,686 / 67,277 B per call at 1/4/10/32/100 KiB, 34–60% less than random characters. The same switch across stacks (`wire.py --content english`, classify, 10 independent documents, per call):

| stack | 10 KiB random → English | 100 KiB random → English |
|---|---|---|
| Logfire | 15.9 → 8.1 KB | 147.7 → 65.0 KB |
| OTel | 7.9 → 3.6 KB | 77.2 → 33.2 KB |
| Datadog | 22.6 → 23.2 KB | 206.9 → 213.4 KB |
| BAML CLI | 39.6 → 39.9 KB | 218.4 → 220.1 KB |

Datadog and BAML send uncompressed, so they don't shrink. BCS billed bytes don't change either (10 KiB: 219,346 → 221,632 B for ten calls; the English documents JSON-escape their newlines). With prose prompts BAML sends about 5× Logfire's bytes at 10 KiB and 3.4× at 100 KiB.

None of this changes Logfire's bill. It bills 2 records per `classify` call whatever the size.

## 2026-11-08: E15 support chatbot, 1 to 50 turns (`experiments/chat.py`, `results/chat-*.json`)

Question: how many bytes (and GB per million conversations) does a multi-turn agent chat use on each platform, and does BCS's content-addressed storage help?

App (`apps/baml_chat/`, `apps/py_apps.py chat`, script in `lab/chat_script.py`): a support bot with a 4 KB system prompt, a 240-character customer message per turn, an `lookup_order` tool call on every other turn (the model asks for it, the tool result goes back into the conversation, the model answers), and a 600-character answer. Every model call sends the whole conversation so far. BAML uses `${for (let m in history)} ${role(m.role)} ${m.content} ${endfor}` in the prompt; Python sends the same messages through the OpenAI SDK, traced the way each vendor documents (`respond` and `converse` decorated, plus the OpenAI instrumentation). Prompt bytes to the model match within 4% across stacks. Text is English from `lab/corpus.py`. Conversations of 1, 2, 3, 5, 8, 10, 15, 20, 30, 40 and 50 turns, one fresh process each, with a fresh user script per run. BAML uploads were forwarded to the real BCS; Logfire, Datadog and OTel went to the local relay (their byte counts don't depend on the backend).

**BCS delivery drops what is still queued 5 s after shutdown begins.** The first sweep used an instant mock model, so a 50-turn conversation finished in about a second with megabytes of uploads queued. Against the real BCS, the CLI printed `warning: telemetry recording failed: cloud recording delivery: ShutdownTimeout`: 29 of 44 planned uploads arrived (15 lost; 30–32 of 441 captured values had no uploaded content). The local stand-in, which answers instantly, received all 44. The limit is `DeliveryConfig::shutdown_timeout = 5 s` (`btel_bcs/src/delivery.rs:145`); the CLI's `--shutdown-timeout` (default 15 s) does not reach it. A process that makes many model calls without real model latency (a batch job against a cache, a replay, a test) can lose its last few megabytes of traces. With a 1 s model delay, uploads keep pace during the conversation and nothing was lost. All numbers below use a 1 s model delay, which also makes the Python SDKs batch the way they would in production.

Results, one conversation, 1 s model delay (BAML on the real BCS: every planned upload arrived, 0 objects were already stored):

| turns | model calls | prompts to model | BCS billed | BCS network | Datadog | Logfire | OTel |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 2 | 9.7 KB | 24.0 KB | 103.7 KB | 31.5 KB | 12.8 KB | 3.6 KB |
| 10 | 15 | 159.6 KB | 348.7 KB | 533.7 KB | 378.6 KB | 106.5 KB | 25.5 KB |
| 20 | 30 | 520.4 KB | 1.09 MB | 1.42 MB | 1.16 MB | 276.2 KB | 60.1 KB |
| 30 | 45 | 1.08 MB | 2.23 MB | 2.70 MB | 2.35 MB | 917.5 KB | 201.5 KB |
| 50 | 75 | 2.81 MB | 5.68 MB | 6.47 MB | 5.94 MB | 2.80 MB | 661.4 KB |

- Bytes grow with the square of the conversation length, because every call resends the history. 5× the turns (10 → 50) is 16× the BCS bill. In a 10-turn conversation 89% of the bytes sent to the model are repeats.
- BCS bills about 2.0× the prompts (5.68 MB for 2.81 MB at 50 turns): each call's history is captured as `Respond`'s `history` input and again inside the HTTP request body.
- **BCS's content-addressed storage saves nothing here.** The shaper cuts a capture into separately stored blobs only when a unit reaches 64 KiB or a string reaches 16 KiB (`btel_settings/src/snapshot.rs` `SPLIT_UNIT_BYTES`, `SPLIT_LEAF_BYTES`). Each message is under 1 KB, so a call's whole `Msg[]` is one new blob, and the prepare responses showed only `inline_with_recording` and `member_of_batch`, never `already_available`. Upload bytes are 1.14× billed (encoding overhead).
- Datadog sends uncompressed JSON with the history on the workflow span and the LLM span: about the same as BCS.
- Logfire sends about 1.0× the prompts. It exports every 0.5 s, so each export holds about one call and gzip has no earlier copy of the history to refer to; pending-span placeholders for still-open spans add copies (327 spans on the wire at 50 turns for 176 stored records). With an instant model, where exports hold many calls, the same conversation was 1.29 MB.
- OTel exports every 5 s (about five calls here) and gzip refers back to the previous call's history while the conversation is under 32 KB: 60 KB at 20 turns, then 201 KB at 30 and 661 KB at 50.
- With an instant model, hosted BCS uploads were 24–32% larger than local ones at 10–30 turns (for example 1.81 vs 1.39 MB at 20 turns) for identical billed bytes. With the 1 s model they matched local within 1%. Not investigated further.

Per million conversations, list prices:

| turns | BCS billed | BCS cost | Logfire records | Logfire cost | Datadog LLM spans | Datadog cost |
|---:|---:|---:|---:|---:|---:|---:|
| 10 | 349 GB | $1,395 | 36M | $72 | 15M | $5,250 |
| 20 | 1,091 GB | $4,364 | 71M | $142 | 30M | $10,500 |
| 50 | 5,682 GB | $22,728 | 176M | $352 | 75M | $26,250 |

Logfire records: one per conversation, two per model call, one per lookup (the OTel span count, 176 at 50 turns, matches). BCS overtakes Datadog's per-call price only past about 50 turns; Logfire stays 20–65× cheaper. `http_bodies = false` would remove the second copy and roughly halve the BCS figures (E11 measured 38–50% on single calls); not measured for chat.

Billed bytes sent to BCS by this experiment (test project): 14.3 MB for the 1 s sweep, plus the instant-model sweep (11 runs, two of them partly lost) and three probe runs of 40–50 turns.

## 2026-11-08: E16 when BCS's cache helps: a photo or a big system prompt in the chat (`chat.py --variant`, `results/chat-{photo,bigsystem}-*.json`)

Question (from the user): isn't BCS supposed to cache, and what about images? Two variants of the E15 chatbot, 1/5/10/20 turns, 1 s model delay, BAML on the real BCS:

- `photo`: a 150 KB incompressible stand-in for a JPEG (200 KB as base64) attached before the first message (`RespondWithPhoto`, `image.from_base64`; Python sends an `image_url` data URI). It stays in the conversation, so every call resends it.
- `bigsystem`: the system prompt plus a 16.5 KB policy manual (20 KB total), over BCS's 16 KiB string threshold.

BAML merges consecutive user messages, so the photo and the first question arrive as one message; the mock rule now searches the whole last message for the order question.

One 20-turn conversation (30 model calls):

| variant | prompts | BCS billed | BCS uploaded | uploaded / billed | Datadog | Logfire | OTel |
|---|---:|---:|---:|---:|---:|---:|---:|
| plain chat | 520 KB | 1.09 MB | 1.39 MB | 127% | 1.16 MB | 276 KB | 60 KB |
| 20 KB system prompt | 1.03 MB | 2.09 MB | 1.80 MB | 86% | 2.20 MB | 1.05 MB | 244 KB |
| photo | 6.53 MB | 11.60 MB | 7.61 MB | 66% | 13.37 MB | 19.28 MB | 4.92 MB |

- **The cache works for large pieces, inside one process.** A captured value is cut into its own content-addressed blob when a string reaches 16 KiB (`SPLIT_LEAF_BYTES`) or a unit reaches 64 KiB (`SPLIT_UNIT_BYTES`). Each call's `Respond` input is a new root blob (the history changed), but the photo and the big system prompt are child blobs that the client uploads once and then only references. A local run with nothing cached on the server uploaded the same bytes (7.61 MB vs 7.61 MB at 20 turns), so the dedup is done by the client within the process, not by the server.
- **It saves upload bandwidth, not money.** Billed bytes are each root's logical size including its children, so the photo is billed in all 30 inputs. The raw HTTP request body also embeds the photo as base64 text and is a different string every call, so it is uploaded and billed every call. Net: about 2 billed copies and 1 uploaded copy of the photo per call.
- **No reuse across processes.** Two separate processes sent the identical photo (`results/chat-photo-crossprocess.json`): both uploaded 706 KB, and BCS reported 0 objects already stored. All blobs went as `inline_with_recording` or `member_of_batch`; none were offered for server-side reuse.
- **Other platforms with the photo:** Datadog sends it uncompressed on the workflow and LLM spans (2.0× prompts). Logfire sends it about 3 times (function argument, model span, pending placeholders) and base64 of compressed data barely gzips: 19.3 MB for one conversation, far past its 5 KB-per-span guideline. These went to the local relay, so we don't know whether Logfire or Datadog would store, truncate or reject spans this large. OTel sends one gzipped copy (0.75×).
- Per million 20-turn conversations with a photo, list prices: BCS 11,600 GB = $46,394; Logfire 71M records = $142; Datadog 30M LLM spans = $10,500.
