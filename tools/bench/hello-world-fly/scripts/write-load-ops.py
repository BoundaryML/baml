#!/usr/bin/env python3
from pathlib import Path
import json,datetime
r=Path(__file__).resolve().parents[1];d=r/'artifacts/reports/baml-in-prod2'
d.mkdir(parents=True,exist_ok=True)
s=json.loads((r/'load-status.json').read_text());machines=json.loads((r/'artifacts/load-machines.json').read_text());machine=machines[0]
text=f'''# Bounded Vegeta load run

The active one-hour run is `{s['id']}`. Status at the saved observation: **{s['status']}**, elapsed {s['elapsed_seconds']} seconds. Start: **{s['start_utc']}**. Expected attack stop: approximately **{s['scheduled_end_utc']}** (September 10, 2026, 2:55 PM PDT), plus at most the final 10-second request timeout and a five-second collector update interval. Each independent attack has its own exact `-duration=3600s`, beginning within the launch window. This report verifies startup and observed delivery, not completion of the hour. The run has already exposed native-service failures, including OOM kills; it is not a stability pass.

Seven independent attacks each request 100 QPS, for 700 QPS aggregate. Each targets one public HTTPS URL from [01-hello-world-matrix.md](01-hello-world-matrix.md). No other app is targeted. The seven services remain deployed after load ends. The separate generator remains idle afterward to allow summary retrieval; stop it explicitly when no longer needed. There is no automatic repeating schedule.

## Generator identity

- App: `baml-hw-0910-load`, org `boundary`, region `iad` (same region as services, separate machine/CPU).
- Machine: `{machine['id']}`, current instance `{machine['instance_id']}`; Linux amd64, 2 performance CPUs, 4096 MiB RAM.
- Image tag: `registry.fly.io/baml-hw-0910-load:{machine['image_ref']['tag']}`; digest `{machine['image_ref']['digest']}`.
- Base: the same pinned `python:3.10-slim` amd64 digest used in the matrix. The Python process supervises and counts load results only; it is outside the seven measured services.
- Restart policy: `{machine['config']['restart']['policy']}`. Root filesystem persistence: `{machine['config'].get('rootfs',{}).get('persist')}`. No public service/port, autostop, app health-check traffic, or sharing of its CPU with measured apps.
- Supervisor PID for this run: `{s['supervisor_pid']}`. A filesystem lock rejects overlapping runs. A boot marker prevents automatic rerun after generator restart. Starting the measured service machines does not trigger load.
- Vegeta: `{s['vegeta_version'].replace(chr(10), '; ')}`. Linux amd64 archive SHA-256: `e8759ce45c14e18374bdccd3ba6068197bc3a9f9b7e484db3837f701b9d12e61`, checked in image build against the release checksums.

## Exact attack settings

The [official Vegeta documentation](https://github.com/tsenart/vegeta/tree/v12.13.0) defines rate as requests per time unit and supports bounded duration, connection limits, and response-body capture. No Prometheus exporter or profiler is enabled.

```sh
vegeta attack -name=VARIANT -targets=/results/{s['id']}/VARIANT.targets -rate=100/1s -duration=3600s -timeout=10s -workers=10 -max-workers=1000 -connections=100 -max-connections=1000 -keepalive=true -http2=false -redirects=0 -max-body=32
```

Each target file contains exactly `GET https://APP.fly.dev/` followed by a newline. Seven attack processes launch together after all seven endpoints pass exact 200/body/content-type smoke checks from the generator. Each stream pipes into `vegeta encode -to=json`; the collector validates decoded bodies and counts status codes/errors, then discards the raw result. TLS certificate verification stays enabled. HTTP/1.1 and keepalive are explicit; redirects are not followed. The 10-second timeout and 1000-worker ceiling retain evidence of stalled requests without unbounded worker growth.

| Variant | Attack PID | Responses counted | Recent completion QPS (5 seconds) | Request timestamp rate | HTTP codes | Transport errors | Body mismatches including transport failures |
| --- | --- | --- | --- | --- | --- | --- | --- |
'''
for n,x in sorted(s['targets'].items()):
 text+=f"| `{n}` | {x['attack_pid']} | {x['requests']} | {x['observed_completion_qps_last_interval']} | {x['observed_request_qps']} | `{json.dumps(x['status_codes'])}` | {sum(x['errors'].values())} | {x['body_mismatches']} |\n"
