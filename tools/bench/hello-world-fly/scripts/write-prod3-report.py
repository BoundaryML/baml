#!/usr/bin/env python3
"""Write a dated observation report without altering apps or load."""
from pathlib import Path
import datetime,json,shutil
r=Path(__file__).resolve().parents[1]
d=r/'artifacts/reports/baml-in-prod3'
d.mkdir(parents=True,exist_ok=True);e=d/'evidence';e.mkdir(exist_ok=True)
s=json.loads((r/'load-status.json').read_text());m=json.loads((r/'pre-runtime-metrics-manifest.json').read_text());machines=json.loads((r/'report-machines.json').read_text())
for src,dst in [('load-status.json','load-status.json'),('pre-runtime-metrics-manifest.json','matrix-manifest.json'),('report-machines.json','machine-state.json'),('operational-failures.json','first-native-failures.json'),('pre-load.json','pre-load-smoke.json'),('load-preflight.json','two-second-preflight.json'),('load-initial-failed.json','failed-collector-attempt.json'),('load-machine.json','load-machine.json')]:
 shutil.copyfile(r/src,e/dst)
mem=(r/'artifacts/baml-al2023-process-state.txt').read_text().split('Name:',1)[0]
(e/'al2023-meminfo.txt').write_text(mem)
rows=[]
for v in m['variants']:
 x=s['targets'][v['variant']];ok=x['status_codes'].get('200',0);total=x['requests'];bad=total-ok
 codes=', '.join(f'{k}: {n:,}' for k,n in x['status_codes'].items() if k!='200') or 'none'
 rows.append(f"| {v['variant']} | {total:,} | {ok:,} | {codes} | {bad/total:.2%} | {x['observed_completion_qps_last_interval']:.2f} |")
