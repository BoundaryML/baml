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
```

The runner owns every child process, verifies each exact response before load, probes each target during sampling, captures Vegeta HTTP status counters, samples native RSS and CPU with `ps`, and shuts down only the processes it started. Results go to `results/<timestamp>-<revision>-<rate>rps/`, including the build manifest, configuration, process logs, 15-second samples, and a machine-readable summary with delivered RPS and an RSS slope fitted after the first minute.

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

## Leak investigation and fix validation

The shared Node and Python slope suggested a request-proportional allocation below both language adapters. A direct `bex_engine` scalar workload confirmed that hypothesis: 100,000 calls allocated no BAML heap slots but increased RSS from 279.53 to 368.64 MiB. Each call created a short-lived VM and registered a weak root holder in `HeapPermitManager::new_permit`. Dead registrations were removed only when `request_park` initiated GC, so a scalar function that never allocated BAML heap objects also never cleaned the registry.

The candidate fix periodically removes dead weak registrations while adding permits, bounding retained rows even when no GC occurs. The same 100,000-call engine test stayed at 280.14 MiB after the fix, with 0.03 MiB total measured growth, and the dedicated `bex_heap::short_lived_permits_are_swept_without_gc` regression test passed.

The required five-minute end-to-end validation rebuilt both native bridges from the fixed combined worktree and sent 300 RPS independently to every target. Each variant completed 89,907 requests with only HTTP 200 responses. Node BAML measured 0.51 MiB/min after warmup versus 0.47 MiB/min for Node baseline, down from 16.46 MiB/min before the fix. Python BAML measured 0.038 MiB/min versus a flat Python baseline, down from 16.45 MiB/min. The fix removes the bridge-specific request-proportional growth over this test window; [the retained fixed-run summary](evidence/2026-09-11-holder-sweep-fix-300rps/summary.json) includes the engine isolation and end-to-end results.
