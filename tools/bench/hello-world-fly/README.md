# Six Fly hello-world variants

The Fly counterpart to [the local longevity harness](../hello-world-local/README.md) lives in `tools/bench/hello-world-fly`. All six service sources live in explicit sibling directories. `manifest.json` records pinned amd64 base images and deployed machine/image identities; its `source` paths resolve relative to this directory. The Fly images use the pinned published BAML nightly, independently of the local harness’s custom heap-metrics build. The two baselines contain no BAML dependency. Bridge handlers call an async deterministic BAML function on each request through one process-wide runtime. Pure BAML services execute the Linux binary produced by `baml pack main`; BAML's native HTTP server is the complete serving path.

`GET /` returns status 200 and the eleven UTF-8 bytes `hello world`, with no trailing newline, `Content-Type: text/plain; charset=utf-8`, and `Cache-Control: no-store`. There is no response cache or per-request access logging. HTTP/1.1 persistent connections are used by the load runner. Express and Uvicorn have a 60-second idle keepalive timeout; BAML uses its native Hyper keepalive behavior because this CLI exposes no idle keepalive parameter. Fly TLS/proxy behavior and request concurrency limits are common to all six apps. Native BAML explicitly disables HTTP/2 on its internal listener. Fly health checks add one request per approximately 30 seconds per machine beyond the configured load.

All apps use one `iad` x86_64 machine with 1 shared CPU and 1024 MiB RAM. Autostop is disabled. Fly's normal on-failure policy (maximum ten retries) applies; no scheduled restarts or forced GC are configured. `BAML_PROFILE=0` is set before native initialization except in `baml-debian-telemetry`, which sets `BAML_PROFILE=1`. Both Debian variants use the same pinned base image, nightly, packed binary, source, and machine resources. The telemetry variant writes runtime execution profiles under `/app/.baml/profiles-v1`; no profile directory override or cleanup policy is applied. Both retain CLI usage telemetry opt-out `BAML_TELEMETRY_DISABLED=1` and `BAML_LOG=off`: CLI analytics are separate from runtime profiling. Build stages keep profiling disabled. The deterministic functions perform no LLM or network work. The comparison dashboard uses existing Fly-managed Grafana metrics.

From the BAML repository root, run `cd tools/bench/hello-world-fly`. Deployment and load-control commands below operate the existing remote Fly apps; copying or editing this directory does not deploy anything. The observations below are historical snapshots, not a freshly verified account of live health.

Run `python3.12 scripts/check.py` to validate the active matrix locally without deploying or generating traffic.

Run from this directory:

```sh
python3 scripts/deploy.py                 # deploy all six; use variant names to select
python3 scripts/deploy.py node-baml       # deploy only this variant
python3 scripts/verify.py smoke           # exact response and one-machine verification
python3 scripts/load-control.py status
python3 scripts/load-control.py stop
python3 scripts/load-control.py start     # continuous until explicitly stopped
fly logs -a baml-hw-0910-node-baml --no-tail
fly machine list -a baml-hw-0910-node-baml --json
```

The load runner refuses overlapping runs and defaults to continuous operation (`-duration=0s`). Its six independent Vegeta 12.13.0 attacks each use `-rate=10/1s` (60 RPS aggregate); all target public HTTPS URLs even when unhealthy or crashing. Each raw result is decoded, checked, counted, then discarded. Summaries and rotating diagnostic logs are bounded outside source control. The generator occupies one separate machine with 8 performance CPUs and 16 GiB RAM, and automatically recovers its own failures. `stop` disables further load across generator reboots; `start` enables continuous load. Optional `start --seconds 3600` applies to the next run after stopping the active run.

All six measured apps have a fixed count of one machine, no autoscaler, and autostop disabled. Proxy autostart and normal on-failure restarts can restart that same VM; they cannot add containers. Deployments use `--ha=false --strategy immediate` and verify exactly one machine. The load VM is updated by its existing machine ID, never cloned. The recorded continuous-run operations are preserved in [02-continuous-load.md](artifacts/reports/baml-in-prod3/02-continuous-load.md).

Saved design, manifests, and operational evidence are under `artifacts/reports/baml-in-prod2/` and `artifacts/reports/baml-in-prod3/`. The `scripts/write-ops.py`, `write-load-ops.py`, and `write-prod3-report.py` report writers now target those local directories instead of the original external notes repository. Local builds, generated SDKs, dependencies and raw deployment logs are ignored before creation. Deployment commands target only the named harness apps.

The initial load exposed native Alpine/Debian OOM kills and AL2023 memory pressure/timeouts. The interim findings and retained evidence are in [00-report.md](artifacts/reports/baml-in-prod3/00-report.md); successful deployment is not a stability pass.


Python and Node variants expose private process/runtime memory metrics on port 9091 for Fly to scrape every 15 seconds. Node exports RSS and V8 heap/external memory; Python starts `tracemalloc` with one frame and exports RSS, traced current/peak allocations, tracking overhead, and GC collection counts. Python tracing adds overhead. The four app deployments begin a new measurement phase; pure BAML apps and the continuous generator remain unchanged. See [03-runtime-memory-metrics.md](artifacts/reports/baml-in-prod3/03-runtime-memory-metrics.md) for exact semantics, new image identities, verified dashboard values, and controls.


The generator was subsequently redeployed at 10 RPS per endpoint. Recorded run/image identity and verification are in [05-load-10rps.md](artifacts/reports/baml-in-prod3/05-load-10rps.md); earlier reports describe the 100-RPS phases.

## Local migration

The complete source tree was copied from `~/work-repos/baml-demos/2026-09-10-fly-hello-world` into this worktree and verified before adapting paths: 505 files and symlinks, totaling 22,807,520 file bytes. This includes all seven variants, generated SDKs, cached BAML output, build/deployment artifacts, lockfiles, machine/run snapshots, load-generator controls, Grafana specifications, and report scripts. The linked `baml-in-prod2` and `baml-in-prod3` notes and evidence were also copied under ignored `artifacts/reports/`. The inventories are `artifacts/migration-inventory.json` and `artifacts/migration-notes-inventory.json`; the original manifest is preserved at `artifacts/pre-migration-manifest.json`.

The original demo and notes remain intact. Historical evidence keeps its original timestamps, image/machine identities, and any historical absolute paths. Generated outputs, local config, and artifacts remain ignored. The migration itself did not alter deployed services, remote load, or Grafana. The subsequent Debian telemetry rollout described above updates the Fly source, deployment, generator, and dashboard. The saved failures remain evidence of experiment instability; no fresh runtime pass is claimed.

## Debian telemetry comparison

The active matrix contains the four Node/Python variants plus `baml-debian` (runtime telemetry off) and `baml-debian-telemetry` (runtime telemetry on). The native `baml-alpine` and `baml-amazonlinux` apps are retired and removed from deployment and load targets. Historical JSON snapshots and report writers retain the original seven-variant observations. The current manifest, load targets, and Grafana specification describe six variants.

The [live dashboard](https://fly-metrics.net/d/baml-hw-0910-matrix?orgId=721257&refresh=30s&from=now-1h&to=now) retains its UID and compares HTTP status counts, latency, normalized VM load, and VM memory for both Debian variants. Profiling data stays inside the telemetry VM; the dashboard uses existing Fly metrics. VM total minus available memory is not BAML heap usage.

The telemetry app passed the exact-response smoke check and created its runtime profile store. The existing telemetry-off Debian app timed out during the same check, so this rollout does not establish stability. Rollout snapshots and deployment logs are retained locally under ignored `artifacts/telemetry-rollout/`.
