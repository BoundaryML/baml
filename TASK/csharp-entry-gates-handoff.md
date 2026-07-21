# C# implementation-entry external-run handoff

Status: the fourth exact-source atomic attempt at commit
`991f491fba8cb10543b1cbb2aba33b5d9b3079bc` proved both verifier repairs,
passed all eight producers, assembled the exact package, passed all six Unix
consumers, all three protocol builders and their fan-in, and the complete
semantic/deployment lane. Both Windows consumers also passed restore,
publish, exact checksum, ABI/lifetime execution, and RID policy before their
PE inspection exposed the same nine unintended AWS-LC jitter-entropy exports.
The focused Windows native-build repair is local. No gate is promoted until
another exact-source attempt passes completely; B4 and C6 remain blocked and
`TASK/implementation.md` must not be created yet.

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

## Remote builder-only diagnostic

Before the exact tag bootstrap was pushed, the registered language-neutral
builder was dispatched directly as a non-publishing diagnostic.
[GitHub Actions run 29620985984](https://github.com/BoundaryML/baml/actions/runs/29620985984)
completed successfully on attempt 1 at exact remote source
`6d52aff1446c66be440771a14b85512c67214ca1`, release version `0.15.0`.
The matrix job and every one of the eight target jobs concluded `success`;
none of the three upstream-experimental targets was hidden by
`continue-on-error`. Every target built, checksummed, and uploaded its
shipping artifact. Native-host C/C++ ABI smoke tests also passed on Linux x64
glibc, macOS ARM64, and both Windows targets where the workflow enables them.

The downloaded artifacts' own identity sidecars all record the exact source
SHA, run ID `29620985984`, attempt `1`, target, canonical asset, and release
version. Their shipping primaries are:

| Target | Bytes | SHA-256 |
| --- | ---: | --- |
| `aarch64-apple-darwin` | 21,111,072 | `76c157a8c8b68d2607ba1ac00f0abb780a1080d88555d575b69bf2cb748f0ddc` |
| `x86_64-apple-darwin` | 21,539,636 | `df4c64c8ae040e99d3f4a0b67ee52355107e5d1ef28aedc4770a907ff1d57991` |
| `aarch64-unknown-linux-gnu` | 21,446,720 | `9410ac423d2f7a2d86282d7a8435e0b160c531f40e21ad9de23e8ce3f185cfdc` |
| `aarch64-unknown-linux-musl` | 21,376,824 | `66b0ff4c0af3d393e295e8e5394fc5e39e59abf5c3a1518d09ef42384c0a00c4` |
| `x86_64-unknown-linux-gnu` | 24,318,040 | `e545e6dca35bdb6c119961d088a65ce1f5ed12c9ab91db1177ca9e0a328e2e4f` |
| `x86_64-unknown-linux-musl` | 24,170,528 | `f6fbe864eb4b994c7b3424b8a8e65e85208199882dd8bb61834776e147604fff` |
| `x86_64-pc-windows-msvc` | 24,422,400 | `90b394181f0721bbe40c0a5db98af117b49e88c8537011f10d0b783234280c0b` |
| `aarch64-pc-windows-msvc` | 22,409,728 | `f895081ac3d990ad823f28c6dee4de85fb2e8f5482d50acbc9842cda0e1de1fd` |

This diagnostic deliberately left the C# entry-diagnostic condition false.
It therefore produced no unstripped bundles, PDB/DWARF proof, normalized
NuGet package, exact-package native consumers, protocol-host fan-in,
trim/single-file executions, unsupported-RID/NativeAOT diagnostics, or atomic
completeness manifest. It is useful cross-platform builder evidence only and
does not promote B3, B4, B8, B11, B12, or C6.

## First atomic attempt

[GitHub Actions run 29626183183](https://github.com/BoundaryML/baml/actions/runs/29626183183)
was triggered by the exact tag
`csharp-entry-gates-9d29c01928df7ce726c49286a3067129fc039115`.
Its event was `push`, its head SHA was
`9d29c01928df7ce726c49286a3067129fc039115`, and attempt 1 froze the
expected `0.15.0` release plan. The plan job and central target-matrix job
passed. All six non-Apple producer jobs passed their shipping build,
shipping-artifact upload, unstripped diagnostic build, platform-specific
debug/symbol verification, and diagnostic upload. That includes all three
upstream-experimental targets; no `continue-on-error` failure was hidden.

Both `aarch64-apple-darwin` and `x86_64-apple-darwin` passed their shipping
build/upload and unstripped diagnostic build, then failed
`Verify Unix diagnostic contains debug information and symbols`. Their
diagnostic upload steps did not run, and `Verify C# entry gates` was skipped
because its complete producer dependency was not satisfied. No package,
consumer, protocol-host, trim/single-file, unsupported-RID/NativeAOT, or
completeness evidence was produced.

The diagnostic build requests `debug=2` and `strip=false` but does not pin
`split-debuginfo`. Cargo's
[profile reference](https://doc.rust-lang.org/cargo/reference/profiles.html#split-debuginfo)
documents `unpacked` as the macOS default when debug information is enabled;
the
[rustc reference](https://doc.rust-lang.org/rustc/codegen-options/index.html#split-debuginfo)
states that this leaves macOS debug information in per-compilation object
files. The verifier intentionally retains and accepts only the dylib itself
or one UUID-matched `.dSYM`, so the default unpacked layout cannot satisfy the
immutable diagnostic-bundle contract. Both Apple architectures failed the
same step within seconds of their successful diagnostic builds. The focused
repair is to request `split-debuginfo="packed"` only for Apple diagnostic
builds, retain the generated `.dSYM`, and keep the existing UUID/DWARF and
local-symbol assertions. That repair is implemented locally: `actionlint`,
independent YAML parsing, shell syntax checks, Cargo profile-override parsing,
and three-target argument expansion prove that only Apple receives the packed
override. The second attempt below proves that repair on both Apple runners.

## Second atomic attempt

[GitHub Actions run 29784081881](https://github.com/BoundaryML/baml/actions/runs/29784081881)
was triggered by exact tag
`csharp-entry-gates-c44ac516a6f71fac143c4ff239beae424b042222` at source
SHA `c44ac516a6f71fac143c4ff239beae424b042222`, event `push`, attempt 1,
release version `0.15.0`. The immutable-plan and central-matrix jobs passed.
All eight producer jobs passed shipping build/upload, diagnostic build,
platform-specific debug/symbol verification, and diagnostic upload. Both
Apple architectures therefore prove the packed dSYM repair, including exact
architecture, dylib/dSYM UUID equality, a DWARF compile unit, local symbols,
and canonical manifest/upload paths. Both Windows PDB verifiers also passed.

The verifier's native-consumer matrix computation passed. Package assembly
downloaded the current-attempt artifacts, validated every shipping
checksum/identity, staged exactly one asset for each of the eight RIDs, and
accepted both Apple plus all four Linux diagnostic manifests. It then failed
while consuming
`x86_64-pc-windows-msvc/diagnostic-verification.txt`. The producer wrote
PowerShell's CRLF line endings; the Linux assembly job's exact `grep -Fxq`
identity checks correctly rejected the trailing carriage return. Downloaded
evidence reproduces the failure and independently proves that the Windows x64
DLL/PDB manifest, PE/PDB identity, procedure symbols, sizes, and distinct
shipping/diagnostic digests all pass. No normalized package, native consumer,
protocol-host, semantic/deployment, or completeness artifact was produced.

The focused local repair writes this cross-platform identity file as explicit
BOM-free LF text on Windows before hashing it into the immutable manifest. It
does not weaken any identity or debug assertion. A new reviewed commit and
matching exact-source tag must prove the repair.

## Third atomic attempt

[GitHub Actions run 29785957216](https://github.com/BoundaryML/baml/actions/runs/29785957216)
was triggered by exact tag
`csharp-entry-gates-ccf3bcfadd5a919b2cbee205ace07a1ac9cd565c` at source
SHA `ccf3bcfadd5a919b2cbee205ace07a1ac9cd565c`, event `push`, attempt 1,
release version `0.15.0`. All eight shipping/diagnostic producers and the
package assembly job passed. The normalized 15-entry package is `68,548,097`
bytes, SHA-256
`9195e1dd1cf8886c68d4f07bfa2ee87049537cb2787f73b04d6655036883b029`,
and was built/normalized twice in 36 seconds under the exact
`200,000,000`-byte ceiling. It contains eight distinct shipping natives
totalling `180,794,948` bytes. The unstripped diagnostic bundles total
`2,458,529,632` bytes and normalize reproducibly to `536,208,515` bytes.
`TASK/package-feasibility-evidence.md` records every per-target measurement
and digest.

Four real native consumers concluded success: `linux-arm64`, `linux-x64`,
`osx-arm64`, and `osx-x64`. Each restored the exact package from a cold private
feed, published exactly one selected native with the package digest, executed
the ABI/lifetime and representative ordinary-call probe, passed the exact
eight-RID policy, inspected architecture/dependencies/minimum platform and
the 26-symbol allowlist, and uploaded current-attempt evidence. Both Windows
consumers also completed restore, publish, ABI/lifetime execution, and RID
policy. Their final digest comparison failed only because Git-for-Windows
`sha256sum` prefixes a backslash when escaping an absolute filename; the
64-hex digest following that marker exactly matched the manifest on x64 and
ARM64. The local repair removes only that optional marker, validates exactly
64 lowercase hex digits, and retains exact manifest comparison.

Both real-musl jobs failed earlier: the outer workflow environment set
`BamlNativeProbeMode=Package`, but `docker run` forwarded only RID, canonical
asset, architecture, and version. The fail-closed MSBuild target therefore
rejected restore before any package execution. The local repair forwards the
existing mode unchanged into the pinned Alpine .NET container.

All three protocol builders passed, and their consistency fan-in proved four
generated files byte-identical across Linux x64, macOS ARM64, and Windows x64.
The exact generated-source digests are:

| Generated source | SHA-256 |
| --- | --- |
| `BamlHandle.g.cs` | `cc5110d0e1e781657c1c4f50c33ce20e67b8ed1dd08fcdaca2dd5e353d25eeb2` |
| `BamlInbound.g.cs` | `14990178481898ef75b5308f5bc7f669baabeb2df4ba8231251f978b04ef275a` |
| `BamlOutbound.g.cs` | `df127ff0b8358a26f03da5b6df9dedcbf835e04284790b19eae7fcd46ff216a6` |
| `BamlType.g.cs` | `24c75222a3d23fee4bdb68df97299a5b77905eb39f73b5b3f76b92f9989ff7a9` |

The semantic/deployment job passed the exact package's stream fixture,
trimmed ABI/media/stream/managed/reflection/RID probes, all four untrimmed or
trimmed sidecar/self-extract shapes, four byte-identical native copies,
`BAML0010`, and `BAML0019`. The stream retained 789 UTF-8 bytes with SHA-256
`2e950ddbdb0c2e12f64c09bc6e4a72f687367894cdea17d632529fd6719d2ef2`.
The final completeness job correctly skipped because four consumer jobs did
not conclude success. No package was published and no release or registry
state changed.

## Fourth atomic attempt

[GitHub Actions run 29788598100](https://github.com/BoundaryML/baml/actions/runs/29788598100)
was triggered by exact tag
`csharp-entry-gates-991f491fba8cb10543b1cbb2aba33b5d9b3079bc` at source
SHA `991f491fba8cb10543b1cbb2aba33b5d9b3079bc`, event `push`, attempt 1,
release version `0.15.0`. Twenty-three jobs passed, two Windows consumers
failed, and final completeness skipped. All eight producers and deterministic
package assembly passed. The normalized 15-entry package is `68,548,074`
bytes, SHA-256
`9e6c1b7b6c0c24048106b2abd8a26bd97c1a4a558059a8d119f5cf8e53db5a83`,
and was built/normalized twice in 37 seconds under the exact
`200,000,000`-byte ceiling. Its shipping natives total `180,794,948` bytes;
the diagnostic bundles total `2,458,570,592` bytes and compress reproducibly
to `536,212,176` bytes. The Actions package artifact is `68,491,349` bytes,
digest
`sha256:a20b0f72a025cd47ce7f6cc6cada34278ec6122ce6f4413df9405773ffbbef69`;
the diagnostic artifact is `533,443,852` bytes, digest
`sha256:20948c93b5454a3bd8ca2a81fab9b6e4139981e98be030c373daa4a197f873f8`.

Both real-musl consumers passed restore, publish, exact one-native selection,
ABI/lifetime and ordinary-call execution, exact digest comparison, RID policy,
ELF dependency/RPATH/strip inspection, and the 26-symbol export allowlist.
The other four Unix consumers also passed. This proves the Docker environment
forwarding repair. Both Windows consumers passed the same execution boundary,
including exact checksum normalization; x64 selected the `24,422,400`-byte
native at SHA-256
`3431e448b573a004af2911b051daf47392964a4c00d8dfd97d8eee241fc814aa`,
and ARM64 selected the `22,409,728`-byte native at SHA-256
`5189b58fd456edd2c9b42d16e0dacea1531772863551fc4c118f0c8ac6e38e33`.
Each then failed PE export comparison on the identical nine extra symbols:

- `aws_lc_0_41_0_jent_entropy_collector_alloc`
- `aws_lc_0_41_0_jent_entropy_collector_free`
- `aws_lc_0_41_0_jent_entropy_init`
- `aws_lc_0_41_0_jent_entropy_init_ex`
- `aws_lc_0_41_0_jent_entropy_switch_notime_impl`
- `aws_lc_0_41_0_jent_read_entropy`
- `aws_lc_0_41_0_jent_read_entropy_safe`
- `aws_lc_0_41_0_jent_set_fips_failure_callback`
- `aws_lc_0_41_0_jent_version`

The pinned non-FIPS `aws-lc-sys 0.41.0` bundles a jitter-entropy library whose
MSVC header marks those functions `__declspec(dllexport)`. Static linkage
therefore widens the final DLL ABI even though the symbols are dependency
internals. The local repair sets aws-lc-sys's supported
`AWS_LC_SYS_NO_JITTER_ENTROPY=1` build opt-out on Windows only. It retains the
selected AWS-LC backend and all bridge exports, does not claim FIPS behavior,
and keeps the exact 26-symbol assertion unchanged. Another exact-source run
must prove both Windows architectures contain no dependency exports and still
execute the package normally.

All three protocol builders, protocol consistency fan-in, and the complete
semantic/deployment lane passed again. The final completeness job correctly
skipped because the two Windows consumers did not conclude success. No package
was published and no release or registry state changed.

## Provenance preflight

The first provenance commit's temporary-index preview contained exactly the
intended 120 source/evidence files. `git diff --cached --check` passed, Git
reported zero binary entries, and the scope contained zero paths under local
`.codex/`, `.TASK.readonly-seed/`, `AGENTS.md`, or any excluded directory.
That scope became commit
`6d52aff1446c66be440771a14b85512c67214ca1`; trigger bootstrap commit
`9d29c01928df7ce726c49286a3067129fc039115` was the first atomic attempt's
source.

The Windows AWS-LC build repair and post-run ledger updates require a new
reviewed source commit and a new matching
`csharp-entry-gates-<full source SHA>` tag. Its precommit scope must contain
only the native-builder workflow plus these four continuously maintained
ledger files. The local `.codex/`, `.TASK.readonly-seed/`, `AGENTS.md`, and all
excluded paths remain outside that commit.

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
| reusable verifier workflow | `d21a9a2d95ef991e65084b66ed02d7bfb4861a05f1a1d24acde8d21b798f4a93` |
| native builder workflow | `4f0c787272179dcc9fd4ed5c0680d31a413d456c0dfc07d360572a9905f42c98` |
