# C# implementation-entry external-run handoff

Status: repository-local preparation is complete; the external workflow has
not run. The exact local provenance commit and its matching one-shot gate tag
must be reviewed and pushed before the first run. B8 passes locally, but its
committed-source exact-package/trim reproduction is still pending. B11 also
passes its complete local Linux trim/single-file matrix, while B4 remains
blocked on the real eight-RID inputs and runners. `TASK/implementation.md`
must not be created yet.

## Exact workflow

`.github/workflows/csharp-entry-gates.yml` is a non-publishing caller. It keeps
`workflow_dispatch` for normal use once the file reaches the default branch
and adds a narrowly filtered `csharp-entry-gates-*` tag-push trigger for the
pre-merge first run, because GitHub's
[manual-run documentation](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow)
requires the workflow file to exist on the default branch. The plan job
requires the bootstrap tag to be exactly
`csharp-entry-gates-<full source SHA>`. It freezes that source SHA and one
Canary release-plan JSON, builds the existing language-neutral `bridge_cffi`
matrix from those exact inputs, including separate unstripped diagnostic
artifacts, and calls
`.github/workflows/verify-csharp-entry-gates.reusable.yaml`.

The verifier:

1. derives all eight target/RID/canonical-asset/native-runner combinations
   from their single authority in `release/platforms.json`;
2. requires exactly one shipping artifact plus its producer SHA-256 sidecar
   and current source/run/attempt/release identity, plus one debug-enabled,
   unstripped diagnostic bundle for every target,
   including targets marked experimental upstream; it constrains the primary
   to the bundle root and proves ELF debug sections/symbol tables, Mach-O
   DWARF/non-stripped state, or a valid Windows PDB plus PE debug directory,
   with an exact target architecture check in every format. Windows reads only
   the PDB signature and streams procedure-symbol inspection.
   Each diagnostic primary library has a different digest and the whole bundle
   is larger than the shipping library; all eight shipping digests and all
   eight diagnostic-primary digests must also be distinct;
3. compiles the managed package fixture once, packs twice, normalizes both
   unsigned packages, requires byte identity and Unix `0644` ZIP permissions,
   generates its `buildTransitive` supported-RID target and all eight native
   package paths from the same central platform matrix, requires the complete
   exact 15-entry inventory, enforces the `200,000,000`-byte ceiling, and
   records all native/package sizes and digests;
4. source-maps the evidence package to its private feed, byte-compares each
   restored cache copy to the normalized input, and publishes it from a cold
   cache on native macOS, glibc Linux, real musl Linux, and Windows x64/ARM64
   runners, preserving the exact ABI/lifetime execution output and source,
   release, native, restore, and publish identities in each consumer artifact;
5. proves exactly one RID-selected native asset, a representative actual BAML
   call, exact product/ABI version, exact native digest, architecture,
   dependency/minimum-platform data, and the reviewed 26-symbol export
   allowlist;
6. records restore/publish time, expanded cache, publish footprint, shipping
   size, diagnostic-bundle size/file count, and unstripped primary digest per
   RID; normalizes every diagnostic bundle twice into byte-identical
   metadata-stable `tar.gz` archives, retains one archive per RID in a separate
   immutable evidence artifact, and records compressed size/digest;
7. runs protocol generation twice on Linux x64, macOS ARM64, and Windows x64
   builders, uploads the generated sources and manifests, executes the same
   five Rust-produced optional-host-call payloads plus six malformed cases,
   preserves each behavior-probe output, and requires all three hosts'
   manifests to be byte-identical in a separate fan-in;
8. executes the actual pull-based stream fixture with a bounded replay-server
   child and a separate consumer child whose replay endpoint is set before
   native runtime startup;
9. publishes and executes the bridge-bearing trimmed ABI/callback and
   Protobuf/stream/media/handle probes from the exact evidence package in
   package-default mode, alongside dynamic/generic/value, reflection-root,
   RID-policy, and generated-program fixtures;
10. executes all four single-file shapes from the exact evidence package:
    untrimmed and trimmed, each with native sidecar and native self-extraction;
    each sidecar and each extracted native must be byte-identical to the exact
    package asset; and
11. proves the exact package's bounded explicit unsupported-RID diagnostic
   (`BAML0010`) and NativeAOT rejection (`BAML0019`).

It uploads the immutable evidence package, the eight retained native
diagnostic archives and their identity/measurement manifest, one consumer
artifact per RID, three protocol-host artifacts and their consistency
artifact, and the semantic/deployment artifact. A completeness job requires
all 15 named inputs from the current run attempt and hashes every contained
file before uploading its own manifest. It does not authenticate to NuGet,
publish, sign, create a release, mutate registry state, or invoke a production
publisher.

