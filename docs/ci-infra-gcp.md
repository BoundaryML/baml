# GCP container cache for release CI

The BAML Language release pulls its `rust-cross` and `cross-rs` build images through a public Google Artifact Registry remote repository. GitHub-hosted runners need no Google credentials, OIDC step, registry password, or Docker login, including for job-level `container:` images that are pulled before steps run. Administrative changes still require an authorized Google identity.

This is a pull-through cache, not a replacement image build pipeline: Google fetches public images from GHCR on demand and serves cached content. Cache misses still depend on GHCR. See Google's [remote repository behavior](https://docs.cloud.google.com/artifact-registry/docs/repositories/remote-overview) and [public repository access](https://docs.cloud.google.com/artifact-registry/docs/access-control#configuring_public_access_to_a_repository).

## Provisioned resource

| Setting | Value |
| --- | --- |
| Project | `baml-infra` |
| Project number | `964166623930` |
| Location | `us-central1` |
| Repository | `ghcr-cache` |
| Format / mode | `DOCKER` / `REMOTE_REPOSITORY` |
| Upstream | `https://ghcr.io` |
| Pull prefix | `us-central1-docker.pkg.dev/baml-infra/ghcr-cache` |
| Repository IAM | `allUsers` has `roles/artifactregistry.reader` |
| Upstream credentials | None; only public GHCR images are intended |
| Encryption | Google-managed key |
| Cleanup policies | None configured |
| Vulnerability scanning | Disabled; `containerscanning.googleapis.com` is not enabled |

The repository was created manually on September 4, 2026. There is no Terraform, CDK, or other IaC state. BoundaryML's `baml-infra` project administrators own IAM, billing, quotas, and cache availability; image contents remain maintained by the upstream projects. [Open the repository in Cloud Console](https://console.cloud.google.com/artifacts/docker/baml-infra/us-central1/ghcr-cache?project=baml-infra).

## Image routing and scope

Replace only the registry prefix, keeping the upstream owner, repository, and tag:

```text
ghcr.io/<owner>/<image>:<tag>
  -> us-central1-docker.pkg.dev/baml-infra/ghcr-cache/<owner>/<image>:<tag>
```

Image references use upstream tags without digest pins so builds can receive updates to those tags through the cache. Tag refreshes follow Artifact Registry's caching behavior; they are not guaranteed to reflect an upstream change immediately.

| Upstream repository:tag | Consumers | Image selection |
| --- | --- | --- |
| `rust-cross/manylinux_2_28-cross:aarch64` | GNU ARM64 toolchain, wrapper, Node SDK | [Platform contract](../release/platforms.json), [Node workflow](../.github/workflows/build2-nodejs-sdk.reusable.yaml) |
| `rust-cross/manylinux2014-cross:x86_64` | GNU x86_64 toolchain and wrapper | [Platform contract](../release/platforms.json) |
| `rust-cross/rust-musl-cross:aarch64-musl` | ARM64 Python musl wheel | Python `container` in the platform contract, forwarded by the [Python workflow](../.github/workflows/build2-python-sdk.reusable.yaml) |
| `rust-cross/rust-musl-cross:x86_64-musl` | x86_64 Python musl wheel | Python `container` in the platform contract, forwarded by the Python workflow |
| `cross-rs/aarch64-unknown-linux-gnu:0.2.5` | ARM64 CFFI | [Cross.toml](../baml_language/Cross.toml) |
| `cross-rs/x86_64-unknown-linux-gnu:0.2.5` | x86_64 CFFI | Cross.toml |
| `cross-rs/aarch64-unknown-linux-musl:0.2.5` | ARM64 CFFI and Java musl | Cross.toml |
| `cross-rs/x86_64-unknown-linux-musl:0.2.5` | x86_64 CFFI and Java musl | Cross.toml |

The authoritative entry point is [release-baml-language.yml](../.github/workflows/release-baml-language.yml). Its [toolchain](../.github/workflows/build2-toolchain.reusable.yaml) and [wrapper](../.github/workflows/build2-wrapper.reusable.yaml) workflows consume platform-contract images; their temporary `CROSS_CONFIG` takes precedence over workspace defaults. The [CFFI](../.github/workflows/build2-bridge-cffi.reusable.yaml) and [Java](../.github/workflows/build2-java-sdk.reusable.yaml) workflows run from `baml_language` and use its `Cross.toml`, preserving environment passthrough. Local `cross` builds from that workspace use the same cache.

The explicit Python override is necessary because [maturin-action](https://github.com/PyO3/maturin-action#inputs) otherwise constructs its own GHCR musl reference. [Cross target image overrides](https://github.com/cross-rs/cross/wiki/Configuration#targettargetimage) similarly bypass cross 0.2.5's implicit GHCR defaults. Merely replacing literal `ghcr.io` strings in workflow YAML misses these paths.

This changes image delivery while preserving the existing image tags, entrypoints, mounts, runners, compilers, and artifact policies. Python keeps `musllinux_1_1`, its existing GNU manylinux policies, and the Python 3.10 abi3 floor. The rust-cross index exposes both Linux amd64 and arm64 host variants; cross-rs 0.2.5 uses amd64 hosts even when cross-compiling to aarch64. Existing native ARM versus cross-compilation choices are unchanged.

Alpine, Microsoft, Quay, and Docker Hub images are unchanged. The PyPI publishing action's own `ghcr.io/pypa/gh-action-pypi-publish` image is outside this rust-cross/cross-rs migration. Legacy `engine` release workflows and integration-test Dockerfiles are also outside the BAML Language release graph and are not migrated here. This does not eliminate every repository dependency on GHCR or GitHub.

## Administrative setup and inspection

These are the commands used to provision the existing resource, not commands CI should run. Do not recreate or delete a working repository just to refresh this setup. See Google's [creation reference](https://docs.cloud.google.com/sdk/gcloud/reference/artifacts/repositories/create).

```sh
gcloud auth login
gcloud config set project baml-infra
gcloud auth list --filter=status:ACTIVE --format='value(account)'
gcloud config get-value project
gcloud billing projects describe baml-infra

gcloud services enable artifactregistry.googleapis.com --project=baml-infra
gcloud artifacts repositories create ghcr-cache \
  --project=baml-infra --location=us-central1 \
  --repository-format=docker --mode=remote-repository \
  --remote-docker-repo=https://ghcr.io \
  --description='Public GHCR pull-through cache for BAML release build images'
gcloud artifacts repositories add-iam-policy-binding ghcr-cache \
  --project=baml-infra --location=us-central1 \
  --member=allUsers --role=roles/artifactregistry.reader
```

The project needs active billing, permission to enable the API and create repositories, and permission to set repository IAM. Organization policies must permit this location and the public IAM member. No GitHub or upstream secret is configured.

Inspect without changing anything:

```sh
gcloud artifacts repositories describe ghcr-cache \
  --project=baml-infra --location=us-central1 --format=json
gcloud artifacts repositories get-iam-policy ghcr-cache \
  --project=baml-infra --location=us-central1 --format=json
gcloud artifacts docker images list \
  us-central1-docker.pkg.dev/baml-infra/ghcr-cache --include-tags
```
