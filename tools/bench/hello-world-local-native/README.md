# Native macOS hello-world longevity harness

This harness compares Node and Python baselines, their BAML bridge equivalents, and a packed BAML executable without Docker. Every process is a native macOS arm64 release artifact and every target receives an independent Vegeta attack over loopback. The default run sends 300 requests/second to each target for five minutes, or 1,500 RPS total.

Each `GET /` returns exactly `hello world` with HTTP 200, `Content-Type: text/plain; charset=utf-8`, and `Cache-Control: no-store`. The two bridge variants call the deterministic BAML function through their process-wide runtimes on every request. The packed BAML target exercises the same function and BAML HTTP server without a language bridge.

## Build

Prerequisites are macOS on Apple Silicon, Rust and the repository's mise toolchain, Node 20, `uv`, npm, and network access for pinned dependencies and Vegeta 12.13.0. `build.py` compiles the CLI, pack host, Python bridge, and Node bridge from one selected `baml_language` workspace with the same `BAML_GIT_SHA`. It records the source revision, dirty state, platform, runtimes, and artifact hashes in `.build/manifest.json`.

```sh
cd tools/bench/hello-world-local-native
python3 scripts/build.py --baml-source /path/to/baml-worktree/baml_language
```

The default source is this repository's `baml_language` directory. Pass another worktree to compare a candidate fix. Generated SDKs, native addons, wheels, virtual environments, dependencies, and load tools remain under ignored `.build/`. A cold release build takes several minutes; after a completed build from the same exact source snapshot, `--skip-rust` reuses the matching native artifacts and refreshes the generated apps. The snapshot fingerprint includes tracked changes and nonignored untracked files, so two dirty states at the same commit cannot silently share artifacts.

The current Node bridge wrapper has pre-existing `HandleKey` versus protobuf `Long` declaration errors. The build follows the release experiment workflow and transpiles that wrapper with `tsc --noCheck`; it type-checks the generated demo SDK normally and exercises the resulting native addon during the run.

## Run

```sh
python3 scripts/run.py --rate 300 --duration 300
python3 scripts/run.py --rate 100 --duration 900
python3 scripts/run.py --rate 300 --duration 0  # continue until Ctrl-C
python3 scripts/run_cycled.py --rate 100 --duration 300 --on-seconds 30 --off-seconds 1
```

The runners own every child process, verify each exact response before load, probe each target during sampling, capture Vegeta HTTP status counters, sample native RSS and CPU with `ps`, and shut down only the processes they started. `run_cycled.py` keeps the applications alive while repeatedly starting load for the requested on period and leaving them idle for the requested off period. Results go to `results/<timestamp>-<revision>-<rate>rps/`, including the build manifest, configuration, process logs, samples, and a machine-readable summary with delivered RPS and an RSS slope fitted after the first minute.

| Variant | URL | Load metrics |
| --- | --- | --- |
| Node 20 / Express | http://127.0.0.1:8501/ | http://127.0.0.1:8611/metrics |
| Node 20 / Express + BAML | http://127.0.0.1:8502/ | http://127.0.0.1:8612/metrics |
| Python 3.10 / Starlette | http://127.0.0.1:8503/ | http://127.0.0.1:8613/metrics |
| Python 3.10 / Starlette + BAML | http://127.0.0.1:8504/ | http://127.0.0.1:8614/metrics |
| Packed BAML | http://127.0.0.1:8505/ | http://127.0.0.1:8615/metrics |

The harness does not impose a per-process memory limit on macOS, so it measures sustained RSS growth rather than an artificial 1 GiB OOM boundary. Vegeta validates HTTP statuses for every completed request; the runner checks the exact body and headers before load and on each sampling interval. Stop unrelated heavy workloads before comparing slopes. In particular, stop only the Compose load-generator services if the Docker longevity harness is still running so its apps, Grafana, Prometheus, and historical data remain available.

## Initial native reproduction

The first 15-minute run used the combined PR #4847 + PR #4811 worktree at `42365a45f9d7a3750ed72bd338006d369f6c0e41`, macOS arm64, Node 20.14.0, Python 3.10.0, and 300 RPS per target. All five targets stayed alive and returned only HTTP 200 responses, with approximately 272,400 completed requests each. After excluding the first minute, Node bridge RSS grew 16.46 MiB/min and Python bridge RSS grew 16.45 MiB/min. Python baseline was flat, Node baseline grew 0.31 MiB/min, and packed BAML grew 1.27 MiB/min. This reproduces the bridge-specific memory growth without Docker; [the retained summary](evidence/2026-09-11-pr4847-4811-300rps/summary.json) contains the measured values.