## Authorization and execution

For the pre-merge first run, an authorized maintainer pushes the exact target
branch plus the lightweight tag
`csharp-entry-gates-<full source SHA>`. GitHub's manual-dispatch API returns
404 until the workflow exists on the default branch, so the tag event is the
only supported bootstrap path and the plan job rejects any mismatched tag.
Creating the local provenance commit and tag is safe repository work; no agent
should push either ref without the corresponding separate user authorization.
Once that push is authorized, this explicitly non-publishing evidence run is
an in-scope verification step; it does not authorize any production release
or publication.

## Provenance preflight

The final precommit temporary-index preview contains exactly the intended 120
source/evidence files. `git diff --cached --check` passes, Git reports zero
binary entries, and the scope contains zero paths under local `.codex/`,
`.TASK.readonly-seed/`, `AGENTS.md`, or any excluded directory. The real index
was not touched by the preview. The local provenance commit must match this
scope exactly; its committed SHA becomes the workflow's `source_sha`. Later
edits invalidate that identity rather than inheriting it.

After the run, record in `verification-gates.md`:

- workflow run URL and exact source SHA;
- every shipping/unstripped native digest and measurement;
- normalized package digest/size and package-build time;
- every native consumer result and measurement;
- protocol-builder hashes;
- stream and trimmed deployment output;
- `BAML0010`/`BAML0019` output; and
- any failed target, assertion, or unavailable runner without downgrading the
  package contract.

Only a complete passing run can promote B3/B4/B8/B11/B12/C6 as their
acceptance criteria permit. After all implementation-document entry criteria
pass, create `TASK/implementation.md` and begin product implementation.

## Local workflow-mechanics evidence

The workflow YAML and its modified native-builder workflow pass repository
`actionlint`, independent YAML parsing, and `git diff --check`. Current local
projects build with .NET SDK `10.0.110` and warnings as errors. The external
workflow pins SDK `10.0.301`; its real-musl consumer pins the official .NET 10
Alpine SDK image digest and exact `binutils`/`file` package versions.
`docker manifest inspect` confirms that digest is a manifest list containing
both `amd64` and `arm64` images required by the two musl consumers.
Every external action reached by the caller, native builder, verifier, and
shared Rust setup is pinned to an exact upstream commit SHA; version comments
retain the intended update channel without leaving a moving runtime input.
The shared setup also pins Rust `1.93.0`, protoc `23.4`, cross `0.2.5`, and
wasm-pack `0.14.0`; Cargo-installed tools use `--locked`. The Windows ARM64
rustup bootstrap uses the archived `1.28.2` initializer, installs no moving
default toolchain, and verifies the downloaded executable against SHA-256
`de9f7d29ccd39efa59a3dda3ec363b396e09b92681229b9b8f6aaa4c84285e9c`
before execution.
The diagnostic native build passes its unstripped profile override and target
directory as Cargo CLI arguments, so Linux `cross` cannot discard them through
its environment allowlist.

The exact command was also executed locally for
`x86_64-unknown-linux-gnu`. It produced a `329,440,320`-byte diagnostic
primary, SHA-256
`9e2b7469050ee349f12b2f24b11b5d4113a9a9d6ff3c4aa94c681b68d28b563f`;
`file` reports `with debug_info, not stripped`, and `readelf` finds
`.debug_info`, `.debug_line`, and `.symtab`. Rebuilding the shipping profile
from the same sources produced a stripped `24,661,376`-byte library, SHA-256
`dc8d399dbfaa14327be3eed25a52fa8661333ce211ca2d5a38fe0a833a323432`.
The diagnostic primary is 13.36 times larger and has a distinct digest.

The exact B8 stream command now also passes locally. The fixture launches the
replay server/runtime and consumer in separate bounded child processes, setting
`BAML_REPLAY_BASE_URL` in the consumer's process environment before the native
runtime starts. It requires a strict-prefix order chain and final continuity,
one completion per pull, zero unsolicited idle completions, exact
cold/final/final-only/cancellation/release behavior, and a maximum of seven
pending callback states. A corrected replay audit showed that the runtime's
incidental initial-prefix boundary can yield 19 or 20 partials, so the
normative assertion permits that chunk-boundary variation while requiring
every later partial to be a strict extension. Normalized ordered
deltas must reconstruct the exact 789-byte canonical final payload with
SHA-256
`2e950ddbdb0c2e12f64c09bc6e4a72f687367894cdea17d632529fd6719d2ef2`
in drained and final-only paths. The same assertions pass after a trimmed
exact-package publish; the external workflow reruns them from the committed
source.

