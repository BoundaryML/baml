# Hello world on ECS / EC2

Five variants × two native architectures, with one dedicated EC2 instance for each of the ten combinations. Each instance runs one measured ECS task with a **hard 1-vCPU quota and 1-GiB memory limit**, with host swap disabled. There is no Auto Scaling group, capacity-provider scaling, or service scaling policy. A separate, fixed eleventh instance hosts ten independent load tasks; load generation and metric export consume none of the measured tasks’ CPU or memory allocation.

| Variant | Runtime base | Architectures |
| --- | --- | --- |
| `python-only` | `python:3.10-slim` / Starlette | arm64, x64 |
| `python-baml` | Same Python base + BAML bridge | arm64, x64 |
| `node-only` | `node:20-alpine` / Express | arm64, x64 |
| `node-baml` | Same Node base + BAML bridge | arm64, x64 |
| `baml-only` | `amazonlinux:2023` / packed BAML server | arm64, x64 |

This uses ECS with the **EC2 launch type**. `matrix.json` describes the matrix and fixed instance types: `c7g.medium` (Graviton3 arm64) and `c7a.medium` (AMD x64), each with one vCPU and 2 GiB host RAM. Every measured container has a hard 1-vCPU / 1-GiB limit; the OS and ECS agent use the remaining host memory and share its CPU. CPU quota is not exclusive core pinning. Startup explicitly disables host swap and masks `swap.target`; the ECS-optimized AL2023 AMI has no swap configured. Container `maxSwap`/`swappiness` settings are omitted because AL2023 does not support swappiness. Both workload families are non-burstable, with one hardware core per vCPU, but processor generation and implementation remain part of the comparison. The separate load host is `c7i.xlarge` (4 vCPU / 8 GiB). One full run therefore needs 14 standard on-demand vCPUs plus any pre-existing account usage; two runs need 28. Additional runs may require an EC2 quota increase.

The handlers, package pins, and instrumentation originate from [the Fly harness](../hello-world-fly/README.md). `GET /` returns exactly `hello world`, no trailing newline, with `Content-Type: text/plain; charset=utf-8` and `Cache-Control: no-store`. BAML bridges await a deterministic BAML function through one process-wide runtime on every request. Native BAML runs the packed server directly. There are no LLM calls, caches, forced collections, or per-request application logs. Python tracing is enabled for both Python variants, just as in the Fly experiment.

BAML is pinned to `0.18.1-nightly.20260908.a`, with its corresponding Node and Python bridges. This is a published toolchain, separate from the custom heap-metrics build used by [the local harness](../hello-world-local/README.md). All base images are pinned to multi-architecture index digests in `images.lock.json`; each build selects the appropriate native CLI, pack host, bridge, and base-image child manifest. `x64` maps to Docker `amd64` and ECS `X86_64`. The load image is x64 for every target, holding generator architecture constant.

## CDK and local iteration

Infrastructure is authored in JavaScript with AWS CDK v2 in `infra/benchmark.js`; `infra/app.js` selects a run from CDK context. CDK synthesizes and deploys the CloudFormation templates, including IAM roles, without a handwritten template renderer. The foundation stack supplies one isolated VPC/public subnet and an immutable ECR repository. Each run stack adds its own fixed hosts, cluster, discovery namespace, services, logs, dashboard, and event capture. Dependencies and CLI are pinned in `package-lock.json`.

```sh
cd tools/bench/hello-world-ecs
export AWS_PROFILE=boundaryml-dev
export AWS_REGION=us-east-1
npm ci
npm test
python3 -m unittest discover -s tests -v

# Print eleven build commands without building or publishing.
python3 scripts/build.py --repository local/hello-world --tag trial-001

# Synthesize the foundation; deploy it once per account/region.
npx cdk synth hello-world-foundation
npx cdk deploy hello-world-foundation --require-approval never \
  --outputs-file artifacts/foundation-outputs.json
```

A CDK-bootstrapped account/region is required. For a new environment, run `npx cdk bootstrap aws://ACCOUNT/REGION` first. The selected `boundaryml-dev` account already has the CDK bootstrap stack. No AWS credentials belong in this directory. Local evidence, CDK synthesis output, and build output are ignored.

