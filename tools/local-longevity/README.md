# Local longevity harness

Five hello-world handlers compare Node, Python, their local BAML bridges, and packed BAML under sustained load in native Linux arm64 containers. This harness was migrated from the September 10, 2026 local hello-world experiment in `baml-demos`; the independent Fly experiment is not part of this repository. Each measured container has a hard 1 CPU quota, 1 GiB memory limit, and no swap. Five containers run the stock Vegeta 12.13.0 executable directly and send **100 requests/second each**, continuously by default (500 RPS total).

`GET /` returns exactly `hello world` (11 bytes, no newline), `Content-Type: text/plain; charset=utf-8`, and `Cache-Control: no-store`. Both bridges await the deterministic BAML function on every request through their process-wide runtime. The native variant runs the executable produced by `baml pack main`. There are no LLM calls, response caches, per-request logs, forced collections, scheduled restarts, or autoscaling.

## Build and start

Prerequisites on macOS: Rust/rustup, Zig, `uv`, Node/npm, Docker with Compose, and a compatible BAML source checkout. **The heap builtin requires [PR #4837](https://github.com/BoundaryML/baml/pull/4837), which is still open at migration time.** Until it lands, pass `--baml-source /path/to/heap-metrics-checkout/baml_language`; the default is this repository’s `baml_language` directory. The preserved local artifacts were built from that PR’s checkout at `77c5356622582b5832033871cc7f0e9a03ee7bd1`, including its uncommitted changes. This migration does not include the runtime implementation. The build installs pinned `cargo-zigbuild==0.23.2` into `.build/build-tools`; the older 0.22 tool cannot link this checkout with Rust 1.98 on arm64. Rustup installs the Linux GNU and musl targets for the checkout's Rust toolchain. A cold release build can take several minutes.

```sh
# From the BAML repository root:
cd tools/local-longevity

# Dedicated Docker VM with capacity for the apps, generator, and monitoring.
colima start baml-hello-world --cpu 10 --memory 16 --disk 40 --vm-type vz --activate=false

python3 scripts/build.py --baml-source /path/to/heap-metrics-checkout/baml_language
./scripts/compose.sh up -d --build
python3 scripts/verify.py --seconds 60
```

The wrapper selects `colima-baml-hello-world` explicitly and does not change your active Docker context. To use another arm64 Docker daemon, set `LOCAL_DOCKER_CONTEXT=your-context` for both the wrapper and verification script. Give the daemon at least 10 CPUs and 12 GiB RAM; per-container limits alone cannot prevent competition in an undersized VM. Direct `docker compose` also works when pointed at the intended daemon.

Every Dockerfile has one runtime stage. Rust compilation, BAML packaging and SDK generation, TypeScript compilation, Node dependency preparation, and Linux wheel downloads happen on the macOS host. Image assembly only copies artifacts and installs runtime OS packages or prebuilt Python wheels. No compiler or BAML CLI is used inside a container. Base image indexes are pinned in the Dockerfiles, Compose, and `images.lock.json`.

## Endpoints and controls

| Experiment | Endpoint |
| --- | --- |
| Node 20 / Express | http://localhost:8081/ |
| Node 20 / Express + BAML | http://localhost:8082/ |
| Python 3.10 / Starlette | http://localhost:8083/ |
| Python 3.10 / Starlette + BAML | http://localhost:8084/ |
| BAML / Debian 12 | http://localhost:8086/ |

- Dashboard: http://localhost:3000/d/baml-local-hello-world
- Prometheus: http://localhost:9090
- Vegeta metrics: scraped directly from each `load-<experiment>:9090/metrics` inside the project network; explore `request_seconds_count` and `request_seconds_bucket` in Prometheus.
- Local evidence: `results/<project-name>/verification.json` and `.build/manifest.json`.

```bash
./scripts/compose.sh ps
# Compose service names for the five independent Vegeta processes.
load_services=(load-node-baseline load-node-baml load-python-baseline load-python-baml load-baml-debian)
./scripts/compose.sh logs --tail 20 "${load_services[@]}"
./scripts/compose.sh stop "${load_services[@]}"       # Stop load; retain apps and charts.
./scripts/compose.sh start "${load_services[@]}"      # Begin new continuous attacks.
./scripts/compose.sh down                      # Stop/remove this stack; retain measurements.

# Rebuild from changes in BAML, then recreate containers with the new artifacts.
python3 scripts/build.py --baml-source /path/to/heap-metrics-checkout/baml_language
./scripts/compose.sh up -d --build

# One finite run; a successful exit is not automatically restarted.
DURATION_SECONDS=120 ./scripts/compose.sh up -d --force-recreate "${load_services[@]}"
# Return to continuous load.
DURATION_SECONDS=0 ./scripts/compose.sh up -d --force-recreate "${load_services[@]}"
```

Copy `.env.example` to `.env` to change the load rate, duration, or published ports. The verification script reads the expected rate from the selected Compose configuration, requires the running Vegeta command to match, and discovers the selected project's actual ports through Compose. `APP_PORT_PREFIX=808` publishes apps on 8081–8084 and 8086; `APP_PORT_PREFIX=818` uses 8181–8184 and 8186. All published ports bind to loopback. Grafana is provisioned for anonymous viewing on localhost.

## Simultaneous copies

Use a distinct Compose project name and unused host ports for each copy. Compose scopes containers, networks, built image names, and monitoring volumes by project. The Docker metrics collector filters by the resolved `COMPOSE_PROJECT_NAME`, including when selected with `-p`; targets resolve only within their own project's network. The build artifacts and read-only dashboard/config files are shared when running from the same directory. Verification reports are stored under `results/<project-name>/`.

For a **new experiment**, create the ignored settings files from the committed templates (do not overwrite settings for an existing run):

```sh
cp -n .env.example .env
cp -n .env.10rps.example .env.10rps
```

Set `GRAFANA_DASHBOARDS_DIR=./grafana/dashboards-comparison` in `.env` for the four all-variant comparison charts on port 3000. The original 23-panel dashboard remains the default template choice and is explicitly selected by `.env.10rps.example`. Both dashboards use the same UID within their separate Grafana instances.

```sh
# Start/recreate the 100 RPS project: apps 808x, Grafana 3000, Prometheus 9090.
./scripts/compose.sh --env-file .env -p baml-local-hello-world up -d --build
python3 scripts/verify.py --env-file .env -p baml-local-hello-world --seconds 60

# Start/recreate the second project: apps 818x, Grafana 3100, Prometheus 9190.
./scripts/compose.sh --env-file .env.10rps -p baml-local-hello-world-10rps up -d --build
python3 scripts/verify.py --env-file .env.10rps -p baml-local-hello-world-10rps --seconds 60

# Read current state without starting or restarting anything.
./scripts/compose.sh --env-file .env -p baml-local-hello-world ps -a
./scripts/compose.sh --env-file .env.10rps -p baml-local-hello-world-10rps ps -a
```

The second dashboard is at http://localhost:3100/d/baml-local-hello-world. Pass the same env file and project name whenever operating a stack. The migrated `.env.10rps` intentionally retains **80 RPS**, the last attempted ramp rate, and its load generators are stopped; the template starts a fresh experiment at 10 RPS. `up` resumes workloads and may recreate containers with destination bind mounts, so it is a deliberate new run operation, not a migration verification step. To resume specifically at 10 RPS, first set `RATE_PER_TARGET=10` in that stack’s env file so verification and future recreations agree.

Published port values must be unique, and the shared Docker VM needs enough CPU/RAM for all running copies. For different BAML builds, use separate harness directories/checkouts so their `.build` artifacts are independent.

`build.py --skip-rust` reuses the existing CLI, pack host, and Python extension after a completed build from the same checkout and revision. The Node binding still runs napi-rs through its incremental Cargo cache to regenerate matching declarations and its loader. Use a normal build after modifying BAML source. The script pins one `BAML_GIT_SHA` fingerprint across all compilation stages so committing during a build cannot make the CLI and runtime reject one another’s bytecode. Build outputs, generated SDKs, dependencies, and results are ignored by version control.

The current local Node bridge wrapper has TypeScript errors between native `HandleKey` and protobuf `Long` declarations. The build transpiles that wrapper with `tsc --noCheck`, preserving its runtime source, and type-checks the generated demo SDK normally. Runtime verification exercises the resulting local addon. This is not a claim that the upstream wrapper passes its type checks.

## Ensuring the runtime is local

The installed `baml` wrapper rejects `[toolchain] path` in repository manifests. This demo instead invokes the locally built CLI explicitly: `scripts/baml.sh` selects `.build/toolchain/baml-cli`, a copy of the selected checkout’s `target/release/baml-cli`. For example, run `../scripts/baml.sh check --agent-skill-check off` from `baml-debian/`. The build script also invokes the local CLI directly, and leaves your global toolchain selection unchanged. Cross-compiled hosts and bridges live under the selected checkout’s `target/local-hello-world/<linux-target>/release`. The Debian executable and Python extension target GNU glibc 2.34. Node uses musl on its Alpine base image; Node's shared library build disables Rust's static CRT default.

**A local CLI alone is insufficient:** `baml pack --target` normally downloads a published target runtime. The build script packages the locally cross-compiled `baml-pack-host` in a checksummed archive and exposes it on an ephemeral loopback HTTP server through `BAML_PACK_HOST_RELEASE_BASE_URL`. The server closes after packing. No published BAML runtime is substituted. Both bridge wrapper sources, the Python ABI3 extension, and the Node native addon come from the same local checkout as the compiler and SDKs; neither app installs a released BAML bridge package. `.build/manifest.json` records artifact SHA-256 hashes and the checkout revision, with a note that uncommitted source changes are included.

## Measurements and limits

The provisioned Grafana dashboard has five rows and four columns, following the Fly dashboard's comparison layout: Throughput, Response latency (p50/p99), CPU usage, and Memory usage. Prometheus scrapes every five seconds and retains at most seven days or 2 GB. Docker logs rotate. Vegeta exposes its built-in Prometheus counters and histogram directly, writes binary results to `/dev/null`, and retains no raw result files. There is no Python request runner, result parser, custom load metric exporter, or `/status` endpoint.

Vegeta uses HTTP/1.1 keepalive, no redirects, a 10-second timeout, a one-second DNS cache to follow container IP changes after restarts, and independent 100 RPS schedules with up to 2,000 workers per target. Slow or failed services keep receiving attempted load. Throughput shows actual HTTP statuses (excluding status `0`) plus a separate transport-error rate derived from `request_seconds_count{status="0"}`. There is no body-mismatch metric; `verify.py` separately checks exact bodies and headers. Use `request_seconds_count` for failures: Vegeta 12.13.0's separate `request_fail_count` exporter creates error series without incrementing them.

Response latency shows only p50 and p99, including failed requests. Vegeta's built-in histogram starts at 5 ms, so sub-5-ms quantiles are coarse estimates. CPU usage shows cgroup CPU time per second as a fraction of one core. The actual 1 CPU and 1 GiB limits remain enforced; their reference lines and the throttled-time series are omitted from the charts.

Memory charts use blue (`#5794F2`) for Process RSS and green (`#73BF69`) for Heap used. Node heap used is V8 heap; Python heap used is current tracemalloc allocations, which exclude the native heap. Both Python variants retain the same tracing as Fly. Debian shows only Process RSS, read from the serving process's `/proc/<pid>/status` through a read-only mount of the Linux Docker host's `/proc`; BAML slot counters appear in separate panels below. No cgroup working-set value is relabeled as RSS. RSS and heap scopes overlap and must not be added.

The three BAML variants export `baml.sys.heap_stats()` from their serving runtime on the private port 9091, using the builtin introduced in PR #4837. Grafana adds three panels for runtime/compile-time object slots, active handles, and nursery reservations. These are gauges in count units, not heap-used bytes: unused allocation reservations and objects awaiting collection are included, while separately allocated payload storage is excluded. Sampling does not force GC, but each scrape allocates its result and the native scrape also executes a BAML HTTP handler. See [HEAP-METRICS.md](HEAP-METRICS.md) for the API and measurement scope.

The Docker metrics collector filters the five labeled containers by project and uses read-only Docker API requests plus the read-only `/proc` mount. The measured apps retain Docker's `on-failure:10` restart policy. Each Vegeta container restarts after an internal failure and exits successfully after a finite run. `verify.py` checks exact responses and persistent connections, actual arm64 architecture and hard quotas, all five delivered rates from fresh Prometheus scrape counts, zero new failed requests or restarts, and dashboard/metric availability. It fails on an unhealthy experiment rather than hiding it.

This reproduces the experiment workload and runtime variants, with local arm64 CPUs, a shared Linux VM kernel, and direct Docker networking. Fly uses amd64 Firecracker VMs and its public HTTPS proxy. Local latency and cgroup CPU/memory measurements have different scopes from Fly's proxy latency, VM load average, and VM memory; they are not interchangeable absolute measurements. The Fly demo and its running load are independent of this stack.

## Initial run: September 10, 2026 (before reducing to five experiments)

The initial run included seven experiments. The Amazon Linux and native Alpine variants were subsequently removed; the historical reports below retain that original run.

The local checkout at `92776b5efc32248086d24ae1406955726e60a739`, built with Rust 1.98.0 in release mode, reproduces native memory growth. Docker recorded an OOM kill for Alpine about 138 seconds after load began, and for Debian and Amazon Linux about 150 seconds after load began. All three restarted within their unchanged 1 GiB limits. The Node and Python variants had no runtime OOM events during setup; Node packaging failures during setup are visible in the early charts and were corrected before verification.

All seven subsequently passed a 60-second verification interval at approximately 100 RPS each, with exact responses, zero new errors/body mismatches/restarts, verified arm64 architecture and hard quotas, and live metrics across all 28 panels. That interval began after the first native OOM/restarts and does **not** establish sustained stability. `results/setup-report.json` combines artifact hashes, Docker OOM timestamps, and that verification interval; `results/container-events.jsonl` retains the Docker event evidence. Continuous load remains enabled so the dashboard can show subsequent memory growth and failures.

## Direct Vegeta and multiple-copy verification

Both the default project and a temporary `hello-world-copy-check` project passed 15-second checks at approximately 100 RPS per experiment with 1 CPU / 1 GiB quotas. Their networks, monitoring volumes, and collector project filters were verified as independent. The temporary stack was removed after validation. Reports remain in `results/<project-name>/verification.json`. The original Debian container later exhausted its ten OOM restarts and was manually started once; this refactor does not fix native runtime memory growth.

## Three-minute load ramp

**This command restarts workloads.** Run `python3 scripts/ramp.py` to test the `.env.10rps` project at 10, 20, 40, 80, 160, 320, 640, and 1280 RPS per experiment for three minutes each. This orchestrates the existing stock Vegeta containers. It restarts the five apps once at the beginning, then changes only the Vegeta processes between stages. It stops this project’s load at the first Debian exit/restart, transport error, or sustained throughput below 95% of target, and preserves monitoring and evidence under `results/baml-local-hello-world-10rps/ramp-<timestamp>/`. The original 100 RPS project remains independent. The env file records the last attempted rate; set `RATE_PER_TARGET=10` in `.env.10rps` before `./scripts/compose.sh --env-file .env.10rps up -d` to resume at 10 RPS.

This measures failure during accumulated load, not a fresh-process throughput ceiling. Before the ramp, the continuously running 10 RPS Debian container was OOM-killed after approximately 25 minutes 18 seconds. Memory growth and total processed requests must be considered when interpreting the failing stage.

The completed ramp passed three-minute stages at 10, 20, and 40 RPS, then Docker OOM-killed Debian 30.6 seconds into the 80 RPS stage (9 minutes 33 seconds into the ramp). Sampled working set reached 1019 MiB; RSS grew approximately 68–70 KiB per completed request during the earlier stages. This supports cumulative memory growth rather than an 80 RPS throughput limit. All five load services in the second project were stopped at failure, while the apps, dashboard, and original 100 RPS project remained running. Full evidence is in `results/baml-local-hello-world-10rps/ramp-20260910-172805/`.

## Migration and retained local evidence

The repository destination is `tools/local-longevity`; the Orca worktree is `local-longevity-test`. All 33,649 source files and symlinks (805,182,444 file bytes) were copied and verified by SHA-256, symlink target, and file mode before adapting source and documentation. `.build/`, generated SDKs, dependency trees, `.env`, `.env.10rps`, and historical `results/` remain available locally and ignored by Git. The inventory is `results/migration-inventory.json`; the original build fingerprint and artifact hashes remain in `.build/manifest.json`. No local env file or bulky build output belongs in the PR. Copied Python build-tool environments can contain absolute shebangs referencing the retained source; recreate `.build/build-tools` with `uv venv` and reinstall the pinned build tool when rebuilding after retiring that source.

The original `~/work-repos/baml-demos/2026-09-10-local-hello-world` directory is retained unchanged because both running projects still bind-mount its Prometheus and Grafana configurations. Keep it until an intentional stack recreation switches those mounts to this harness. Existing project names preserve the named Prometheus/Grafana volumes. No workload or monitoring container was restarted by the migration, and no volume was removed. Editing a destination dashboard does not update the retained source mount in a currently running container.

The handoff recorded `node-baml`, `python-baml`, and `baml-debian` exited with code 1 on the default stack before the dashboard change; those BAML throughput values were zero and CPU/RSS were historical. At migration inspection all five default apps were running again; the second stack’s five load generators were exited with code 0 after the ramp. This changed state does not erase the earlier failure evidence or establish longevity. Read-only container snapshots and validation output are retained under `results/migration/`. Historical successful intervals above must not be interpreted as continuous stability.

## Validation without starting workloads

```sh
python3 scripts/check.py
./scripts/compose.sh --env-file .env.example config --quiet
./scripts/compose.sh --env-file .env.10rps.example config --quiet
# Requires the preserved or rebuilt local CLI; does not start a server.
(cd node-baml && ../scripts/baml.sh check --agent-skill-check off)
(cd python-baml && ../scripts/baml.sh check --agent-skill-check off)
(cd baml-debian && ../scripts/baml.sh check --agent-skill-check off)
```

`check.py` validates source syntax and dashboard invariants without Docker, BAML build outputs, or installed app dependencies. CI runs these checks and resolves both example Compose configurations. `verify.py` exercises existing HTTP endpoints and observes load; it never starts containers and intentionally fails for stopped apps/load generators, mismatched rates, or restarts. A clean source checkout cannot reproduce the full runtime until the heap-metrics dependency is available and the local artifacts are built.

Migration validation on September 10, 2026 passed the offline checks, all three BAML checks using the preserved CLI, both example Compose configurations, both served dashboard comparisons (27 panels on port 3000 and 23 on port 3100), and all 20 comparison queries over the preceding hour through Grafana. All 26 container identities and mounts were unchanged. The 60-second default-stack verification **failed for native Debian**: 6,000 attempts, 5,985 HTTP 200s, 15 transport errors, and one new restart. The other four variants each delivered 6,000 HTTP 200s at approximately 100 RPS without new transport errors or restarts. The second-stack verification failed at its stopped load generator. No full-load pass or sustained stability is claimed. The earlier successful verification report is also retained at `results/migration/historical-verification.json`.
