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

Replace only the registry prefix, keeping the upstream owner, repository, tag, and digest:

```text
ghcr.io/<owner>/<image>:<tag>@sha256:<digest>
  -> us-central1-docker.pkg.dev/baml-infra/ghcr-cache/<owner>/<image>:<tag>@sha256:<digest>
```

The references in the repository pin the verified upstream index digest as well as retaining the descriptive tag. The digest is authoritative; moving the upstream tag does not update our builds. Index pins preserve platform selection instead of forcing the ARM jobs onto an amd64 child manifest.

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

This changes image delivery and freezes image versions, not container contents, entrypoints, mounts, runners, compilers, or artifact policies. Python keeps `musllinux_1_1`, its existing GNU manylinux policies, and the Python 3.10 abi3 floor. The rust-cross index exposes both Linux amd64 and arm64 host variants; cross-rs 0.2.5 uses amd64 hosts even when cross-compiling to aarch64. Existing native ARM versus cross-compilation choices are unchanged.

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

## Verification and upgrades

An anonymous manifest request tests public access without consulting Docker's credential helpers or downloading image layers:

```sh
curl --fail --silent --show-error --dump-header - --output /dev/null \
  --header 'Accept: application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json' \
  'https://us-central1-docker.pkg.dev/v2/baml-infra/ghcr-cache/rust-cross/manylinux_2_28-cross/manifests/sha256:ca53fa07ecf1c3e6408c51fbca64c036d9d29af832d3f8bb954910e89097f275'
```

Expect HTTP 200 and the matching `Docker-Content-Digest`. Provisioning verified all eight index digests anonymously, relevant amd64/arm64 child manifests and config blobs, plus two tiny layer downloads. That is not a full image pull or a release build: uncached layers are fetched lazily. Full outage protection requires every needed platform's layers to have been pulled successfully before the outage.

For an intentional image upgrade, inspect the upstream tag's manifest index, compare the cache response and digest, check required host platforms, and update every occurrence of the affected pin in one reviewed PR. Do not silently substitute a single-platform digest or advance the cross version. Update matching platform tests. No automatic tag-based image updates are configured; the release maintainers must schedule upstream security review and digest refreshes.

Representative pre-release checks, on a suitable Linux Docker host:

1. Pull each pinned image for every host platform used by CI, without credentials. ARM64 toolchain and Node jobs need the arm64 rust-cross variant; the wrapper and Python cross-builds need amd64. Cross-rs targets need amd64.
2. Run the GNU ARM64 container with Bash and a bind mount to verify executable permissions, workspace writes, networking, and required tools, for example `docker run --rm --platform linux/arm64 --entrypoint bash -v "$PWD:/workspace" -w /workspace "$IMAGE" -lc 'id; test -w /workspace; command -v bash curl tar; ldd --version'`, with `IMAGE` set to the committed reference.
3. Exercise both Python musl matrix legs using the existing maturin workflow commands; inspect output wheel names for `cp310-abi3` and the expected `musllinux_1_1_<arch>` tag, then install/import on representative musl systems.
4. Exercise all four Linux CFFI builds and both Java musl builds; verify expected ELF architecture, dynamic-library output, and consumer loading. Preserve `RUSTFLAGS`, source-remapping variables, and musl `-crt-static`.
5. Exercise GNU toolchain/wrapper builds and ARM64 Node packaging; run existing artifact/consumer smoke tests. Confirm logs name `us-central1-docker.pkg.dev` for the eight scoped images.

Use normal reviewed CI/release procedures for build validation. Provisioning and the migration PR do not dispatch releases or publish test artifacts to production registries.

## Operations, security, and rollback

Anonymous read access is intentionally public to the internet, not restricted to BoundaryML workflows. It does not grant public writes or administration, but this remote repository can cache other public GHCR paths: the configuration is not an owner/image allowlist. Third parties can consume storage, egress, and request quota by pulling through it. Do not add private upstream credentials to this public repository.

Monitor [Artifact Registry quotas](https://docs.cloud.google.com/artifact-registry/quotas), repository size, and [storage/network charges](https://cloud.google.com/artifact-registry/pricing), especially internet egress to GitHub runners. Budget alerts and abuse-related quota review are operational follow-ups, not configured by this setup; a budget alert is not a spending cap. If public-cache abuse becomes material, reassess the public access design rather than putting a secret in every job without accounting for pre-step container pulls.

No cleanup policy is configured, deliberately preserving old release pins for rollback. If adding cleanup later, retain every digest still referenced by supported release commits; deleting cached content reintroduces an upstream dependency. Cached content can survive an upstream outage, but the single GCP region and project remain availability dependencies.

Digest pins protect content identity, not upstream trust or vulnerability status. This setup does not add signature/attestation verification, rebuild the images, or enable vulnerability scanning. Upstream image licensing and security maintenance obligations remain unchanged. Review upstream provenance and vulnerabilities when updating pins, and retain evidence with the PR.

If GCP pulls fail, first check anonymous manifest access, repository IAM, quotas/billing, and upstream availability. Roll back image routing by replacing `us-central1-docker.pkg.dev/baml-infra/ghcr-cache/` with `ghcr.io/` in the platform contract, Node workflow, and Cross.toml, updating matching tests while retaining the digest pins and explicit Python container wiring. This returns to upstream delivery without changing image contents. Do not delete the cache or change public IAM as part of a workflow rollback; either affects other users independently.