## Prepare images once, reuse them across runs

`build.py` executes only with `--execute`. Its default Docker context is `colima`, a separate build VM from the measured `colima-baml-hello-world` VM; override with `--docker-context` and optionally `--builder`. Both architectures require native builder nodes or working emulation. Each BAML bridge image checks its hello-world call during the build. Node RSS may be zero under cross-architecture build emulation; `verify.py` checks positive RSS on the native hosts.

```sh
REPOSITORY=$(python3 -c 'import json; print(json.load(open("artifacts/foundation-outputs.json"))["hello-world-foundation"]["RepositoryUri"])')
aws ecr get-login-password --region "$AWS_REGION" | \
  docker --context colima login --username AWS --password-stdin "${REPOSITORY%%/*}"

# Use a fresh tag for every source revision; repository tags are immutable.
python3 scripts/build.py --repository "$REPOSITORY" --tag trial-001 --execute --push
```

The resulting `artifacts/trial-001/images.json` records immutable ECR digests after each successful build. If an unchanged build is interrupted, repeat with `--resume` to reuse completed entries; use a fresh tag after editing sources. A local build omits `--push` and uses `--load`; its manifest is rejected for AWS deployment. The same complete pushed manifest can be reused across runs, holding application code constant while changing load. The repository is retained when the foundation stack is deleted.

## Run different load profiles simultaneously

Every run gets a distinct CDK stack, ECS cluster, ten workload instances, load host, security groups, Cloud Map namespace, services, log groups, event capture, and dashboard. Runs share the foundation network and immutable image repository. Metrics include `RunName`, `Variant`, and `Architecture`, so runs do not blend together. Profiles currently define a continuous constant request rate per target; `steady-10.json` and `steady-100.json` exercise all ten targets at 10 and 100 RPS respectively. Add another profile JSON with a `name` and integer `rate_per_target` to compare another rate. These are independent full runs, not stages of a ramp.

Use a fresh lowercase run name (3–32 characters) for each new experiment. All hosts use the foundation’s public subnet in `us-east-1a` (or the selected region’s `a` zone), avoiding cross-AZ differences, and receive public IPs for outbound image pulls and management. There is no NAT gateway or inbound public HTTP/SSH. Workload task ENIs have private IPs; only that run’s load-host security group can reach ports 8080/9091. Cloud Map resolves private task addresses within the VPC. Load tasks use bridge networking on their dedicated host, avoiding ten load-task ENIs or ENI trunking.

```sh
python3 scripts/run.py up --name hello-steady-10 \
  --images artifacts/trial-001/images.json --profile profiles/steady-10.json

# The first run remains active while this independent run starts.
python3 scripts/run.py up --name hello-steady-100 \
  --images artifacts/trial-001/images.json --profile profiles/steady-100.json

python3 scripts/run.py status --name hello-steady-10
python3 scripts/run.py status --name hello-steady-100

# After metric ingestion, inspect two complete minutes and retain JSON evidence.
python3 scripts/verify.py --name hello-steady-100
```

`run.py` passes the run configuration to the pinned CDK CLI and writes stack outputs under `artifacts/<run>/outputs.json`. Each run uses its own CDK synthesis directory, so independent CLI invocations do not share an output lock. It inherits `AWS_PROFILE` and `AWS_REGION`, with optional `--aws-profile` / `--region` overrides before the subcommand. For a reviewable synthesis or diff, use the same context directly:

```sh
npx cdk diff hello-steady-10 -c run=hello-steady-10 \
  -c images=artifacts/trial-001/images.json -c profile=profiles/steady-10.json
```

There is one ECS service with desired count 1 for every measured combination. A placement constraint binds it to its dedicated, labelled instance. Deployment maximum is 100%, so rollout cannot run an extra measured task alongside it. ECS replaces a failed task on the same instance; it does not provision another instance. If an instance fails, that cell stays unavailable. The task-state EventBridge log records exits, stopped reasons, and replacements; task restarts must not be mistaken for sustained stability. There is no scheduled restart or container restart override. Services begin independently as instances register; startup skew and initial DNS/connection failures are included in load evidence.