All native-bearing evidence projects require an explicit
`BamlNativeProbeMode=Direct` or `Package`. Package mode additionally requires
the same existing isolated feed through both `BamlNativeProbeFeed` and the
`BAML_NATIVE_PROBE_FEED` variable consumed by NuGet source mapping; direct mode
rejects either feed input. The evidence package ID and version are frozen to
`Baml.Bridge.MultiRidPackageProbe` `0.0.0-b4`; either override fails before
restore. Validation runs before package-reference collection as well as
compilation. A NuGet-generated package-path property then proves
that the restored assets graph contains the exact evidence version in package
mode and contains none in direct mode, so either direction of a stale
`--no-restore` mode switch fails. The external verifier sets package mode
globally and byte-compares the restored package to its normalized input.
The final local trimmed run used a fresh cache for the BAML package; only
Microsoft runtime/host/ASP.NET/ILLink packs were preseeded from the audit
host's existing offline cache because its portable packs were not otherwise
available. The BAML package still restored solely from the mapped evidence
feed and byte-compared exactly. The external workflow performs a normal cold
restore instead.

The runner labels were rechecked on 2026-07-17 against GitHub's
[hosted-runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
The ARM64 macOS build/consumer and protocol builder use `macos-15`, avoiding
the announced macOS 14 image retirement; the x64 build/consumer uses
`macos-15-intel`. `ubuntu-24.04-arm` and `windows-11-arm` are current
public-repository labels; GitHub
[announced ARM64 hosted-runner general availability](https://github.blog/changelog/2025-08-07-arm64-hosted-runners-for-public-repositories-are-now-generally-available/)
for public repositories in August 2025. The separate
[Visual Studio 2026 Windows ARM64 image](https://github.blog/changelog/2026-06-11-new-runner-images-in-public-preview/)
remains a 2026 public preview with a later label migration planned; this
workflow does not opt into that preview.
Runner unavailability must be recorded as a failed external gate; it does not
authorize dropping `win-arm64` or substituting another architecture.

Because this host cannot produce the real matrix, a deliberately non-evidentiary
mechanical staging copied the one current Linux x64 native binary into all
eight RID paths. That package is **not** B4 proof. It validates only the
assembly machinery:

- the managed fixture compiled once;
- two raw packs normalized byte-identically to a `60,964,748`-byte package,
  SHA-256
  `61eda292f7a5dab4565f38cb679feb5046c18b18449e517c9d7c34b074b7ab72`;
- its mechanics-only RID slots all contain the exact current local native,
  SHA-256
  `cdb5bcbe5b23ab973953a4ec000e0d37413741c594d2b3c0365a0278e9be06ad`;
- all 15 ZIP entries have stable order/time and Unix mode `0644`;
- ordinary package-default resolution executed current canonical bytecode,
  two representative calls, cancellation, callback containment, buffer
  ownership, media handle clone/release, and product version `0.15.0`;
- the packaged target rejected `linux-s390x` with `BAML0010` whose exact
  message lists all eight supported RIDs, and rejected `PublishAot=true` with
  `BAML0019`;
- the warning-free runtime RID policy maps the eight exact platform
  combinations, throws `PlatformNotSupportedException` for unsupported
  combinations without architecture/libc substitution, and detects this host
  as `linux-x64`; the same probe also executed inside the pinned Alpine image
  as exact `linux-musl-x64`; and
- fresh exact-package builds of the ABI, stream/media, generated-program, and
  NativeAOT guard fixtures restored through the source-mapped configuration
  with zero warnings/errors; and
- exact-package untrimmed `linux-x64` single-file sidecar and self-extract
  forms both execute successfully; their exact allowed inventories are
  executable plus optional PDB, with one native library only in sidecar mode.
  Each sidecar and each isolated extracted native byte-compares to the exact
  current package asset; and
- the warning-free exact-package B11 retry executes trimmed ABI,
  Protobuf/media/pull-stream, managed contract, reflection rooted/unrooted,
  RID-policy, and both trimmed single-file native forms. The committed-source
  external workflow must still reproduce that local result.

The protocol probe also consumed five real Rust-encoder protobuf payloads
(SHA-256
`8950193938db37064a5488edf237a04ba817470b1651e8185d247b3aaadcedf5`)
and executed all six malformed-message branches. It reported
`optional_callback_slots=5/5`,
`malformed_optional_callback_slots=6/6`, and
`rust_produced_optional_callback_slots=5/5`. The corrected shared callback
integer decoder also reported `callback_baml_int_valid=4/4`,
`callback_baml_int_rejected=12/12`, and
`callback_baml_int_fail_closed=ok`: exact minimum/maximum values pass through
both required and present-optional positions, while the four out-of-domain
`long` carriers and missing/wrong-oneof values at both positions are rejected
before the counting callback can run.

The local native duplication is intentionally excluded from package baseline,
compression, cross-RID, architecture, and runner claims.

Local warning-free commands for the two new semantic fixtures were:

```text
env NUGET_PACKAGES=/tmp/baml-csharp-a3-trim-nuget dotnet build \
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.ReflectionRootProbe/Baml.Bridge.ReflectionRootProbe.csproj \
  --configuration Release --nologo --no-restore -p:NuGetAudit=false
dotnet run --project \
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.ReflectionRootProbe/Baml.Bridge.ReflectionRootProbe.csproj \
  --configuration Release --no-build --no-restore -- rooted

env NUGET_PACKAGES=/tmp/baml-csharp-a3-trim-nuget dotnet build \
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.RidDiagnosticProbe/Baml.Bridge.RidDiagnosticProbe.csproj \
  --configuration Release --nologo --no-restore -p:NuGetAudit=false
dotnet run --project \
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.RidDiagnosticProbe/Baml.Bridge.RidDiagnosticProbe.csproj \
  --configuration Release --no-build --no-restore
```

They reported:

```text
reflection_root=public_constructor_and_properties
rid_policy=8_exact_no_substitution
unsupported_runtime=PlatformNotSupportedException
detected_rid=linux-x64
```

The pinned Alpine SDK image separately reported:

```text
rid_policy=8_exact_no_substitution
unsupported_runtime=PlatformNotSupportedException
detected_rid=linux-musl-x64
```

Current source SHA-256 values:

| Source | SHA-256 |
| --- | --- |
| multi-RID package project | `0fc734b64dd71e5469b52d422a1be9aa3933f33efbbe251cecd18ee199845a21` |
| multi-RID package target template | `d41f56d8f7326b061a1e423a970e82894565b9f4081aa77bc64fd866d4e571f7` |
| multi-RID package marker | `db0267db2c7ca8800e43e23f0adf74733ba8c414c948589d4b1d3922cdc8ed81` |
| multi-RID package README | `28987ce1e2fda81f86d8e6674a7e41f6b97d40a1a99a8a637daef6ad6b333f6a` |
| explicit native-probe mode target | `01445658f4cce7e9531f6c1154e8ef1924974660b1f8f86da0b035da680c0776` |
| reflection project | `6d51462394edc7991aa39b38f7bd05f44a85a6dba12141e49955f2d94c6757e5` |
| reflection source | `e90abe9435409c7b066aa801339aed7e714d3a77b2b547a953d92c8bbeb07374` |
| reflection README | `a76acf945a4f9ada25d7e1a105c33009a740850839c8655af2bb18a06a55926b` |
| RID-policy project | `6d51462394edc7991aa39b38f7bd05f44a85a6dba12141e49955f2d94c6757e5` |
| RID-policy source | `cbb97235e49e5f71222eb76e5354076ca6796691cc5842e579c0b667d4b9178a` |
| RID-policy README | `cc123950bb44e8b560f0acd42fc5a8d88b0e37a9a866befa47d29f6187fe0276` |
| reviewed native export allowlist | `efd373b47be61e17534cb222048ebbdbd3e8308eebe25cd424b601b740101a60` |
| platform contract | `22e8c19cdc567e1a48689a70f4eca3367271e84c78f268822c3b8613194f7a74` |
| platform-contract Rust validator | `bbd4047c47378f56ba7d88b84bb1ccd07c96aadb5c606341d90262bec6547518` |
| exact NuGet source mapping | `9f9269380ec09c18cbba4cd560ed2dd8aacbcc1326b7a4c72891782401565619` |
| Rust optional-host-call producer | `dbb4ac63ec46b144858098b6f56fbc6392a3adff44543c6217c428fe9a48656f` |
| outbound protocol contract | `ee2a6765c918bfd57a6b910831edb1d90fc55e3b7b4d0c10b453618b03eb1ac8` |
| C# protocol behavior source | `c901f15ffbff1b1106f959dc288b02e860ea31eac82f0cef0b60a31f80137524` |
| call-ID source | `861a36ada5c864f4365020c007a7d2bd269f19ae67663c57c568e44c80f88d2e` |
| shared Rust setup action | `2063428518629315d1d6bb67e4c864764466cfc6a9654469dec014d908c2713a` |
| manual caller workflow | `cd3e45ef30ecd8789116e16a3c34212b35a905244020b417a57e70a35fc28f96` |
| reusable verifier workflow | `9b4118773f3c2c397ab1b50270c97cd12a920631ee3f1df7ef803230a780619e` |
| native builder workflow | `38f117d90015add19bdd0edc153b66d7339ba1b16cb621356ae3d4aac3782f41` |