## Quiet-window control without the background-GC fix

A paired five-minute experiment rebuilt the native artifacts once from the same clean pre-#4870 commit `42365a45f9d7a3750ed72bd338006d369f6c0e41`, then ran 100 RPS per target continuously and with 30 seconds on followed by one second off. The cycled run started 10 load periods, accumulated 290.55 loaded seconds, delivered about 29,000 HTTP 200 responses per target at 100 RPS, and kept every target healthy. The continuous control delivered 30,000 HTTP 200 responses per target at 100 RPS and also kept every target healthy.

The comparison uses the change between median RSS in minutes 2–3 and minutes 4–5, divided by two minutes, so one abrupt Node allocator release near the end of the continuous run does not dominate the result. Node BAML grew 4.94 MiB/min continuously and 1.03 MiB/min with quiet windows; Python BAML grew 5.32 MiB/min continuously and 0.14 MiB/min with quiet windows. Over the same windows, the Node baselines measured 1.14 and -0.50 MiB/min respectively, while both Python baselines were within 0.03 MiB/min of flat. In the cycled run, Node BAML moved from 163.69 MiB at two minutes to 166.64 MiB at the last sample, and Python BAML moved from 78.95 MiB to 79.23 MiB.

This is evidence that regular one-second gaps let the existing 100 ms idle collector prevent the persistent bridge growth seen under continuous load. It rules out the original roughly request-proportional leak over this five-minute profile, but it does not prove that a slower leak cannot appear in a longer run. The retained [paired comparison](evidence/2026-09-14-pre4870-cycled-vs-continuous-100rps/comparison.json), [cycled summary](evidence/2026-09-14-pre4870-cycled-vs-continuous-100rps/cycled-summary.json), and [continuous summary](evidence/2026-09-14-pre4870-cycled-vs-continuous-100rps/continuous-summary.json) record the derived measurements and request validation; both raw sample streams and build manifests are retained alongside them.

## Leak investigation and fix validation

The shared Node and Python slope suggested a request-proportional allocation below both language adapters. A direct `bex_engine` scalar workload confirmed that hypothesis: 100,000 calls allocated no BAML heap slots but increased RSS from 279.53 to 368.64 MiB. Each call created a short-lived VM and registered a weak root holder in `HeapPermitManager::new_permit`. Dead registrations were removed only when `request_park` initiated GC, so a scalar function that never allocated BAML heap objects also never cleaned the registry.

Packed baml-only programs do not use a separate server-specific collector. Their HTTP path invokes each handler through `VmSpawner::spawn_with_callable` in one shared engine, and materializing BAML request and response objects charges the engine's 32 MiB allocation budget. That pressure eventually makes a VM call the ordinary engine collector at a safepoint. The scalar bridge path does not allocate runtime BAML objects, so it never reaches the same trigger even though it creates and retires VM root holders.

The revised candidate extends the engine-owned background worker instead of adding cleanup work to permit allocation. Completed work first gets the existing 100 ms quiet-time collection opportunity. An independent five-second maximum deadline cannot be postponed by continuous calls; when it expires, the worker sets the same `park_requested` flag used by allocation-pressure GC, waits for VMs at their existing safepoints, and runs the same major collection. `HeapPermitManager::new_permit` remains unchanged.

The five-minute end-to-end validation rebuilt both native bridges from clean commit `bc7d504ba456fdb7888d0b3c00a5fade0403941d` and sent 300 RPS independently to every target. Node BAML measured 1.01 MiB/min after warmup versus 0.93 MiB/min for Node baseline, down from 16.46 MiB/min before the fix. Python BAML measured 0.20 MiB/min versus 0.005 MiB/min for Python baseline, down from 16.45 MiB/min. Every BAML target stayed alive, passed every exact-response probe, and returned only HTTP 200 responses. The Node baseline recorded one status `0` among 89,980 attempts, so the runner returned nonzero even though all 89,979 completed baseline requests were HTTP 200. [The retained background-GC summary](evidence/2026-09-14-background-gc-300rps/summary.json) contains the complete measurements. The earlier [permit-sweep experiment](evidence/2026-09-11-holder-sweep-fix-300rps/summary.json) remains as superseded evidence.
