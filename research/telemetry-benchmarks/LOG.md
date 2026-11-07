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