Rerunning `up` for an existing name updates that run rather than creating a second copy. Pass the same profile and images with `--load-count 0` to stop only its ten load tasks. Add `--app-count 0` to stop its application tasks too. The eleven EC2 instances still exist until the stack is deleted. Changing the image/profile for an existing run changes the measurement phase; use a fresh name when a clean comparison is intended.

## AWS-native measurements

No Grafana or Prometheus service is provisioned. Each independent Vegeta 12.13.0 attack runs continuously against its private target with HTTP/1.1 keepalive, a one-second DNS cache, 10-second request timeout, no redirects, and up to 2,000 workers. A Python collector consumes and discards encoded responses. It checks HTTP 200 bodies and emits bounded **CloudWatch Embedded Metric Format (EMF)** JSON to stdout via `awslogs`, in namespace `BAML/HelloWorld`.

| Metric | Meaning |
| --- | --- |
| `Requests`, `Http200` | Interval completion counts; sum divided by the period gives throughput |
| `LatencyMs` | Raw nonnegative latency samples, at most 100 per log event; CloudWatch calculates p90/p99 |
| `TransportErrors` | Responses with status 0 |
| `HttpErrors` | HTTP statuses other than 200 |
| `BodyMismatches` | HTTP 200 responses whose body differs from `hello world` |
| `ProcessRssBytes` | Serving-process RSS, available for Node and Python variants |
| `NodeHeapBytes`, `PythonTracedBytes` | V8 or traced Python allocations; neither is total native heap |
| `ProcessMetricsUp` | Whether the private process metrics scrape succeeded |

Counters flush every ten seconds; latency batches flush at 100 samples or the interval boundary. They use high-resolution EMF storage, while the comparison dashboard uses 60-second periods. Latencies include failed requests and are timestamped at collection, not request initiation. Initial failures and generator backpressure remain visible; configured rate does not prove delivered rate. The generator has a bounded task allocation too, so check actual completion rate and failures before inferring a server ceiling. CloudWatch metrics/log ingestion has latency; missing samples are not zero utilization.

ECS supplies standard service `CPUUtilization` and `MemoryUtilization` metrics; Container Insights enhanced is enabled for task/container resource and count detail. Dashboard memory is ECS container usage relative to its configured memory limit, **not process RSS**. The packed Amazon Linux server has no process-RSS exporter, so no cgroup value is relabelled as RSS. RSS, V8/Python heaps, and task memory have overlapping scopes and must not be added. CPU/memory metric resolution can miss short peaks; the retained task-state events are essential evidence for OOMs and replacements.

Each run gets a native CloudWatch dashboard. Generate another dashboard to overlay whole runs:

```sh
node scripts/compare.js hello-steady-10 hello-steady-100 > artifacts/compare.json
aws cloudwatch put-dashboard --region "$AWS_REGION" --dashboard-name hello-load-comparison \
  --dashboard-body file://artifacts/compare.json

aws logs tail /baml/hello-world/hello-steady-10/task-events --region "$AWS_REGION" --since 1h
aws logs tail /baml/hello-world/hello-steady-10/load --region "$AWS_REGION" --since 10m
```

The comparison overlays all ten cells for each selected run across successful throughput, p90 latency, CPU, memory, transport failures, and body mismatches. App, load, and task-event logs have seven-day retention and their groups are retained when a run stack is deleted. The ECS-created Container Insights performance group uses its AWS default retention. Download `status` snapshots before deleting a run, and use a new run name afterward: retained log-group names are intentionally not reused automatically.

```sh
# Deletes only this run's infrastructure, including its eleven EC2 instances.
npx cdk destroy hello-steady-10 -c run=hello-steady-10 \
  -c images=artifacts/trial-001/images.json -c profile=profiles/steady-10.json
```

## Deployed development run

