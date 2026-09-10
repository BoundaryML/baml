# Size-gate cache audit (2026-09-10)

Audited canary `38275af01f5762f409d38d387f4e756d5ac016b3`, the September R2 migration, and completed canary and PR runs. R2 compiler caching is working. The confirmed defect is missed Cargo dependency caching: size-gate's read-only keys have no matching writer. There is no evidence in the sampled runs of stale compiled artifacts being reused.

## Evidence from CI

Every job below reports `s3, name: baml-build3, prefix: /baml/ci/` and zero cache read errors, write errors, and timeouts. Hits/misses are sccache's cacheable Rust compilations, not all compiler invocations. All ten jobs report `No cache found.` for Swatinem/rust-cache.

| Run | Job | R2 hits | R2 misses | Hit rate |
| --- | --- | ---: | ---: | ---: |
| [Canary push 34499800289](https://github.com/BoundaryML/baml/actions/runs/34499800289) | [Linux](https://github.com/BoundaryML/baml/actions/runs/34499800289/job/102947645574) | 708 | 1 | 99.86% |
| Same | [macOS](https://github.com/BoundaryML/baml/actions/runs/34499800289/job/102947645723) | 713 | 1 | 99.86% |
| Same | [Windows](https://github.com/BoundaryML/baml/actions/runs/34499800289/job/102947645605) | 640 | 77 | 89.26% |
| Same | [WASM](https://github.com/BoundaryML/baml/actions/runs/34499800289/job/102947645501) | 449 | 0 | 100% |
| Same | [Report](https://github.com/BoundaryML/baml/actions/runs/34499800289/job/102958675002) | 41 | 0 | 100% |
| [PR run 34512477133](https://github.com/BoundaryML/baml/actions/runs/34512477133) | [Linux](https://github.com/BoundaryML/baml/actions/runs/34512477133/job/102989973154) | 689 | 20 | 97.18% |
| Same | [macOS](https://github.com/BoundaryML/baml/actions/runs/34512477133/job/102989973209) | 689 | 25 | 96.50% |
| Same | [Windows](https://github.com/BoundaryML/baml/actions/runs/34512477133/job/102989973217) | 618 | 99 | 86.19% |
| Same | [WASM](https://github.com/BoundaryML/baml/actions/runs/34512477133/job/102989973274) | 433 | 16 | 96.44% |
| Same | [Report](https://github.com/BoundaryML/baml/actions/runs/34512477133/job/103001956988) | 41 | 0 | 100% |

The canary measurement jobs took about 16m Linux, 15m macOS, 30m Windows, and 81s WASM despite high hit rates. Their non-cacheable calls were 143, 144, 133, and 104 respectively, mostly `crate-type`. A high sccache hit rate does not eliminate executable/cdylib linking, build-script execution, packing, and measurement. Cargo's `Compiling` messages alone do not establish a miss; rustc still goes through the wrapper on a cache hit.

The native reports in this canary run contain three actual policy violations; the report job fails on those JSON results. The daily [baseline refresh run 34460902515](https://github.com/BoundaryML/baml/actions/runs/34460902515) successfully builds the helper and adopts reports, then fails while enqueueing its PR because `CI-v2 Failure Alert` is failing. Neither failure indicates an R2 cache failure. This cache fix does not change ceilings or enqueue/merge a baseline PR.

The first canary push after the repository-secret change, [run 33797930291](https://github.com/BoundaryML/baml/actions/runs/33797930291) on September 3, shows the same split: Linux 703/1 hits/misses, macOS 708/1, Windows 635/77, and WASM 443/0, all using `baml-build3` with zero read/write errors, while all four dependency caches miss. This is not a new R2 authentication failure appearing after the secrets move.

## Why the dependency cache misses

[Swatinem/rust-cache's configuration](https://github.com/Swatinem/rust-cache/blob/v2/src/config.ts) builds keys from the prefix/shared key, runner OS/architecture, installed Rust versions and compiler environment, then manifests/lockfiles/configuration. Its restore prefix retains the environment hash and only drops the final manifest hash. A different compiler environment therefore cannot fall back to the writer's entry.

Observed restore keys in canary run 34499800289:

| Family | Size-gate reader | Cargo-test writer |
| --- | --- | --- |
| Linux | `v0-rust-linux-cargo-Linux-x64-358631ac` | `v0-rust-linux-cargo-Linux-x64-da02379a` |
| WASM | `v0-rust-linux-wasm-Linux-x64-358631ac` | `v0-rust-linux-wasm-Linux-x64-da02379a` |
| Windows | `v0-rust-windows-cargo-Windows_NT-x64-5f662bdc` | `v0-rust-windows-cargo-Windows_NT-x64-e8921c96` |
| macOS | `v0-rust-macos-cargo-Darwin-arm64-70e14784` | Cargo-test job disabled |

Cargo-test writers set `CARGO_PROFILE_DEV_OPT_LEVEL=1` and `CARGO_PROFILE_TEST_OPT_LEVEL=1`. Windows additionally sets both debug profiles to `line-tables-only`. Size-gate does not. The Linux writer successfully restores an older manifest entry ending `da02379a-738ce5f2`, then uploads a new cache; the size reader with `358631ac` cannot see either. Both jobs report the same installed Rust versions, isolating the environment mismatch. The profile divergence predates the R2 move: [#4075](https://github.com/BoundaryML/baml/pull/4075) introduced the optimization settings in July. The macOS cargo-test job was disabled in [#4418](https://github.com/BoundaryML/baml/pull/4418) in August.

The fix lets the four hosted measurement jobs save their own exact keys only on `refs/heads/canary`. This preserves build profiles, existing key invalidation, and PR read-only dependency caching. It adds at most one writer per size-gate platform/environment, with normal manifest-key churn, rather than copying test profile settings into release measurement jobs. The Linux report job remains a reader. No compiled `target/` directory is added to rust-cache.

The baseline-refresh workflow previously requested the default cache paths including `target/`, unlike the writer's `cache-targets: false`; GitHub's cache version also depends on the requested paths. Its environment differed and it did not initialize sccache. It now uses the same download paths and environment as the hosted Linux size-gate writer, loads `.envrc`, and uses R2 for its helper compilation. It still downloads existing size reports rather than rebuilding shipping artifacts.

## Read/write paths and invalidation

| Layer | Contents/path | Writer and isolation |
| --- | --- | --- |
| R2 sccache | Bucket `baml-build3`, account endpoint `https://c0bf62c013f607c849c6a50cdd627b7b.r2.cloudflarestorage.com`, region `auto`, CI prefix `baml/ci/` | Credentialed builds read and write content-addressed compiler objects; CI jobs/branches share this namespace. Local developer builds use `baml/local/`. |
| rust-cache | Cargo home registry, git dependencies, and eligible installed binaries/metadata; excludes `baml_language/target` | Cargo-test jobs and now hosted size-gate measurements write only on canary, under their actual environment keys. Report/baseline/ix jobs only read. |
| mise | Installed tools selected by `install_args` | `mise-baml3` prefix plus platform/configuration/tool-set hashes; only canary saves. Size-gate uses pinned sccache 0.10.0. |
| Reports | `size-gate-<platform>.json` artifacts | Written by each measurement job and downloaded from that workflow run by the report job. They are not restored from rust-cache or R2. |
| Baselines | Committed `.ci/size-gate/*.toml` and `.cargo/size-gate.toml` | Compared against fresh measurements. The scheduled refresh deliberately selects a completed branch CI run with all four reports and records its provenance in a reviewed PR. |

`.envrc` selects R2 only when both `BAML_SCCACHE_R2_*` credentials exist. The POSIX `tools/baml-sccache` wrapper maps those to `AWS_*` immediately before invoking sccache. Windows bootstraps the native `tools_sccache` binary, then sets `RUSTC_WRAPPER` to its absolute path, avoiding cmd.exe's argument limit. A per-worktree Unix socket isolates POSIX servers; `SCCACHE_BASEDIRS` normalizes workspace paths. `sccache --version` runs before `.envrc` but does not start the server; compilation starts it through the credential-mapping wrapper.

The bucket/account migration in [#4685](https://github.com/BoundaryML/baml/pull/4685) changed `baml-build1` in the old account to `baml-build3` in the BoundaryML account, invalidating the old remote namespace. The HTTPS endpoint and `auto` region match [sccache's R2 requirements](https://github.com/mozilla/sccache/blob/v0.10.0/docs/S3.md). [#4738](https://github.com/BoundaryML/baml/pull/4738) moved credentials from the GitHub deployment environment to optional reusable-workflow repository secrets. `ci.yaml`, `ix-ci.yml`, and `canary-cache-refresh.yml` explicitly pass both names to their callees. Recent logs confirm the new bucket is actually used, not merely configured.

sccache keys incorporate compiler identity, compiler arguments (including profile, target and features), dependency/source inputs and tracked environment; they are not branch-name or Cargo.lock-only keys. Cargo runs the release build before size measurement and repacks the fixture. Source/profile changes should invalidate compilation while identical inputs can share objects across branches. Executables and cdylibs are not cached as linked outputs by this sccache version. See [Rust support and caveats](https://github.com/mozilla/sccache/blob/v0.10.0/docs/Rust.md) and [key construction](https://github.com/mozilla/sccache/blob/v0.10.0/src/compiler/rust.rs).

The scheduled cache purge deletes GitHub cache entries for the Linux/WASM/MSRV/Windows shared-key prefixes, then invokes cargo-tests. It does not delete R2 objects. It does not populate size-gate's different environment keys; after a purge, the next successful canary measurement populates those again. macOS is left warm because that refresh workflow has no macOS cargo-test repopulator. Toolchain/environment changes create new keys; manifest-only changes can restore an older download cache and fetch the missing dependencies safely.

## Branch behavior and limits

- Canary pushes: credentialed R2 reads/writes; cargo tests and hosted size measurements save dependency caches. Cache saves require the action's successful post-job path.
- Same-repository PRs, main pushes and merge groups: R2 reads/writes if secrets are supplied; size-gate dependency caches are read-only, with canary/default-branch caches available under GitHub's scope rules. A new environment key remains cold until canary writes it.
- Fork PRs: optional secrets are empty, so `.envrc` unsets R2 settings and uses runner-local sccache. Dependency caches remain readable when GitHub scope/version/key rules permit, but no dependency writes are enabled. The local compiler cache is not uploaded, so cross-run compiler misses are expected.
- ix preview: trusted jobs retain seeded ignored files through `clean: false`; fork PRs use hosted runners. ix remains read-only for rust-cache and primarily relies on its seed and R2. Installed toolchain differences can prevent a match with hosted canary keys. No new ix cache writer is introduced.

No bucket purge, cache deletion, baseline change, or merge was performed for this audit. R2 credentials are shared across trusted CI code, not a security boundary between same-repository branches. The evidence demonstrates reuse and zero reported storage errors; it is not a byte-for-byte cached-versus-uncached rebuild comparison, a full review of every procedural macro's undeclared inputs, or an inspection of R2 credentials/lifecycle policies. Upstream explicitly documents limitations for procedural macros that read untracked files. A fresh fork/ix run was not used to validate those paths. Existing canary/PR logs cannot prove that new canary-only cache writes have occurred before the fix lands; initial PR dependency misses remain expected.
