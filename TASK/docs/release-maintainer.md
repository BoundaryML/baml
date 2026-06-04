# BAML Language Release Maintainer Notes

## Model

`baml_language/release.toml` is the human-authored source of truth:

```toml
[release]
canary_version = "0.11.0"
```

Canary publishes exactly that version. Nightly derives the next patch and a UTC date suffix, for example `0.11.1-nightly.20260602.a`.

## Common Commands

```sh
scripts/baml-language-version bump --patch
scripts/baml-language-version sync
scripts/baml-language-version check
scripts/baml-language-version compute --channel nightly
scripts/baml-language-version compute --channel nightly --pypi
scripts/baml-language-version plan --channel nightly --out release-plan.json
baml toolchain status
```

PyPI nightly versions use PEP 440 dev releases, for example `0.11.1-nightly.20260602.a` becomes `0.11.1.dev2026060200`.

## Workflow

`.github/workflows/release-baml-language.yml` is the release graph entrypoint. It uses a non-cancelling concurrency lock:

```yaml
group: baml-language-release-canary
cancel-in-progress: false
```

Build jobs download the single `release-plan.json` artifact and run `scripts/baml-language-version stamp --plan release-plan.json` before compiling.

Dry-run dispatch:

```sh
gh workflow run release-baml-language.yml -f channel=nightly -f dry_run=true
```

Dry runs generate a `dry-run-release-files` workflow artifact containing `manifest/v1/**`, `homebrew/Formula/baml.rb`, and AUR `PKGBUILD` / `.SRCINFO` files. When the run is on `refs/heads/canary`, the workflow also uploads public manifests under `https://pkg.boundaryml.com/dryrun/<run-id>/manifest/v1`.

Dry-run wrapper validation:

```sh
BAML_HOME="$(mktemp -d)" \
BAML_MANIFEST_BASE_URL="https://pkg.boundaryml.com/dryrun/<run-id>/manifest/v1" \
baml toolchain use nightly
```

## Publishing

Production publishes are guarded to `refs/heads/canary`. Mutable pointers (`canary.json`, `nightly.json`, `wrapper.json`, install scripts) are repairable on rerun. Immutable per-version manifests and GitHub release assets are never overwritten.

Automatic canary branch releases publish nightly by default. If the source commit advances `baml_language/release.toml` and `baml-language-<canary_version>` does not already exist, the workflow publishes canary first and queues a serialized nightly run after the canary manifest is live.

The Python wheel build is reusable, but the PyPI upload is a top-level `publish-pypi` job in `.github/workflows/release-baml-language.yml`. Registry publish jobs stay top-level whenever OIDC/trusted publishing is bound to the workflow identity that performs the upload. Before production PyPI publishing, update or validate the PyPI trusted-publisher binding for project `baml-core` so it authorizes `release-baml-language.yml`; leave the PyPI environment blank unless the job declares a matching GitHub Actions `environment`.

Homebrew and AUR publish only wrapper packages when `wrapper_changed == true`. Toolchain releases never dispatch package-manager updates. `scripts/baml-package-manager-artifacts` generates the formula and AUR files from wrapper archive checksums; publish jobs refuse to run unless the Homebrew token or AUR SSH key is configured.

## Rollback

Rollback is a code revert plus mutable pointer repair. Leave immutable artifacts in place and move channel pointers to a fixed version.

## Legacy Alpha Consumers

`tools/baml-bench` still resolves `baml-language-*-alpha.*` GitHub pre-releases and downloads a matching release binary. That is separate service logic, not part of the new release graph. Before deleting the old alpha release assets entirely, either migrate that service to `manifest/v1/{canary,nightly}.json` or explicitly retire the benchmark builder path.
