# Packed BAML wrapper vs Rust wrapper — 2026-10-04

Both wrappers select/install the Rust CLI and edit configuration. The packed wrapper executes that policy as an ordinary BAML program and gets automatic runtime recording; it calls no telemetry API. The CLI itself stays in Rust.

```sh
baml --version
baml check --project ./project
baml run Main --project ./project
```

Rust wrapper source: `bb5e886de6d0317bbae4b4ab9710f0b84e9fb3f8` (the stack's canary base). Packed wrapper/compiler/host source: `c43da5cc7c0e6fe5d95b51806e4203995d1000de` (fast clock startup plus conservative duration capture after a clock fault). Both use Rust 1.98.0 and the normal release profile on an Apple M2 Max, 64 GiB, macOS 15.6.1. Earlier measurements in PR #5123 predate this clock change and remain historical.

## Quick commands

Milliseconds, **median / p95**; 100 measured fresh processes per command/variant after four warmups, interleaved with seed 20261004. Telemetry-off and local rows describe the packed wrapper; the fixture CLI is native C and records nothing.

| Operation | Rust | Packed, off | Packed, local |
| --- | --- | --- | --- |
| toolchain --help | 5.16 / 7.25 | 22.74 / 24.72 | 24.56 / 26.47 |
| --version, installed version | 5.30 / 6.34 | 23.24 / 25.10 | 25.61 / 27.49 |
| toolchain list, 10 installations | 5.37 / 6.60 | 23.76 / 27.23 | 25.57 / 27.36 |
| Launch installed native child | 6.84 / 8.53 | 22.73 / 24.28 | 24.69 / 26.83 |
| Launch local native child | 6.76 / 8.12 | 22.29 / 24.71 | 24.17 / 27.07 |
| --version, query local child | 17.77 / 19.89 | 25.14 / 27.81 | 27.28 / 28.98 |
| toolchain pin, local path | 5.62 / 7.60 | 33.49 / 36.01 | 35.87 / 39.13 |
| toolchain use, local path | 5.64 / 6.49 | 33.79 / 36.23 | 36.14 / 39.05 |
| toolchain install, already installed | 5.41 / 9.59 | 23.44 / 28.26 | 25.34 / 28.98 |
| Child streams 32 MiB | 11.19 / 19.55 | 26.77 / 31.63 | 29.03 / 35.40 |

The cached install row parses a cached manifest and validates an existing installation; it does not download or extract. Pin/use edit small TOML fixtures. Stream and passthrough outputs are checked for their expected byte counts. Help/version formatting differs, so these compare the same user operation rather than identical text.

## CPU, first output, and wrapper-only memory

Median CPU milliseconds (user + system):

| Operation | Rust | Packed, off | Packed, local |
| --- | --- | --- | --- |
| toolchain --help | 3.59 | 21.01 | 23.32 |
| --version, installed version | 3.69 | 21.78 | 24.64 |
| toolchain list, 10 installations | 3.78 | 22.00 | 24.37 |
| Launch installed native child | 4.90 | 20.33 | 22.96 |
| Launch local native child | 4.84 | 19.82 | 22.02 |
| --version, query local child | 5.11 | 23.32 | 25.74 |
| toolchain pin, local path | 4.02 | 22.12 | 24.82 |
| toolchain use, local path | 4.03 | 22.29 | 24.89 |
| toolchain install, already installed | 3.78 | 22.00 | 24.36 |
| Child streams 32 MiB | 7.23 | 22.19 | 24.83 |

Installed `--version`, median milliseconds:

| Metric | Rust | Packed, off | Packed, local |
| --- | --- | --- | --- |
| First output | 5.01 | 19.01 | 20.86 |
| Exit after first output | 0.28 | 4.20 | 4.66 |
| Total | 5.30 | 23.24 | 25.61 |

Wrapper-only peak **RSS / physical footprint**, MiB, medians:

| Operation | Rust | Packed, off | Packed, local |
| --- | --- | --- | --- |
| toolchain --help | 3.61 / 2.39 | 31.12 / 22.33 | 32.64 / 23.24 |
| --version, installed version | 4.14 / 2.46 | 32.05 / 22.40 | 34.04 / 23.83 |
| toolchain list, 10 installations | 4.27 / 2.52 | 32.11 / 22.42 | 33.91 / 23.70 |
| toolchain pin, local path | 4.13 / 2.49 | 31.95 / 22.05 | 34.13 / 23.71 |
| toolchain install, already installed | 4.02 / 2.45 | 32.33 / 22.49 | 33.77 / 23.46 |

Peak RSS is from wait4; footprint is near-exit proc_pid_rusage. Passthrough root accounting resets when Rust execs into the child, so those peaks are not a comparable retained-wrapper memory measurement. The process-tree samples below measure that cost directly.

## Retained memory while a child runs

Four launches per row; the child prints ready, optionally touches 64 MiB, then sleeps for two seconds. Sampling is 150 ms after readiness. All values include the child.

| Variant | Processes | Threads | FDs | RSS MiB | Footprint MiB |
| --- | --- | --- | --- | --- | --- |
| Direct child, child allocates 0 MiB | 1 | 1 | 3 | 1.17 | 0.97 |
| Rust, child allocates 0 MiB | 1 | 1 | 3 | 1.19 | 1.00 |
| Packed, off, child allocates 0 MiB | 2 | 15 | 12 | 32.45 | 23.13 |
| Packed, local, child allocates 0 MiB | 2 | 17 | 12 | 35.84 | 26.06 |
| Direct child, child allocates 64 MiB | 1 | 1 | 3 | 65.22 | 65.06 |
| Rust, child allocates 64 MiB | 1 | 1 | 3 | 65.21 | 65.04 |
| Packed, off, child allocates 64 MiB | 2 | 15 | 12 | 95.96 | 86.58 |
| Packed, local, child allocates 64 MiB | 2 | 17 | 12 | 99.09 | 89.31 |

Rust uses exec on this common path, so its process is replaced by the child. The packed wrapper waits for the child and retains its runtime. RSS sums can count shared pages twice; footprint is closer to memory charged by macOS. Four samples describe this fixture and do not establish precise memory equivalence across systems.

A real user BAML `Hold` function prints ready and sleeps for two seconds. The same release CLI runs it in every row. Local telemetry applies to the user CLI in both Rust/local and packed/local; the packed wrapper also records itself.

| Variant | Processes | Threads | RSS MiB | Footprint MiB |
| --- | --- | --- | --- | --- |
| Direct CLI, off | 1 | 27 | 37.53 | 26.07 |
| Rust, off | 1 | 27 | 36.66 | 25.22 |
| Rust, local | 1 | 29 | 39.14 | 27.24 |
| Packed, off | 2 | 41 | 67.75 | 47.10 |
| Packed, local | 2 | 45 | 72.91 | 51.39 |

## Real CLI operations

Milliseconds, median / p95; 40 measured runs per row/variant after three warmups, interleaved. All wrappers call the exact same release CLI. The project has one `Main` function that prints hello.

Telemetry off in wrapper and CLI:

| Operation | Direct CLI | Rust + CLI | Packed + CLI |
| --- | --- | --- | --- |
| --version, query real CLI | 7.54 / 8.42 | 19.22 / 20.10 | 30.08 / 32.76 |
| check --project | 49.88 / 58.30 | 54.32 / 65.23 | 69.94 / 80.01 |
| run Main --project | 29.46 / 32.05 | 32.67 / 34.92 | 48.38 / 53.42 |

Local telemetry in wrapper and CLI:

| Operation | Direct CLI | Rust + CLI | Packed + CLI |
| --- | --- | --- | --- |
| --version, query real CLI | 7.45 / 8.39 | 19.42 / 20.38 | 31.94 / 37.47 |
| check --project | 49.35 / 56.78 | 53.24 / 59.84 | 71.38 / 82.20 |
| run Main --project | 31.50 / 37.39 | 35.08 / 39.31 | 52.29 / 59.05 |

The Rust wrapper has no recording of its own. `--version` wrappers print their own version and query the local CLI; the direct CLI only reports itself. Checking does not execute the user program; running Main does.

## LSP

Four initialized sessions per variant, identical tiny workspace and release CLI. Median initialize response time includes Python Popen/client framing. Memory is sampled 500 ms after initialize/initialized, then shutdown/exit completes. Off/local controls use matching telemetry in the user CLI.

| Variant | Initialize ms | Processes | Threads | RSS MiB | Footprint MiB |
| --- | --- | --- | --- | --- | --- |
| Direct CLI, off | 13.19 | 1 | 41 | 253.43 | 236.97 |
| Direct CLI, local | 13.75 | 1 | 43 | 251.76 | 234.98 |
| Rust, off | 16.12 | 1 | 41 | 250.62 | 234.18 |
| Rust, local | 16.49 | 1 | 43 | 255.81 | 239.04 |
| Packed, off | 31.55 | 2 | 55 | 283.12 | 257.50 |
| Packed, local | 33.87 | 2 | 59 | 286.01 | 259.60 |

These are initialized idle sessions, not large-project editing/indexing benchmarks.

## Generic runtime floor and recording levels

60 measured processes after three warmups, interleaved:

| Application | Wall p50 / p95 ms | CPU p50 ms | Peak RSS MiB | Peak footprint MiB |
| --- | --- | --- | --- | --- |
| Native child | 1.64 / 2.25 | 1.25 | 1.17 | 0.99 |
| Empty packed Main, off | 22.23 / 25.08 | 20.32 | 29.34 | 21.24 |
| Empty packed Main, local | 24.52 / 28.40 | 22.88 | 31.73 | 23.00 |

40 measured wrapper --version processes per recording level:

| Level | Wall p50 / p95 ms | CPU p50 ms |
| --- | --- | --- |
| off | 24.12 / 27.42 | 22.61 |
| low | 26.53 / 29.70 | 25.08 |
| medium | 26.47 / 30.27 | 25.18 |
| high | 26.29 / 30.23 | 25.08 |

The empty packed program accounts for much of the short-command launch cost. Removing clock calibration makes recording levels close in this fixture; it does not remove generic program loading/engine initialization or delivery shutdown.

## Export and storage

Loopback cloud server implements prepare POST plus presigned PUT, with a synthetic ingestion key. These measure protocol/shutdown behavior, not production BCS/S3, TLS, or Internet latency. All application exit statuses are zero. Failed deliveries emit a warning.

| Condition | Runs | First output ms | Total exit ms | Exit tail ms | Requests | PUT bytes |
| --- | --- | --- | --- | --- | --- | --- |
| fast | 20 | 21.03 | 26.85 | 5.92 | 2 | 9568 |
| 100ms_per_request | 5 | 21.89 | 235.06 | 213.24 | 2 | 9568 |
| 503_retries | 3 | 23.78 | 637.25 | 613.47 | 4 | 0 |
| timeouts | 1 | 21.16 | 40647.24 | 40626.09 | 4 | 0 |

The timeout row is one deliberate fault injection, not a percentile estimate. The fast clock fixes startup; cloud delivery still joins its uploader and can hold exit through the retry window. Nonblocking export remains separate runtime work.

Local telemetry in quick commands; 104 recordings per row including warmups:

| Operation | Average recording bytes | New shared CAS bytes | Kernel writes p50 bytes |
| --- | --- | --- | --- |
| toolchain --help | 4098 | 105 | 8192 |
| --version, installed version | 9444 | 0 | 12288 |
| toolchain list, 10 installations | 9056 | 0 | 12288 |
| Launch installed native child | 11677 | 0 | 12288 |
| Launch local native child | 5140 | 0 | 8192 |
| --version, query local child | 6023 | 0 | 8192 |
| toolchain pin, local path | 10542 | 0 | 16384 |
| toolchain use, local path | 8881 | 0 | 16384 |
| toolchain install, already installed | 13213 | 0 | 16384 |
| Child streams 32 MiB | 5142 | 0 | 8192 |

Logical file lengths and kernel I/O accounting are distinct. Kernel write counts do not establish fsync durability or physical device traffic. The 32 MiB inherited stdout stream is not copied into the telemetry recording under default policy.

## Binary size and root-process counters

| Artifact | Executable MiB | gzip -9 MiB |
| --- | --- | --- |
| Rust wrapper | 4.48 | 2.20 |
| Packed BAML wrapper | 31.70 | 13.54 |
| Generic pack host | 28.68 | 12.85 |
| Empty packed Main | 31.51 | 13.50 |

The packed wrapper is 7.07× the Rust executable size and 6.15× its gzip size. It adds 0.19 MiB over an empty packed application. gzip covers binary bytes, not tar/release archive overhead. Artifact SHA-256 fingerprints are retained in the results JSON.

Installed --version, median root-process counters:

| Metric | Rust | Packed, off | Packed, local |
| --- | --- | --- | --- |
| Retired instructions | 28,125,510 | 176,374,172 | 197,787,324 |
| CPU cycles | 12,211,356 | 71,684,438 | 80,856,577 |
| Minor faults | 429 | 2,267 | 2,396 |
| Major faults | 0 | 0 | 0 |
| Involuntary switches | 16 | 168 | 205 |

Instructions/cycles and I/O come from near-exit root-process accounting, not summed process-tree counters. CPU frequency and scheduling vary on this shared machine.

## Reproduction and scope

The retained datasets contain **4,289 measured process launches**, plus 183 warmups and **76 live process-tree sessions**. Smoke checks and the earlier superseded LSP dataset are excluded. The benchmark uses isolated HOME/BAML_HOME, 10 fixture toolchains, synthetic cached manifests and credentials, BAML_TOOLCHAIN for local selection, disabled update checks, and matching user-CLI telemetry. Build/test processes finished before measured launches. Executable/file caches were warmed; these are fresh-process measurements, not cold-boot/cache-purged tests.

The native C harness starts the timer before posix_spawn, drains stdout and uses wait4 for wall/CPU/peak RSS. proc_pid_rusage supplies root counters when available. Resident sampling uses libproc over the full process tree. CPU is user + system time, including reaped child usage as accounted by the OS. Summed RSS may count shared pages twice. Median components need not add to median totals.

```sh
# Build Rust baml at the canary revision above in an isolated checkout/target:
cargo build --release -p baml --locked
# In the fast-clock checkout, with a different Cargo target directory:
cargo build --release -p baml_cli -p baml_pack_host --bins --locked
BAML_TELEMETRY=off BAML_CLI_ALLOW_DIRECT=1 ./target/release/baml-cli pack Main \
  --project crates/baml/baml_src --host ./target/release/baml-pack-host \
  --output ./target/baml-packed
# Also pack an identical project containing function Main() -> void {}.
python3 scripts/btel-startup/wrapper_compare.py all \
  --output OUTPUT --rust RUST_BAML --packed BAML_PACKED \
  --cli BAML_CLI --host BAML_PACK_HOST --tiny EMPTY_PACKED
```

The committed [results JSON](../../scripts/btel-startup/wrapper-results-20261004.json) retains all summary quantiles/ranges, source revisions, machine/method metadata, memory/thread/FD aggregates, recording sizes, cloud results and binary fingerprints. The [benchmark script](../../scripts/btel-startup/wrapper_compare.py) and C helpers reproduce them. Full individual samples, binaries and build/measurement logs remain under `target/wrapper-comparison-20261004/` in this checkout; external log copies are in `/tmp/wrapper-comparison-*.log`.

Performance is measured on one native ARM64 macOS developer machine with normal background applications. Cross-target checks establish compilation, not equivalent physical-platform latency or migration behavior. No clean-build comparison, download/extraction/self-update timing, cache purge, power measurement, long soak/leak test, or physical Linux/Windows/iOS/Wasm benchmark is claimed.
