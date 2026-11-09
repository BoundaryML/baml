# Findings: BAML + Boundary Cloud vs Logfire, Datadog and OpenTelemetry

Measured on 2026-11-06/07 with BAML 0.25.2, Logfire 5.4.0, ddtrace 4.16.3 (LLM Observability, agentless), OpenTelemetry SDK 1.45.2 and `opentelemetry-instrumentation-openai-v2` 2.4b0. LOG.md has the raw notes, `results/` has the raw data, and the website is `site/index.html` (published at https://claude.ai/artifact/EJopk9WZFEpG1o2pQmWSfz).

All runs use a local fake OpenAI server, so the numbers measure tracing, not the model. Hosted runs forwarded to the real Logfire, Datadog and BCS projects, and every request was accepted.

**Billing check:** the BCS Billing page went from 2,799 spans / 4.5 MB to 5,457 spans / 34 MB. That exactly matches the 5,457 spans and about 34.1 MB predicted by replaying BCS's billing rules over the recorded uploads, so the cost figures below rest on billed bytes the page confirms. A second check on 2026-11-09 compared every hosted BAML run since with what BCS stored: for all 96 runs, the spans `baml query` finds under the run's process IDs equal the replay's count (6,161 of 6,161; `experiments/bcs_check.py`).

## Strengths of BAML + BCS

1. **No tracing code.** The four apps needed 0 setup lines in BAML (plus 1 explicit `$trace` to capture a plain function's values). The Python versions needed 10 lines for Logfire and Datadog, and 28 for plain OTel (`results/loc.json`).
2. **Fast and small.** A full BAML CLI process (start, 51 calls, upload, exit) takes 150 ms and 61 MB. Untraced Python takes 744 ms and 88 MB, and Python + Datadog takes 1,930 ms and 135 MB. The per-call cost of tracing is +0.04 ms for BAML, against +0.06 ms for OTel, +0.19 ms for Datadog and +0.23 ms for Logfire (`results/overhead.json`). Per-call differences do not matter next to real model latency. Process startup matters for CLIs, jobs and serverless.
3. **Little lost when a process dies.** The BAML CLI lost 0% of traces on SIGTERM and 2.8% on kill -9. Python stacks lose on SIGTERM too, because Python's default SIGTERM skips `atexit`: OTel lost 40–42% and Datadog 7–37% (`results/crash.json`).
4. **No dropped spans under bursts.** In 8,200 function calls in under a second, BAML (`high`) and ddtrace delivered all of them. The OTel SDK and Logfire delivered 2,560 (69% dropped) because of the 2,048-span queue, with one "Queue full, dropping Span." line on stderr (`results/pipeline.json`).
5. **Spans are free on BCS.** The 8,200 spans billed 445 bytes. The same workload is about $82 per million requests on Logfire (41 records each) and about $70 on Datadog APM indexed spans.
6. **Retries are visible.** BAML recorded all 30 HTTP attempts in the agent runs, including the 10 503 failures. Logfire and Datadog recorded 20 model spans, because the OpenAI SDK's internal retry happens inside one instrumented call.
7. **Full fidelity, no silent edits.** BCS stores the raw HTTP request and response plus inputs and outputs and never redacts unless asked (`@trace.redact`). Logfire's default scrubbing replaced 8 of 10 ordinary support tickets with `[Scrubbed due to '…']`, triggered by words like password, session, cookie, secret santa and authentication (`results/scrub.json`).
8. **Fastest to search.** A finished BAML CLI run appeared in `baml query` after a median of 5.8 s (1.8–6.7 s, 6 trials), against 15.2 s for Logfire's SQL API and 37.8 s for Datadog span search (`results/searchable.json`).
9. **Traces leave the process as it runs.** In a 96-call agent conversation, each call's trace reached the real BCS a median 0.57 s after the model replied (max 0.74 s), only 4% of 113 MB was sent after the last call, and nothing was lost. Datadog's LLM Observability writer sent all 202 MB at exit (LOG.md, "E18").
10. **Dedup on the wire.** Ten identical 100 KiB calls upload 328 KB, against 2.19 MB for ten different ones.

## Weaknesses and surprises

1. **Large prompts are expensive on BCS.** BCS bills captured bytes, and the prompt counts twice: once as the function input and once inside the HTTP request body. Measured per million calls:

   | app, document | BCS | BCS, `http_bodies=false` | Logfire | Datadog LLM Obs |
   |---|---:|---:|---:|---:|
   | classify, 1 KiB | $14 | $8 | $4 | $350 |
   | classify, 10 KiB | $88 | $45 | $4 | $350 |
   | classify, 100 KiB | $825 | $414 | $4 | $350 |
   | agent, 100 KiB | $835 | $421 | $8 | $700 |

   Logfire's per-record price does not depend on size. Its costs page says it meters "every span, log or metric", averages payload size over time, and may send "a polite email, never a surprise fee" above about 5 KB per span. Our 10 KiB and 100 KiB spans exceed that guideline, so the low Logfire cost for large prompts holds by list price but may not hold for a heavy LLM customer in practice. Values over 10 MB per record are truncated, and retention is 30 days (90 on Growth). Rule of thumb: BCS is cheaper than Logfire only when billed bytes per call are under about 500 B × the number of records Logfire would store. That holds for programs with many steps and little data, not for document-sized prompts. BCS is cheaper than Datadog LLM Obs below about 85 KB per model call (about 170 KB with `http_bodies=false`).
2. **CAS dedup is not passed on in billing.** Ten identical 100 KiB calls bill the same 2.06 MB as ten different ones.
3. **`http_bodies = false` halves the bill** for large inputs (38% at 1 KiB, 50% at 100 KiB). But it is only available in `[bridge]`/`[pack]` baml.toml, not as a runtime setting for the CLI (`results/wire-nobodies.json`).
4. **The Python bridge ignores `BAML_TELEMETRY`.** `off`, `low` and `medium` produced identical uploads. Bridge and pack artifacts read the level variable only when the publisher sets `[bridge.env_var_names] BAML_TELEMETRY = "BAML_TELEMETRY"` (`btel_settings/src/artifact.rs`). A Python developer who sets `BAML_TELEMETRY=off` still uploads.
5. **About 55 KB fixed upload per process** (runtime definitions), so short-lived processes send 25–50× more bytes than Logfire or OTel for a single call. It is billed at only a few hundred bytes.
6. **Default `medium` shows nothing per call for plain functions** (timing aggregates only). To see non-LLM steps you need `high` or a call-site `trace.span()`.
7. **No disk spool.** What is unsent at kill -9 is gone, about the last 0.5 s of work.
8. **Long chats are expensive, and BCS's cache doesn't help.** A support bot that resends its conversation every turn billed 349 KB at 10 turns and 5.68 MB at 50 turns: $1,395 and $22,728 per million conversations, against $72 and $352 on Logfire and $5,250 and $26,250 on Datadog LLM Obs. Bytes grow with the square of the turn count, and BCS bills about twice the prompts (history as input and inside the HTTP body). Content-addressed storage only splits out pieces of 64 KiB (16 KiB for a string), so each call's history of sub-1 KB messages is one new blob: 0 cache hits in 50 turns (LOG.md, "E15").
9. **The cache helps only with large pieces, only within a process, and never on the bill.** Strings of 16 KiB or more (an image's data, a long document) become shared blobs the client uploads once per process. With a 150 KB photo in a 20-turn chat, BCS uploaded 7.61 MB but billed 11.60 MB ($46,394 per million conversations); with a 20 KB system prompt, 1.80 MB uploaded for 2.09 MB billed. The raw HTTP body embeds the photo as base64 and is new every call, so it is uploaded and billed every time. A second process with the identical photo uploaded it again: 0 objects already stored (LOG.md, "E16").
10. **Bigger conversations make it much worse.** A coding agent that reads one file per turn reached 2.18 MB of context (about 545k tokens) after 48 turns. BCS billed 197.9 MB for that one conversation, $791,777 per million conversations, against $482 on Logfire and $33,600 on Datadog LLM Obs. The cache hit on 91 of 96 calls, because files of 16 KiB or more became shared pieces, but that only cut uploads to 113 MB; the raw HTTP request (2.2 MB per call by the end) is never shared. A 100-turn chat of 1.5–2.5 KB messages (about 106k tokens) had 1 cache hit in 100 calls and billed 43.0 MB, $172,061 per million (LOG.md, "E17").
11. **Uploads still queued 5 s after shutdown starts are dropped.** BCS delivery's `shutdown_timeout` is a fixed 5 s, not affected by the CLI's `--shutdown-timeout`. A 50-turn chat against an instant model lost 15 of 44 uploads with `ShutdownTimeout`. With a realistic 1 s model, nothing was lost.

## The biggest opportunity

LLM traces are mostly repeats, and BCS bills bytes, so BCS bills the repeats. Because BAML owns the runtime, it can record each new piece of information once: store pieces once and bill them once; record history as "previous list + new message" (the VM knows it is the same list); store the raw request as a receipt (template, input references, settings, hash) and re-render it on read; and reference content stored once per project (files read with `baml.fs.read`, images, system prompts). Applied to the measured conversations, this cuts BCS's bill 70–1,734×, to 20–80× below Datadog and about level with Logfire's list price on three of four conversations (the 100-turn chat of new text stays 4.4× above Logfire). Compression is not counted: queries read decompressed values, so it saves bandwidth and storage, not billed bytes. SDK-based tools can't do this, because they see each call as an opaque string. The site's "Learnings" section has the projection and the full list.

## Notes on the other stacks

- **Logfire** records function arguments but not return values by default. Its pending spans (633 on the wire for 525 stored records) cost bytes but are not stored.
- **OTel's OpenAI instrumentation** records no message text on spans (content goes to log events), which is why its byte counts are so low. It also loses everything inside its 5 s batch window on a kill.
- **Compression:** Logfire and OTel gzip their exports; Datadog and BAML send uncompressed bytes. With ten independent 10 KiB documents of random characters, per-call bytes are BAML 39.6 KB, Datadog 22.6 KB, Logfire 15.9 KB and OTel 7.9 KB. An earlier version of this comparison used near-identical documents, which gzip compressed across calls and made Logfire and OTel look 4–8× smaller (LOG.md, "E1 rerun").
- **Logfire payload anatomy** (LOG.md, "E14"): each traced call carries the prompt twice, as the function argument and inside the model span's `gen_ai.input.messages`, plus about 2 KB of metadata. The export groups spans by instrumentation scope, so the two copies of one prompt are ten spans apart and gzip's 32 KB window can't match them; both copies are paid for (100 KiB: 155 KB sent = 2.02 × one gzipped prompt). A repeated prompt is nearly free up to 10 KiB and costs full price from 32 KiB, a step at the window size. Content matters as much as size: with English prose instead of random characters, Logfire sends 8.1 KB per 10 KiB call (BAML 39.9 KB, about 5×) and OTel 3.6 KB, while Datadog and BAML stay the same. None of this changes Logfire's per-record bill.
- **Datadog LLM Obs** sends uncompressed JSON (about 207 KB per 100 KiB call, with no dedup), adds about 1.2 s and 47 MB at startup, and has the highest cost per model call.

## Open items

- Live OpenAI runs (blocked: infisical login).