text+='''
Configured QPS is not a guarantee of observed completion rate. The recent interval counts completions; a backlog can produce a later completion burst above 100 without increasing the configured issue rate. Request timestamp rate is calculated from request timestamps on completed results and can fluctuate while outstanding requests remain. Counts divided by elapsed wall time include initial launch delay. Code `0` denotes transport failure rather than an HTTP response. Body mismatch counts include empty bodies on those failures; they do not by themselves mean a 200 response had incorrect content.

Observed early failures are retained; they have not been erased by restarting attacks or services. At the 40-second observation, Node/BAML had 31 transport failures (30 header timeouts and one EOF), and native Alpine had 40 (37 header timeouts, two EOFs and one connection reset). All seven recent completion rates had reached approximately 100 QPS; the other five variants had no errors. This evidence alone does not establish whether the failures originate in the runtime, shared CPU scheduling, or the public Fly network path. The subsequent 155-second observation recorded 2,574 AL2023 transport errors, 1,354 Debian transport errors, and 360 Alpine HTTP 502s plus 40 transport errors. Fly machine events confirm native Alpine exited 137 with `oom_killed: true` at 20:57:09.974 UTC and automatically restarted; native Debian exited 137 with `oom_killed: true` at 20:57:27.993 UTC and automatically restarted. AL2023 had a critical health check and proxy connection timeouts at that capture; no OOM event had yet been recorded for it. These observations supersede the earlier no-restart check. Machine instance IDs can remain unchanged across automatic restarts, so event histories and `restart_count` are used as evidence. See `operational-failures.json` in the source directory. Detailed memory metrics and root-cause analysis are deferred as requested.

Exact errors in the current saved observation:

```json
'''+json.dumps({n:x['errors'] for n,x in s['targets'].items() if x['errors']},indent=2)+'''
```

## Status, stop, restart, and extension

Run these commands from the source directory. They address only the dedicated load machine.

```sh
cd tools/bench/hello-world-fly
python3 scripts/load-control.py status
python3 scripts/load-control.py stop
python3 scripts/load-control.py start --seconds 3600
python3 scripts/load-control.py start --seconds 7200
```

Wait for the previous run to finish (or stop it and confirm `stopped`) before starting another. `--seconds` defaults to 3600, accepts 1 through 86400, and applies to each of the seven attacks. Extending means an explicit new bounded run; it does not alter the running Vegeta processes. Starting a new run prints its supervisor PID; check status after 10 seconds and ensure the returned run ID/PID is the new one. An early startup failure is recorded in `/results/launch.log`, capped at 1 MiB. A normal completed load leaves all measured apps and the idle generator running.

```sh
fly logs -a baml-hw-0910-load --no-tail
fly machine list -a baml-hw-0910-load --json
fly ssh console -a baml-hw-0910-load -C 'cat /results/launch.log'
# Emergency stop all load by stopping only this generator:
fly machine stop 8ee335b73679e8 -a baml-hw-0910-load
# Start its idle container again; then use load-control.py start for a new run:
fly machine start 8ee335b73679e8 -a baml-hw-0910-load
```

## Summaries and retention

The remote directory `/results/RUN_ID/` contains seven one-line target files, seven bounded stderr logs (1 MiB maximum each), and `status.json`, atomically replaced every five seconds. `/results/latest` points to the latest run directory. There are no per-request result files. At most ten run directories are permitted before manual archival/removal; no automatic deletion hides old failures. The generator startup log is replaced for each explicit restart. The root filesystem survives machine restarts; archive summaries before replacing/destroying the machine.

```sh
cd tools/bench/hello-world-fly
python3 scripts/load-control.py status > load-status.json
python3 scripts/verify.py during-load
python3 scripts/write-load-ops.py
```

The local `load-status.json` and copied `hello-world-load-status.json` are snapshots, not live views. Use the status command for current/final counts, timestamps, and process exit codes. A completed hour should report approximately 360,000 requests per endpoint (2.52 million total); retain any deficit, transport/HTTP errors, and nonzero exits. No promise of zero loss is implied by `completed`, which describes normal generator exit.

## Implementation issues and prior attempts

- Fly CLI 0.4.100 `machine run` duplicated a digest when given a digest-only registry reference, producing an invalid image identifier before any machine launch. Using the immutable deployment tag succeeded; machine metadata verifies the resulting digest.
- Two performance CPUs require at least 4096 MiB on Fly. The initial 2048 MiB launch was rejected without creating a machine. The final generator has 4096 MiB. All measured apps stayed at 1024 MiB and one shared CPU.
- The initial CLI `--autostop off` spelling passed an unused `off` argument to the entrypoint; the no-service generator has no autostop service to configure. Future launch commands should use `--autostop=off` or omit it. The entrypoint does not use this argument.
- The first collector attempt, `20260910T205240Z`, failed at startup because Python 3.10 `datetime.fromisoformat` rejected Vegeta's nanosecond fractions. It stopped its attack children and was marked `failed`. Some requests were issued, but its zero collector counts are not proof of zero traffic. The parser now normalizes fractional seconds to microseconds. Only the dedicated generator image was updated; its boot marker prevented an automatic retry.
- A bounded two-second preflight `20260910T205420Z` then exited cleanly with all recorded responses 200 and exact bodies. Counts were Node baseline 184, Node/BAML 180, Python baseline 173, Python/BAML 130, AL2023 199, Debian 198, Alpine 196. The short preflight missed part of the 200-request schedule during startup; it is retained separately and is not the one-hour result.
- The full run `20260910T205506Z` started afterward and retains its actual early transport failures. No measured app was restarted to conceal these errors.
'''
(d/'02-load-generation.md').write_text(text)
(d/'hello-world-load-status.json').write_text(json.dumps(s,indent=2)+'\n')
(r/'load-machine.json').write_text(json.dumps(machine,indent=2)+'\n')
for src,dst in [('load-initial-failed.json','load-initial-failed.json'),('load-preflight.json','load-preflight.json')]:
 (r/dst).write_text((r/'artifacts'/src).read_text())