text=f'''# BAML on Fly: hello-world stability findings

**All seven requested containers were implemented and deployed successfully, but the native BAML HTTP-server rows failed under the initial 100-QPS load.** Alpine and Debian have confirmed OOM kills and automatic restarts. Amazon Linux 2023 stopped responding reliably, failed its health check, and showed exhausted available memory; its captured machine history does not yet confirm an OOM kill. Both host-language baselines and the Python/BAML bridge have no recorded request failures in the six-minute snapshot. Node/BAML recorded 31 early transport errors and then continued at approximately 100 QPS.

This is an **interim report**, not a completed one-hour result. The run remains active and the apps remain deployed. The user described the result as somewhat expected. The experiment has exposed a reproducible investigation target; it has not established the precise runtime defect or demonstrated long-term stability for the rows that have not failed yet.

## Observation window

- Load run: `{s['id']}`.
- Start: September 10, 2026, **13:55:06 PDT / 20:55:06 UTC**.
- Load snapshot: **{s['elapsed_seconds']:.3f} seconds** after start, approximately **14:01:07 PDT / 21:01:07 UTC**.
- Machine-state capture began at **{machines['captured_at_utc']}**; individual app queries followed sequentially.
- Scheduled attack end: approximately **14:55:06 PDT / 21:55:06 UTC**, followed by up to 10 seconds to drain outstanding requests and a five-second summary interval.
- No measured app has been manually restarted, resized, patched, or redeployed during the full run. Reported native restarts are Fly's normal response to process failure.

## What was deployed

The source is `{r}`. Every measured app is a new app in Fly organization `boundary`, region `iad`, with exactly one Linux amd64 machine, 1 shared CPU, 1024 MiB RAM, no swap, and autostop disabled. All seven endpoints passed exact status/body/content-type smoke checks before any sustained load. Existing apps and unrelated concurrent repository work were preserved.

| Variant | Base and actual runtime | HTTP implementation |
| --- | --- | --- |
| node-baseline | `node:20-alpine`: Node 20.20.2, Alpine 3.23.4, musl 1.2.5 | Express 5.1.0, no BAML |
| node-baml | Same exact Node base | Express calls actual BAML per request through the Node musl bridge |
| python-baseline | `python:3.10-slim`: Python 3.10.21, Debian 13.6/trixie, glibc 2.41 | Starlette 0.47.3 / Uvicorn 0.35.0, no BAML |
| python-baml | Same exact Python base | Starlette calls actual BAML per request through the Python bridge |
| baml-amazonlinux | `amazonlinux:2023`: AL2023.12.20260817, glibc 2.34 | Packed BAML executable and native HTTP server |
| baml-debian | `debian:bookworm-slim`: Debian 12, glibc 2.36 | Packed BAML executable and native HTTP server |
| baml-alpine | `alpine:3.22`: Alpine 3.22.5, musl 1.2.5 | Packed BAML executable and native HTTP server |

All base images are pinned to their exact amd64 digests. The [matrix manifest](evidence/matrix-manifest.json) contains every URL, source directory, base digest, deployed image tag/digest, machine ID, resource setting, bridge version, and captured OS/package version output. Detailed deployment instructions and compatibility notes remain in [the matrix operations document](../baml-in-prod2/01-hello-world-matrix.md).

BAML CLI/pack: `0.18.1-nightly.20260908.a`. Node bridge: `@boundaryml/baml-bridge@0.18.1-nightly.20260908.a`. Python bridge: `baml-bridge==0.18.1.dev2026090800`, with generated-code dependency `pydantic==2.11.9`.

The pure BAML rows execute `/app/hello`, built with `baml pack main`, as their container entrypoint. There is no Node/Python transport wrapper. The GNU packed binary is identical on AL2023 and Debian; Alpine uses the published musl target. AL2023 required no replacement libc, and Alpine required no glibc compatibility layer. The Node serving process was verified to load the actual `linux-x64-musl` addon. No BoundaryML/baml repository changes were made.

## Workload and controls

`GET /` returns exactly status 200 and the eleven UTF-8 bytes `hello world`, with no newline, `Content-Type: text/plain; charset=utf-8`, and `Cache-Control: no-store`. The bridge handlers call generated `hello_world_async()` on every request through a single process-wide runtime. The deterministic BAML function simply returns a string; it performs no LLM or network work. Native requests execute the BAML handler through `baml.http.Server` and construct a new `baml.http.Response`.

There is no forced GC, response caching that bypasses BAML, periodic restart policy, access logging, or profiler. `BAML_PROFILE=0` is set before runtime initialization. CLI telemetry is disabled. Fly's normal on-failure restart policy applies, with maximum ten retries; autostart can also respond to requests for a stopped machine. These restarts are failures to record, not evidence of continuous stability.

The generator is separate Fly machine `8ee335b73679e8` in `iad`, with **2 performance CPUs and 4096 MiB RAM**. It runs seven simultaneous independent [Vegeta 12.13.0](https://github.com/tsenart/vegeta/tree/v12.13.0) attacks, each with `-rate=100/1s -duration=3600s`, for **700 configured QPS aggregate**. It uses public HTTPS URLs, HTTP/1.1, keepalive, a 10-second timeout, 10 initial workers and 1000 maximum workers per attack. It validates/counts responses and discards raw per-request output; bounded summaries are stored outside source control. It survives terminal exit and does not automatically repeat the run.

Client connection settings are common. Express/Uvicorn have 60-second idle keepalive timeouts; native BAML uses Hyper's keepalive behavior because this API exposes no idle keepalive option. Fly health checks add approximately one request every 30 seconds per app. Shared CPU scheduling and the public Fly proxy/network path remain potential contributors; this is not an isolated runtime microbenchmark.

## Six-minute results

“Counted” includes completed requests and transport failures; requests still outstanding are absent. Failure percentage is relative to counted results, not the full configured schedule. Code `0` means no HTTP response. The last column is the most recent five-second **completion** rate, which can exceed 100 when queued results complete in a burst.

| Variant | Counted | HTTP 200 | Other codes | Failed fraction | Recent completion QPS |
| --- | --- | --- | --- | --- | --- |
'''+ '\n'.join(rows)+'''

These are observed results, not seven assumptions of “100 QPS delivered.” The worker ceiling and 10-second timeouts limit progress when endpoints stop responding. AL2023 and Debian have substantial outstanding or missed work relative to the approximately 36,071 requests per endpoint implied by the snapshot's elapsed wall time. Successful completion rate is materially below configured load on the failing native rows. The raw status-code and error counters are preserved in [the load snapshot](evidence/load-status.json).

All recorded HTTP 200 responses have the expected body. The collector's `body_mismatches` also counts empty/error responses on transport failures and 502s; that number is not a count of malformed successful responses. No latency, RSS time series, BAML heap trace, or GC metrics platform was built.

## Failure evidence

**Native Alpine:** Fly recorded exit code **137**, `oom_killed: true`, at **20:57:09.974 UTC**, approximately **124 seconds into the full run**, and automatically restarted the machine. A second OOM exit is recorded at **20:59:18.806 UTC**, with `restart_count: 2`. The post-restart recovery did not make the run stable: the later machine snapshot again showed a critical health check. Fly proxy logs include 502-related startup failures and Machines API rate-limit messages during automatic restarts.

**Native Debian:** Fly recorded exit code **137**, `oom_killed: true`, at **20:57:27.993 UTC**, approximately **142 seconds into the full run**, and automatically restarted the machine (`restart_count: 1` in the snapshot). A later health check was critical again. The response snapshot contains 8,654 transport failures.

**Native AL2023:** The machine remained reported as `started` but its health check was critical. Fly proxy logs reported timeouts connecting to the instance. A one-off `/proc/meminfo` read showed `MemTotal: 985220 kB`, **`MemAvailable: 0 kB`**, and no swap. The fuller process-state read stalled and was cancelled; this is a partial diagnostic snapshot, not a memory time series. Memory pressure is directly observed, but the captured event history contains no OOM kill or restart for this row. Its successful-response count remained at 11,935 while transport errors continued rising.

**Node/BAML:** The full run recorded one EOF and 30 header timeouts early, with no subsequent increase through the six-minute snapshot. Its recent rate was approximately 100 completions/second and its machine health check was passing. No full-run restart was present in captured machine events. A separate, corrected deployment-time module-format failure is described below.

**Both baselines and Python/BAML:** No recorded transport or HTTP failures in this snapshot; approximately 100 recent completions/second, passing machine health checks, and no full-run restart events. This short observation does not establish bounded memory use or exclude a later bridge GC problem.

Evidence: [first native failure events](evidence/first-native-failures.json), [later machine state](evidence/machine-state.json), and [AL2023 memory snapshot](evidence/al2023-meminfo.txt). Fly machine instance IDs can remain unchanged across automatic restarts; conclusions use exit/restart events and `restart_count`, not only instance identity.

## What this establishes—and what remains open

The requested images, architecture and bridge combinations can all build, boot, and serve the correct deterministic response. Compatibility was not the blocking result. The test also establishes that this pinned native BAML HTTP-serving path does not sustain the chosen 100-QPS workload within a 1-GiB machine without serious failures: two OS/libc variants have explicit OOM evidence, and the third has severe memory pressure and loss of service.

The pattern across GNU and musl native builds makes native request/runtime lifetime management a useful next investigation. It does **not** yet prove a particular GC bug, allocation leak, connection leak, or per-request memory slope. The experiment combines the native HTTP transport, request/response allocation, VM execution, GC, connection behavior, and Fly's runtime environment. The bridge rows exercise a different serving path. Their better early result does not isolate which component explains the difference.

A focused follow-up would inspect native request-task/VM/response reclamation, collect the deferred process/BAML memory and GC evidence, and repeat this exact matrix after a relevant fix. Preserve the same workload and image/runtime pins except for the intentional change. There is no need to broaden the matrix or build a dashboard to investigate this result.

## Setup problems kept separate from load failures

- The first Node bridge deployment used CommonJS against an ESM-only bridge package, causing `ERR_PACKAGE_PATH_NOT_EXPORTED` and up to ten startup retries. Switching to ESM/TypeScript `NodeNext` fixed it; the final image executes a real BAML call during build. This happened before the load window.
- Generated Python reflection required Pydantic even for the trivial function. A build-time BAML-call check caught the missing dependency, and adding Pydantic fixed the build before that machine was deployed.
- Fly blocked “amazon” in the app name, so the AL2023 app is named `baml-hw-0910-baml-al2023`. Its base is still exactly `amazonlinux:2023`.
- The generator's first launch attempts were rejected for a CLI digest-reference issue and the 4096-MiB minimum for two performance CPUs. Neither rejection created a running load.
- Collector attempt `20260910T205240Z` stopped almost immediately because Python 3.10 rejected nanosecond timestamps. Some requests were issued, but no valid result counts were collected. Normalizing timestamp precision fixed it. A separate two-second preflight then completed with all recorded responses 200, though startup timing left it below the 200-request-per-endpoint schedule. The actual hour began afterward as `20260910T205506Z`. These attempts are retained in the evidence directory.

## Live status and controls

This report and its evidence files are static snapshots. The original one-hour run continues until approximately **14:55 PDT**. Use these commands for the current/final result and explicit stop/restart control:

```sh
cd tools/bench/hello-world-fly
python3 scripts/load-control.py status
python3 scripts/load-control.py stop
python3 scripts/load-control.py start --seconds 3600
fly machine list -a baml-hw-0910-baml-alpine --json
fly logs -a baml-hw-0910-baml-alpine --no-tail
```

Do not start a new attack while the current one is active; the supervisor lock rejects overlapping runs. The generator's run directory is `/results/20260910T205506Z/`; it contains seven target files, bounded stderr logs, and an atomically updated `status.json`. The load machine has restart policy `no` and persistent root filesystem. After the hour, it idles and retains summaries; the seven measured apps also remain deployed. For a final artifact, capture the status after the scheduled end, retaining all errors, request deficits, and exits. A collector status of `completed` means its processes exited normally, not that every endpoint passed.

Full operational instructions: [deployment matrix](../baml-in-prod2/01-hello-world-matrix.md) and [load generation](../baml-in-prod2/02-load-generation.md). The original [design](../baml-in-prod2/00-design.md) is unchanged.
'''
(d/'00-report.md').write_text(text)
print(d/'00-report.md')
