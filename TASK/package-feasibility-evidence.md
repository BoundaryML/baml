# C# atomic-package feasibility evidence

Status: B4 partially executed through the third exact-source atomic attempt on
2026-07-20. All eight producers and deterministic package assembly passed,
producing a measured exact eight-RID package under the ceiling. Four native
consumer jobs passed; both Windows consumers executed successfully and failed
only post-run checksum parsing, while both musl jobs failed before restore on
a missing Docker environment forward. The focused verifier repairs are local;
the package baseline is measured but not approved until all eight consumers
and final completeness pass atomically.

## Target and size authority

- Branch/start commit: `paulo/csharp-bridge` /
  `1ebf901f7896faaec4672fdc4b2f2835db2f1cc0`.
- Host: Ubuntu 26.04 x64; .NET SDK `10.0.110`, runtime `10.0.10`;
  Rust/Cargo `1.93.0`; `cross 0.2.5`.
- Product version: `0.15.0`.
- The official
  [NuGet publishing documentation](https://learn.microsoft.com/en-us/nuget/nuget-org/publish-a-package#package-size-limits)
  says nuget.org has a package size limit of about 250 MB. The
  [nuget.org FAQ](https://learn.microsoft.com/en-us/nuget/nuget-org/nuget-org-faq#what-is-the-maximum-size-of-packages-i-can-upload-to-nugetorg)
  states that packages up to 250 MB are allowed.
- Q10's exact safety ceiling is `200,000,000` bytes: 80% of a conservative
  decimal `250,000,000`-byte interpretation, and below 80% of 250 MiB as well.
  Both the primary normalized unsigned `.nupkg` and the final signed package,
  if signing is enabled, must remain below this exact byte count.

This freezes the ceiling, not the baseline. The baseline cannot be approved
until the package contains all eight real release-matrix binaries.

## Current Linux x64 native baseline

The repository workflow derives the CFFI matrix from `release/platforms.json`
and builds `bridge_cffi` with the `release-bridge-cffi` profile. A direct host
execution of the exact target/profile command completed:

```text
cd baml_language
env RUSTC_WRAPPER= cargo build \
  -p bridge_cffi \
  --profile release-bridge-cffi \
  --target x86_64-unknown-linux-gnu
```

The only warning was the pre-existing unrelated dead-code warning in
`baml_compiler2_emit/src/verifier.rs`. Artifact:

| Property | Local direct-profile result |
| --- | --- |
| Path | isolated current-source `cargo-shipping/x86_64-unknown-linux-gnu/release-bridge-cffi/libbridge_cffi.so` |
| Shipping size | `24,661,376` bytes |
| SHA-256 | `dc8d399dbfaa14327be3eed25a52fa8661333ce211ca2d5a38fe0a833a323432` |
| Format | ELF64 x86-64 shared object, dynamically linked, stripped |
| Required libraries | `libgcc_s.so.1`, `libm.so.6`, `libc.so.6`, `ld-linux-x86-64.so.2` |
| RPATH/RUNPATH | none |
| Highest local symbol-version requirement | `GLIBC_2.39` |

The exact workflow diagnostic command uses Cargo CLI overrides so `cross`
cannot discard them:

```text
cargo build -p bridge_cffi \
  --profile release-bridge-cffi \
  --target x86_64-unknown-linux-gnu \
  --target-dir target/csharp-entry-unstripped \
  --config profile.release-bridge-cffi.strip=false \
  --config profile.release-bridge-cffi.debug=2
```

Against the same current sources it produced a `329,440,320`-byte ELF,
SHA-256
`9e2b7469050ee349f12b2f24b11b5d4113a9a9d6ff3c4aa94c681b68d28b563f`.
`file` reports `with debug_info, not stripped`; `readelf` confirms
`.debug_info`, `.debug_line`, and `.symtab`. The shipping profile removes
`304,778,944` bytes (`92.51%`) from that diagnostic primary while retaining
the unwind-enabled release behavior. The external workflow also uploads
platform-native PDB/dSYM/split-debug sidecars when Cargo emits them and hashes
every file in each diagnostic bundle.

The current .NET 10 host probe loaded that exact current shipping digest
through the source-generated getter and reported:

```text
api_v1_size=176
product_version=0.15.0
csharp_registration=ok
```

The dynamic symbol inspection found 26 exports:

```text
__testonly_seed_function_ref
__testonly_seed_generic_media
baml_get_api_v1
baml_handle_clone
baml_handle_release
baml_media_base64
baml_media_file
baml_media_from_base64
baml_media_from_file
baml_media_from_url
baml_media_mime_type
baml_media_url
call_function
cancel_function_call
complete_host_call
create_baml_runtime
destroy_baml_runtime
flush_events
free_buffer
initialize_runtime_from_bytecode
invoke_runtime_cli
new_function_call
register_callback
register_host_dispatch_callback
register_host_release_callback
version
```

`baml_get_api_v1` remains the only symbol the C# bridge resolves. The other
symbols are current shared-library compatibility exports; the two test-seed
exports are also consumed by existing Python/TypeScript/Go bridge test
machinery and cannot be removed as a C# packaging shortcut. The final release
inspector must compare every RID against a centrally reviewed exact allowlist.

This artifact is a useful local size/load baseline but is not the canonical
release Linux x64 input: the workflow uses `cross` for this target so that it
does not inherit the audit host's `GLIBC_2.39` floor. The exact local `cross`
command, an escalated retry, a retry with `RUSTC_WRAPPER` empty, and a
local-only `CROSS_CUSTOM_TOOLCHAIN=1` attempt all stopped before the container
build because the host rustup process aborted in
`wait-timeout-0.2.1`'s `sigchld_handler`:

```text
bad error on write fd: Operation not permitted (os error 1)
couldn't install toolchain `1.93.0-x86_64-unknown-linux-gnu`
```

The pinned toolchain is already installed and active. Repeating the same
command is not a useful local action; the canonical CI runner or a host
without this signal-handler restriction must produce the release artifact.

## Deterministic unsigned-package normalization

NuGet's packer emits a random core-properties part name and root relationship
ID on each raw pack. Two independent packs of identical deterministic managed
inputs had different hashes and sizes:

| Raw package | Size | SHA-256 |
| --- | ---: | --- |
| A | `74,068` | `a3ffb663b68496cf143432e722456a042ddfbbccf339013aeca64400ab0c39b7` |
| B | `74,069` | `9dda4e227f6865c7621da0897ed5937ac4dc2f98d3019677f9a66c5bdbeb5ef9` |

The evidence-tree normalizer is:

```text
baml_language/sdks/csharp/bridge_csharp/tools/Baml.NuGetNormalizer
```

Source SHA-256:

| Source | SHA-256 |
| --- | --- |
| `Baml.NuGetNormalizer.csproj` | `083bf25d9e1fcad0bff524b36ba3f2920a31ce2a79b85ce8707834c39db3a67e` |
| `Program.cs` | `458cd22fe8babcfde46357dd5f12de3141d4877ac5be618c26d1ad81be652474` |
| `README.md` | `3c78bd9bc50a36a539b6a355b71a57a36197e022393920fe96d7d1e6201d85ff` |

Release build completed with zero warnings/errors. It:

- rejects signed input and never overwrites or rewrites an artifact in place;
- rejects unsafe, duplicate, and case-colliding ZIP paths;
- validates root OPC metadata and requires one core-properties relationship;
- uses a fixed core-properties path;
- derives stable relationship IDs from relationship semantics;
- updates a content-type override when present;
- sorts entries ordinally and fixes timestamp, Unix regular-file mode `0644`,
  and compression;
- preserves every non-OPC payload byte; and
- writes through a same-directory temporary file before one atomic move.

Normalizing the two differing raw packages yielded byte-identical
`73,669`-byte files with SHA-256
`7627776b998f2961716b2a392b3a7f241a02dbc8ee10f964d58a6376e3e95583`.
The nuspec, managed DLL, and core-properties payload hashes matched before and
after normalization. `zipinfo` reports `-rw-r--r--` for every normalized
entry; the prior zeroed external attributes were corrected because they
extracted as mode `000` with generic ZIP tools. A fresh isolated exact-package
restore/build/run accepted the normalized package and reported:

```text
exact_package_consumer=ok
transport_generation=absent
public_protobuf_surface=absent
```

Negative processes rejected a signed package, an in-place rewrite, an existing
output, and a `_rels/.rels` / `_RELS/.rels` case collision, and left no output
artifact.

The same normalizer processed the actual B1 native package twice
byte-identically. Its normalized package was `7,622,017` bytes with SHA-256
`f09265ff1570a9bd5b04ccc2038f7e70516e47724b27ed15b5f68f786d5f68f6`;
the embedded and RID-specific published native file remained byte-identical at
SHA-256
`cdb5bcbe5b23ab973953a4ec000e0d37413741c594d2b3c0365a0278e9be06ad`.
A fresh package-only cache and publish executed from `/tmp` and reported:

```text
product_version=0.15.0
resolution=package-default
packaged_getter_table=ok
```

This fixes the unsigned release identity point. Signing, if configured, occurs
after normalization and exact-byte inspection; no normalizer runs after a
signature is applied.

## Provisional compression envelope

An earlier feasibility packing of the then-current `24,662,112`-byte
direct-profile artifact into the one-RID fixture and normalizing it produced a
`9,213,296`-byte package,
SHA-256
`fb490624e6c3ee66121e0fec8251c4df6b91076ade559f15e5f08b014384a48f`.
The package executed through default RID resolution and published the exact
`aab8f1...` native bytes.

That historical native input compressed to `37.36%` of its raw size. Merely copying
this measured one-RID package cost eight times would be `73,706,368` bytes,
`36.85%` of the safety ceiling. This is an arithmetic risk indicator only:
different formats/architectures and the final managed assembly may compress
differently, and duplicated Linux bytes are expressly forbidden as B4 proof.

## CI-ready external closure

The manual, non-publishing workflow and evidence package described in
`TASK/csharp-entry-gates-handoff.md` now encode every externally executable B4
requirement. One job freezes the source SHA and release-plan JSON consumed by
every native build and verifier, and every reachable external action is pinned
by exact upstream commit SHA. Each CFFI entry in
`release/platforms.json` owns its .NET RID, canonical package filename, and
consumer runner; the workflow has no second triple-to-RID table. The evidence
run requires all eight targets even when the upstream artifact is marked
experimental. It validates each producer `.sha256` sidecar, requires exactly
one shipping input and one debug-enabled, unstripped diagnostic bundle per
target whose primary library differs and whose total size is larger, the
reviewed 26-symbol allowlist, and platform-native diagnostic proof: ELF debug
sections plus symbol table, Mach-O non-stripped state plus DWARF compile unit,
or a valid Windows PDB plus PE debug directory. Every diagnostic primary also
has an exact target-architecture check; Windows reads only the PDB signature
and streams procedure-symbol inspection. It then requires one compiled managed
assembly, two byte-identical normalized packs, the exact complete
15-entry inventory and ceiling, cold-cache restore/publish measurements, and
one representative package-default BAML call on every native runner.

The exact package is isolated with NuGet package-source mapping: only
`Baml.Bridge.MultiRidPackageProbe` can resolve from the private evidence feed,
while the narrowly allowed Microsoft/Google/Grpc dependencies can resolve
from nuget.org. Every package-mode restore compares the cached `.nupkg` bytes
to the normalized evidence artifact. Raw ELF/Mach-O/PE inspections, consumer
measurements, protocol outputs, semantic deployment outputs, and package data
are uploaded. Each unstripped bundle is also normalized twice into a
metadata-stable `tar.gz`; byte identity is required, and one measured archive
per RID is retained in its own source/run/attempt/release-stamped artifact.
A final fan-in requires 15 named evidence artifacts and hashes every contained
file.

The package fixture is
`baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.MultiRidPackageProbe`.
Its project packages only a generic `runtimes/**/*` asset glob and requires
exactly eight native files. The verifier derives the supported RID list from
the same `release/platforms.json` matrix and expands the checked-in
`Baml.Bridge.MultiRidPackageProbe.targets.in` template before pack; neither
the project nor template duplicates eight native paths or a handwritten RID
list. The generated `buildTransitive` target carries bounded checks for
explicit unsupported RIDs (`BAML0010`) and NativeAOT (`BAML0019`). The
negative fixture permanently excludes `bin/**;obj/**` so isolated output
roots cannot ingest stale generated attributes. Its unsupported-RID lane
restores without the deliberately invalid RID, then supplies `linux-s390x`
only to the no-restore build so `BAML0010` wins before host-pack resolution.
The
separate warning-free RID policy fixture proves the eight exact runtime mappings and
`PlatformNotSupportedException` outcome for unsupported OS/architecture/libc
combinations without substitution.

A local mechanics-only run duplicated the current Linux x64 library into all
eight package paths and therefore is not B4 evidence. It did prove the
single-managed-build/two-pack workflow, deterministic normalization,
`0644` permissions, exact package-default actual calls, and both packaged
build diagnostics. Its normalized package was `60,964,748` bytes with SHA-256
`61eda292f7a5dab4565f38cb679feb5046c18b18449e517c9d7c34b074b7ab72`.
The real package baseline remains unset until the workflow consumes eight
distinct immutable release artifacts.

## Remote eight-target builder diagnostic

The already registered language-neutral native builder was dispatched
directly as a safe non-publishing diagnostic. [Run
29620985984](https://github.com/BoundaryML/baml/actions/runs/29620985984)
completed successfully at exact source
`6d52aff1446c66be440771a14b85512c67214ca1`, attempt 1, release version
`0.15.0`. All eight jobs, including the three targets marked experimental for
other upstream uses, concluded `success`; each produced a distinct shipping
binary, matching SHA-256 sidecar, and run/source/release identity sidecar.
`TASK/csharp-entry-gates-handoff.md` records every binary's exact size and
digest.

This direct diagnostic did not request the unstripped native builds and did
not invoke the C# verifier. It therefore produced no paired debug bundles,
platform-native debug proof, normalized package, package size/baseline,
consumer executions, or atomic completeness manifest. It proves that the
current branch's eight shipping builders can complete on their registered
runners; it is not B4 evidence for an atomic package and does not unblock C6.

The first exact-source atomic attempt, [run
29626183183](https://github.com/BoundaryML/baml/actions/runs/29626183183),
used tag and source SHA
`9d29c01928df7ce726c49286a3067129fc039115`. Its immutable plan and all six
non-Apple shipping/diagnostic producers passed. Both Apple shipping and
diagnostic builds also completed, but their diagnostic verification failed
because the build left Cargo's macOS debug layout at the documented
`split-debuginfo=unpacked` default while the immutable bundle contract accepts
the dylib or one UUID-matched `.dSYM`. The diagnostic uploads and downstream
atomic verifier were correctly skipped. No B4 or C6 promotion is claimed.

The second exact-source atomic attempt, [run
29784081881](https://github.com/BoundaryML/baml/actions/runs/29784081881),
used tag and source SHA
`c44ac516a6f71fac143c4ff239beae424b042222`. All eight shipping and
diagnostic producers passed, including the repaired packed dSYM checks on
both Apple architectures and the existing PE/PDB checks on both Windows
architectures. Package assembly validated the current-attempt shipping
identities/checksums, staged one native per RID, and accepted both Apple plus
all four Linux diagnostic manifests. It stopped on the first exact identity
line in the Windows x64 diagnostic verification file because PowerShell had
written CRLF and the Linux consumer uses exact whole-line matching. The
downloaded Windows x64 artifact confirms that its DLL/PDB manifest, PE/PDB
debug evidence, sizes, and distinct shipping/diagnostic digests are valid;
only the cross-host text encoding violated the immutable contract. No package
or consumer output was produced, so no B4 or C6 promotion is claimed.

The third exact-source atomic attempt, [run
29785957216](https://github.com/BoundaryML/baml/actions/runs/29785957216),
used tag and source SHA
`ccf3bcfadd5a919b2cbee205ace07a1ac9cd565c`. All eight producers and
package assembly passed. The immutable package evidence is:

| Property | Exact result |
| --- | --- |
| Package bytes | `68,548,097` |
| Package SHA-256 | `9195e1dd1cf8886c68d4f07bfa2ee87049537cb2787f73b04d6655036883b029` |
| Pack/normalize time | `36` seconds |
| Exact inventory | `15` entries: managed assembly, metadata/build files, and eight runtime natives |
| Shipping native total | `180,794,948` bytes |
| Diagnostic total | `2,458,529,632` bytes |
| Reproducible compressed diagnostics | `536,208,515` bytes |
| Bytecode | `683,918` bytes / `44ec354587d912e222d0263e3bc8a944514195da2c134e9e1db6ce4e202d66f2` |
| Package ceiling | `200,000,000` bytes, passed |

Per-target shipping and diagnostic-primary measurements:

| Target | Shipping bytes | Shipping SHA-256 | Diagnostic primary bytes | Diagnostic primary SHA-256 |
| --- | ---: | --- | ---: | --- |
| `aarch64-apple-darwin` | 21,111,072 | `76c157a8c8b68d2607ba1ac00f0abb780a1080d88555d575b69bf2cb748f0ddc` | 32,335,928 | `d152f08f331cd855937178122dc77553ff0b527bacbbedfdf5f786b73d177e7f` |
| `x86_64-apple-darwin` | 21,539,636 | `df4c64c8ae040e99d3f4a0b67ee52355107e5d1ef28aedc4770a907ff1d57991` | 32,699,296 | `5ae418eb827d68151a02480d5f829dedf6949f9ed3233599bffdab066665218c` |
| `aarch64-unknown-linux-gnu` | 21,446,720 | `9410ac423d2f7a2d86282d7a8435e0b160c531f40e21ad9de23e8ce3f185cfdc` | 327,083,656 | `aa155d0a9ab135b9b42217f4083e4fabe8537ec0181b6cf1984803408d9e7a3b` |
| `aarch64-unknown-linux-musl` | 21,376,824 | `66b0ff4c0af3d393e295e8e5394fc5e39e59abf5c3a1518d09ef42384c0a00c4` | 327,747,632 | `ef3f17c0ecffd9b18a315cc8a4650dc3de96b21817abaf8d0482426f99c2919e` |
| `x86_64-unknown-linux-gnu` | 24,318,040 | `e545e6dca35bdb6c119961d088a65ce1f5ed12c9ab91db1177ca9e0a328e2e4f` | 320,241,312 | `57af4bb42e6b96ad11ff668f1a1804c45d10cc005cdfee3c32428a9eb0e574de` |
| `x86_64-unknown-linux-musl` | 24,170,528 | `f6fbe864eb4b994c7b3424b8a8e65e85208199882dd8bb61834776e147604fff` | 320,745,912 | `9184ac223533920b934aedcc3e2f7c38b89dc3ba9c1b4470d2079be919afae93` |
| `x86_64-pc-windows-msvc` | 24,422,400 | `52443df0eb1efbfd9427f4f904ed31059fdaddbc9ddf74784eb37f715029ab41` | 24,429,568 | `0b61624f7f5860b525b07a6d70f8361d8ef591371e74ad2c12fe02796c907d87` |
| `aarch64-pc-windows-msvc` | 22,409,728 | `0f5aa78d83c8a881e78eccd7f726076547889d20e0d4a2d0a706fc5fe6cef8d2` | 22,403,072 | `ae3eb9efd8a43ac8d4e58fc9ef9669ffed659112e3fb4cd909ffa99fa2b0cd79` |

Per-target complete diagnostic-bundle measurements:

| Target | Bundle bytes/files | Archive bytes | Archive SHA-256 |
| --- | ---: | ---: | --- |
| `aarch64-apple-darwin` | 272,942,143 / 7 | 69,393,492 | `21e019e9f78568827b58606f853e5587db5f25db269bef878fa6dc5a67a69ed4` |
| `x86_64-apple-darwin` | 274,457,341 / 7 | 71,112,946 | `d2c58b04d365a33193e01622373b65257f58703653a33b6acea6aba16beb9368` |
| `aarch64-unknown-linux-gnu` | 327,089,363 / 4 | 66,354,045 | `6fdee77549e05aaee555c7dc71c6b49205acfd0c940f27e7d46647bddf6e06e4` |
| `aarch64-unknown-linux-musl` | 327,753,012 / 4 | 66,683,036 | `44a48abb74a3c6b0e40675871e083aa0289d596e4a78ecd357d498fd796abe81` |
| `x86_64-unknown-linux-gnu` | 320,246,954 / 4 | 67,770,900 | `8932ebe353b585dc6cc770739a4b8877b6dbe515fa08d3604bae7bfb39a15643` |
| `x86_64-unknown-linux-musl` | 320,751,587 / 4 | 67,983,801 | `581d9ab4faa7253471d4d315adb88bedb4266c856c41f602c8e924950b540ecd` |
| `x86_64-pc-windows-msvc` | 329,403,797 / 5 | 64,826,527 | `a8c3b29249853c8457687973022d11b8a4cef749bedd3d829a1b8fbd7268ece4` |
| `aarch64-pc-windows-msvc` | 285,885,435 / 5 | 62,083,768 | `c5d6020165efc3532a108deafaa91f7683525abaed8aef9725238eb145f1700b` |

The package artifact itself has Actions artifact digest
`sha256:a96470dd11d710cbd8dd60efc2d821e0a48194e5e7708189e06f5087de90a66d`;
the retained diagnostic-archive artifact has digest
`sha256:76df7c677a23c1b5650ec71cc35e58fbd7359f54e6649fc22fc4461616087287`.

Four consumers (`linux-arm64`, `linux-x64`, `osx-arm64`, `osx-x64`) passed
completely. Both Windows consumers passed restore, exact package recovery,
publish, sole-native selection, ABI/lifetime behavior, ordinary calls, and
RID policy before the checksum parser retained GNU's leading escape marker.
Both musl containers failed before restore because the workflow did not
forward the already-required `BamlNativeProbeMode=Package`. Protocol
generation and semantic/deployment evidence passed, but completeness skipped;
no B4 or C6 promotion is claimed.

## Remaining closure

B4 remains blocked, not passed. The measured package is a valid feasibility
baseline but the third attempt did not complete all consumers or the final
digest. The locally validated musl environment/checksum-parser repairs must
be committed and executed from a new exact-source bootstrap tag; a passing
attempt must:

1. supply all eight real immutable artifacts from the frozen release plan;
2. inspect format, architecture, minimum OS/libc, dependencies, RPATH, exports,
   shipping/unstripped diagnostic sizes, and digest for each;
3. build the managed assembly once and assemble exactly the prescribed RID
   paths with no duplicate/unclassified native asset;
4. normalize the complete unsigned package and enforce the exact
   `200,000,000`-byte ceiling plus baseline-regression rule;
5. measure cold restore, expanded cache, RID publish, pack/restore time, and
   symbol/diagnostic artifact sizes;
6. execute the exact normalized package on every claimed native runner and
   prove one selected native file in each RID-specific publish; and
7. sign only if required, then reuse the exact signed bytes for final consumer
   verification and publication.

No package was published and no release/registry state was changed.
