# Hello-world benchmarks

| Harness | Purpose |
| --- | --- |
| [hello-world-local](hello-world-local/README.md) | Five local arm64 Docker variants, local BAML builds, Prometheus/Grafana |
| [hello-world-fly](hello-world-fly/README.md) | Six active amd64 Fly variants with retained deployment/run evidence |
| [hello-world-ecs](hello-world-ecs/README.md) | Five variants × arm64/x64 on dedicated ECS/EC2 instances; independent simultaneous runs and CloudWatch metrics |

Each harness owns its configuration, generated outputs, and evidence. The local and Fly directories retain their existing ignored state after relocation. The original `baml-demos` source paths remain intact for existing mounts and historical scripts. AWS run creation is explicit; the ECS harness does not alter either existing experiment.
