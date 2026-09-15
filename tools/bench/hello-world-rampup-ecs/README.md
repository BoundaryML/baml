# Hello-world ramp-up benchmark on ECS / EC2

This benchmark finds the overload point and highest sustained request rate of five hello-world implementations on native ARM64 and x64 hosts. Every measured cell gets a dedicated non-burstable EC2 instance and one ECS task with a hard 1-vCPU quota and 1-GiB memory limit. Host swap is disabled, services do not autoscale, and failed tasks are replaced only on their original fixed host.

| Variant | Runtime base | Architectures |
| --- | --- | --- |
| `python-only` | `python:3.10-slim` / FastAPI / Uvicorn | arm64, x64 |
| `python-baml` | Same Python stack + BAML bridge | arm64, x64 |
| `node-only` | `node:20-alpine` / Express | arm64, x64 |
| `node-baml` | Same Node stack + BAML bridge | arm64, x64 |
| `baml-only` | `amazonlinux:2023` / packed BAML server | arm64, x64 |

`GET /` returns exactly `hello world` with no trailing newline. BAML bridges await one deterministic BAML function per request through a process-wide runtime. BAML is pinned to `0.18.1-nightly.20260908.a`; base images and JavaScript dependencies are pinned as well.

## Measurement topology

The ten workload hosts are `c7g.medium` for ARM64 and `c7a.medium` for x64. The load generators share one `c7i.12xlarge`, and each generator has a hard 4-vCPU / 1-GiB task allocation. The load host has 48 vCPUs, so ten generators consume at most 40 vCPUs and leave eight for host overhead. A full run consumes 58 standard on-demand vCPUs plus any unrelated account usage.

All hosts are in one availability zone. Workload endpoints are private and reachable only from the load-host security group. Each run has isolated services, discovery, logs, task-state events, dimensions, and a CloudWatch dashboard. The VPC and immutable ECR repository are shared through the `hello-world-foundation` stack.

## Load semantics

Each generator schedules requests for exactly four seconds, then schedules none for one second. A fresh bounded Vegeta attack starts for each active window. If requests have not drained by the next cycle boundary, the attack is killed and the cycle records `forced_stop=true`; missing completions remain visible because every cycle records both `scheduled` and `Http200`.

The default ramp starts at 100 active-window RPS per target and adds 100 RPS every 30 seconds, or six duty cycles, until 25,000 RPS. At a configured active-window rate `R`, the offered wall-clock average is `0.8R` because of the 4/1 duty cycle.

Every completed cycle emits a JSON record containing the configured rate, scheduled requests, completed requests, HTTP/transport errors, completion ratio, and forced-stop state. Vegeta aggregates its native binary result stream and emits exact counters plus p50/p90/p99/max latency once per cycle in CloudWatch Embedded Metric Format under `BAML/HelloWorldRampup`; Python never parses individual load responses. A separate request during the off window checks the exact `hello world` body. Node and Python variants expose process RSS plus V8/Python allocation metrics; ECS supplies task CPU and container memory utilization.

The included analyzer calls a rate strictly passing when it has at least five observed cycles, at least 99% aggregate successful completion, at least 95% completion in every cycle, no HTTP/transport/body errors, no task stop, and no forced generator cutoff. It marks a rate collapsed when it has less than 99% aggregate completion, less than 95% completion in any cycle, a body mismatch, a task stop, or a forced cutoff, and reports the first of two consecutive collapsed ramp steps as the coarse fall-over threshold. A one-rate sustain run can fail on its single observed rate. Isolated transport errors and isolated collapsed steps remain visible but do not falsely define the knee. A ramp result is only a coarse bound; confirm the final candidate with a separate sustain profile long enough to expose memory growth, task replacement, and thermal/runtime behavior.

## Local validation

```sh
cd tools/bench/hello-world-rampup-ecs
npm ci
npm test
python3 -m unittest discover -s tests -v
npx cdk synth hello-world-foundation
```

`scripts/build.py` prints commands unless `--execute` is supplied. Both architectures require native builder nodes or working emulation.

```sh
export AWS_PROFILE=boundaryml-dev AWS_REGION=us-east-1
REPOSITORY=$(aws cloudformation describe-stacks --stack-name hello-world-foundation --query 'Stacks[0].Outputs[?OutputKey==`RepositoryUri`].OutputValue' --output text)
aws ecr get-login-password | docker --context colima login --username AWS --password-stdin "${REPOSITORY%%/*}"
python3 scripts/build.py --repository "$REPOSITORY" --tag rampup-001 --execute --push
```

Build output and immutable image manifests are stored under ignored `artifacts/<tag>/`.

## Deploy and analyze a ramp

Use a fresh lowercase run name for every clean experiment. The default profile is [`profiles/ramp-100.json`](profiles/ramp-100.json).

```sh
python3 scripts/run.py up --name hello-ramp-001 --images artifacts/rampup-001/images.json --profile profiles/ramp-100.json
python3 scripts/run.py status --name hello-ramp-001
python3 scripts/analyze.py --name hello-ramp-001 --aws-profile "$AWS_PROFILE" --since-hours 1
```