Using `AWS_PROFILE=boundaryml-dev`, account `147997132427`, region `us-east-1`, run **`hello-ecs-100-20260910-02`** was deployed with CDK on September 10, 2026 (Pacific time). It uses `profiles/steady-100.json` and the eleven pushed images in the local, ignored `artifacts/ecs-20260910-03/images.json` manifest. [Open its CloudWatch dashboard](https://us-east-1.console.aws.amazon.com/cloudwatch/home?region=us-east-1#dashboards:name=hello-ecs-100-20260910-02).

The foundation deployed successfully and the active run reached `UPDATE_COMPLETE` after the collector fix. All ten load services run task-definition revision 2 with the corrected load digest. All eleven hosts registered and all twenty services reached desired/running count 1. Read-only SSM inspection confirmed five native ARM64 and five native x64 workload hosts, task-parent `cpu.max=100000 100000`, `memory.max=1073741824`, and no active host swap. All ten private HTTP endpoints returned the exact eleven-byte body and expected headers; all eight Node/Python RSS endpoints reported positive native RSS. Initial ten-second load intervals delivered about 100 successful requests/second per cell, with no HTTP errors, transport errors, or body mismatches. **The sustained check failed:** both `baml-only` architectures subsequently hit the 1-GiB limit and exited with code 137 (`OutOfMemoryError`). ECS replaces those tasks on the same fixed hosts, so a later running task is not evidence of uninterrupted stability. In the exported 04:34–04:36 UTC interval on September 11, all eight Node/Python cells delivered 100.03–100.05 HTTP 200 responses/second with zero recorded errors; CPU, memory, p90 latency, and RSS samples were present. The two native BAML cells delivered only 58.33 and 83.33 HTTP 200 responses/second across their outages. No application image, resource limit, or load profile was changed to suppress the native-server failures.

Snapshots, native-host inspection, private-network checks, metric exports, and the exact deployment configuration are retained under ignored `artifacts/hello-ecs-100-20260910-02/`. Full build and deployment logs are under `artifacts/`. The first attempt, `hello-ecs-100-20260910`, rolled back before workloads started because ECS rejected `maxSwap` without swappiness; its failure evidence and retained logs are separate from the active run. The corrected CDK configuration disables host swap on AL2023. Target outages also exposed a collector bug: Vegeta emits `body: null` on transport failures. The collector now treats that as an empty body and continues counting errors; the regression test and a refused-connection container test cover this case. Build `ecs-20260910-03` changes only the load image; all ten app digests are identical to build `ecs-20260910-02`. Post-rollout CloudWatch logs confirmed the corrected collector stayed running and exported transport-error counts during subsequent native-server outages. The load-task rollout is a measurement phase boundary. Error counters before that fix undercount outages because the collector crashed, and the retained task events/stopped-task snapshots are required to interpret that period.

```sh
export AWS_PROFILE=boundaryml-dev AWS_REGION=us-east-1
python3 scripts/run.py status --name hello-ecs-100-20260910-02
python3 scripts/verify.py --name hello-ecs-100-20260910-02

# Once the account quota allows another full run, reuse these exact images.
python3 scripts/run.py up --name hello-ecs-10-comparison \
  --images artifacts/ecs-20260910-03/images.json --profile profiles/steady-10.json
```

The account’s standard on-demand EC2 quota was 16 vCPUs at deployment, including 2 used by a pre-existing instance. This run uses the remaining 14. A request to raise the quota to 64 is open (`943bb8d7aca14ad29de5d431ecd7c274sNiCnU6t`); an additional complete run needs that increase or equivalent available capacity. Run naming, network isolation, logs, metrics, and comparison dashboards already support concurrent runs. Existing local and Fly workloads were not restarted or modified for this deployment.

## Validation and references

Local tests cover ten unique placement targets, architecture/task limits, absence of scaling resources, independent run identities and load rates, immutable-image validation, EMF batching/counting/error handling, and cross-run dashboard definitions. CDK assertions validate the synthesized resources and two simultaneous run stacks. Python tests validate the collector. Deployment observations are recorded separately below; a successful deployment and short smoke check do not establish longevity. Existing local/Fly workloads are left alone.

- [ECS CPU quotas and task memory constraints](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task-cpu-memory-error.html)
- [ECS swap support and AL2023 limitations](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/container-swap.html)
- [ECS task parameters for EC2](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task_definition_parameters_ec2.html)
- [ECS-optimized AMI parameters](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/retrieve-ecs-optimized_AMI.html)
- [ECS private service discovery](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/service-discovery.html)
- [CloudWatch EMF specification](https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/CloudWatch_Embedded_Metric_Format_Specification.html)
- [ECS service utilization metrics](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/service_utilization.html)
