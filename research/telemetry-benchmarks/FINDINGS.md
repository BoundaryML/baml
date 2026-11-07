# Findings: BAML + Boundary Cloud vs Logfire, Datadog and OpenTelemetry

Measured on 2026-11-06/07 with BAML 0.25.2, Logfire 5.4.0, ddtrace 4.16.3 (LLM Observability, agentless), OpenTelemetry SDK 1.45.2 and `opentelemetry-instrumentation-openai-v2` 2.4b0. LOG.md has the raw notes, `results/` has the raw data, and the website is `site/index.html` (published at https://claude.ai/artifact/EJopk9WZFEpG1o2pQmWSfz).

All runs use a local fake OpenAI server, so the numbers measure tracing, not the model. Hosted runs forwarded to the real Logfire, Datadog and BCS projects, and every request was accepted.

## Strengths of BAML + BCS

1. **No tracing code.** The four apps needed 0 setup lines in BAML (plus 1 explicit `$trace` to capture a plain function's values). The Python versions needed 10 lines for Logfire and Datadog, and 28 for plain OTel (`results/loc.json`).
2. **Fast and small.** A full BAML CLI process (start, 51 calls, upload, exit) takes 150 ms and 61 MB. Untraced Python takes 744 ms and 88 MB, and Python + Datadog takes 1,930 ms and 135 MB. The per-call cost of tracing is +0.04 ms for BAML, against +0.06 ms for OTel, +0.19 ms for Datadog and +0.23 ms for Logfire (`results/overhead.json`). Per-call differences do not matter next to real model latency. Process startup matters for CLIs, jobs and serverless.
3. **Little lost when a process dies.** The BAML CLI lost 0% of traces on SIGTERM and 2.8% on kill -9. Python stacks lose on SIGTERM too, because Python's default SIGTERM skips `atexit`: OTel lost 40–42% and Datadog 7–37% (`results/crash.json`).
4. **No dropped spans under bursts.** In 8,200 function calls in under a second, BAML (`high`) and ddtrace delivered all of them. The OTel SDK and Logfire delivered 2,560 (69% dropped) because of the 2,048-span queue, with one "Queue full, dropping Span." line on stderr (`results/pipeline.json`).
5. **Spans are free on BCS.** The 8,200 spans billed 445 bytes. The same workload is about $82 per million requests on Logfire (41 records each) and about $70 on Datadog APM indexed spans.
6. **Retries are visible.** BAML recorded all 30 HTTP attempts in the agent runs, including the 10 503 failures. Logfire and Datadog recorded 20 model spans, because the OpenAI SDK's internal retry happens inside one instrumented call.
7. **Full fidelity, no silent edits.** BCS stores the raw HTTP request and response plus inputs and outputs and never redacts unless asked (`@trace.redact`). Logfire's default scrubbing replaced 8 of 10 ordinary support tickets with `[Scrubbed due to '…']`, triggered by words like password, session, cookie, secret santa and authentication (`results/scrub.json`).
8. **Dedup on the wire.** Ten identical 100 KiB calls upload 328 KB, against 2.19 MB for ten different ones.

## Weaknesses and surprises

1. **Large prompts are expensive on BCS.** BCS bills captured bytes, and the prompt counts twice: once as the function input and once inside the HTTP request body. Measured per million calls:

   | app, document | BCS | BCS, `http_bodies=false` | Logfire | Datadog LLM Obs |
   |---|---:|---:|---:|---:|
   | classify, 1 KiB | $14 | $8 | $4 | $350 |
   | classify, 10 KiB | $88 | $45 | $4 | $350 |
   | classify, 100 KiB | $825 | $414 | $4 | $350 |
   | agent, 100 KiB | $835 | $421 | $8 | $700 |

   Logfire's per-record price does not depend on size (up to its 10 MB span limit). Rule of thumb: BCS is cheaper than Logfire only when billed bytes per call are under about 500 B × the number of records Logfire would store. That holds for programs with many steps and little data, not for document-sized prompts. BCS is cheaper than Datadog LLM Obs below about 85 KB per model call (about 170 KB with `http_bodies=false`).
2. **CAS dedup is not passed on in billing.** Ten identical 100 KiB calls bill the same 2.06 MB as ten different ones.
3. **`http_bodies = false` halves the bill** for large inputs (38% at 1 KiB, 50% at 100 KiB). But it is only available in `[bridge]`/`[pack]` baml.toml, not as a runtime setting for the CLI (`results/wire-nobodies.json`).
4. **The Python bridge ignores `BAML_TELEMETRY`.** `off`, `low` and `medium` produced identical uploads. Bridge and pack artifacts read the level variable only when the publisher sets `[bridge.env_var_names] BAML_TELEMETRY = "BAML_TELEMETRY"` (`btel_settings/src/artifact.rs`). A Python developer who sets `BAML_TELEMETRY=off` still uploads.
5. **About 55 KB fixed upload per process** (runtime definitions), so short-lived processes send 25–50× more bytes than Logfire or OTel for a single call. It is billed at only a few hundred bytes.
6. **Default `medium` shows nothing per call for plain functions** (timing aggregates only). To see non-LLM steps you need `high` or a call-site `trace.span()`.
7. **No disk spool.** What is unsent at kill -9 is gone, about the last 0.5 s of work.
8. **Read access was not testable.** The ingest key gets 403 on `baml query`, so time-to-searchable for BCS was not measured. Logfire took a median of 15.2 s and Datadog APM 37.8 s.

## Notes on the other stacks

- **Logfire** records function arguments but not return values by default. Its pending spans (633 on the wire for 525 stored records) cost bytes but are not stored.
- **OTel's OpenAI instrumentation** records no message text on spans (content goes to log events), which is why its byte counts are so low. It also loses everything inside its 5 s batch window on a kill.
- **Datadog LLM Obs** sends uncompressed JSON (about 207 KB per 100 KiB call, with no dedup), adds about 1.2 s and 47 MB at startup, and has the highest cost per model call.

## Open items

- BCS Billing page check: the prediction for the hosted matrix is +2,658 spans and +29,552,599 bytes (expected page about 5,457 spans and about 34.1 MB). The page showed 2,799 spans / 4.5 MB before. It has not been reloaded since; Firefox access is read-only.
- Live OpenAI runs (blocked: infisical login).
- BCS time to searchable (needs a key with read access).