The ramp profile schema is:

```json
{
  "name": "ramp-100",
  "mode": "ramp",
  "start_rate_per_target": 100,
  "increase_rate_per_target": 100,
  "increase_every_seconds": 30,
  "maximum_rate_per_target": 25000,
  "on_seconds": 4,
  "off_seconds": 1
}
```

Ramp rate fields may also be maps keyed by implementation or full cell name. [`profiles/ramp-node-continuation.json`](profiles/ramp-node-continuation.json) is an explicitly labeled continuation profile used after a complete 100-RPS-starting ramp: it resumes plain Node at 4,000 RPS while holding already-bounded implementations at 100 RPS. Continuation evidence must be interpreted together with the initial ramp and must not be presented as a fresh 100-RPS-starting run.

## Binary-search one in-VPC target

After establishing a passing lower bound and failing upper bound, the binary profile tests the upper bound first, verifies the lower bound, and searches to the configured resolution. Every candidate receives `seconds_per_candidate` of 4-on/1-off cycles. Request deadlines must fit within the one-second off-window, and the explicit connection cap prevents the load host's ephemeral ports from defining the result.

```json
{
  "name": "binary-node-only-arm64-5000-10000",
  "mode": "binary",
  "lower_rate_per_target": 5000,
  "upper_rate_per_target": 10000,
  "resolution_rps": 100,
  "seconds_per_candidate": 30,
  "connections": 1000,
  "request_timeout_ms": 800,
  "on_seconds": 4,
  "off_seconds": 1
}
```

Deploy only the desired application cell and its AWS load task. The generator emits `binary_cycle_finished`, `binary_candidate_finished`, and `binary_search_finished` records to the retained load log group.

```sh
python3 scripts/run.py --aws-profile "$AWS_PROFILE" up --name hello-vpc-node-binary-01 --images artifacts/rampup-001/images.json --profile profiles/binary-node-only-arm64-5000-10000.json --target-cell node-only-arm64
```

## Confirm sustained rates

A sustain profile accepts one rate for all cells or overrides keyed by implementation or full `implementation-architecture` cell name. Full-cell keys take precedence over implementation keys. This permits all ten coarse bounds to be tested together without changing workload images or resource allocations.

```json
{
  "name": "sustain-candidates",
  "mode": "sustain",
  "rate_per_target": {
    "python-only-arm64": 1200,
    "python-only-x64": 1300,
    "python-baml": 900,
    "node-only": 3100,
    "node-baml": 2400,
    "baml-only": 100
  },
  "on_seconds": 4,
  "off_seconds": 1
}
```

Deploy each confirmation under a fresh run name, retain its analysis/status snapshot, and inspect the task-event log for replacements or OOM exits. `scripts/verify.py` snapshots complete CloudWatch minutes for throughput, errors, latency, CPU, memory, and process RSS.

```sh
python3 scripts/run.py up --name hello-sustain-001 --images artifacts/rampup-001/images.json --profile profiles/sustain-candidates.json
python3 scripts/analyze.py --name hello-sustain-001 --aws-profile "$AWS_PROFILE" --since-hours 1
python3 scripts/verify.py --name hello-sustain-001 --minutes 15
```

## Drive one public target from the local computer

For a local Vegeta experiment, deploy exactly one workload cell with no AWS load task. `--local-load-cidr` must be the caller's current public IPv4 `/32`; the target uses bridge networking with fixed host ports 8080 and 9091, and the security group admits only that address. This mode deliberately changes the network path from the comparable in-VPC benchmark, so report it separately.

```sh
python3 scripts/run.py --aws-profile "$AWS_PROFILE" up --name hello-local-node-10k-01 --images artifacts/rampup-001/images.json --profile profiles/sustain-node-only-arm64-10000.json --load-count 0 --target-cell node-only-arm64 --local-load-cidr "$(curl -fsS https://checkip.amazonaws.com)/32"
```

The stack outputs `TargetUrl`. Local Vegeta must bound request timeouts inside the one-second off-window; otherwise an overloaded attack is still draining when the next cycle begins. Preserve the binary result stream and use `vegeta report -type=json` so overload errors remain countable. At high concurrency, explicitly cap `-max-connections` below the local operating system's ephemeral-port capacity.

Run a 30-second-per-candidate binary search with the local client after establishing one passing lower bound and one failing upper bound. `--verify-lower` prevents a search across an invalid bracket when the public network path behaves differently from the in-VPC path.

```sh
python3 scripts/local_binary_search.py --url http://TARGET_PUBLIC_IP:8080/ --lower 2000 --upper 5000 --resolution 100 --seconds 30 --connections 1000 --timeout-ms 800 --verify-lower --output artifacts/hello-local-node-10k-01/local-binary-search.jsonl
```

## Cleanup

Download status, analysis, verification, and task-event evidence before deletion. Log groups are retained for seven days and their names cannot be reused immediately.

```sh
npx cdk destroy hello-ramp-001 -c run=hello-ramp-001 -c images=artifacts/rampup-001/images.json -c profile=profiles/ramp-100.json
```
