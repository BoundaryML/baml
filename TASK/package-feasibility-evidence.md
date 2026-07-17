# C# atomic-package feasibility evidence

Status: B4 partially executed on 2026-07-17; blocked on all eight
workflow-produced target artifacts and the required native runner matrix.
Deterministic unsigned-package normalization and the current Linux x64 local
baseline are proved. The workflow, fixtures, and contract edits are included
in the local provenance change; the baseline SHA alone does not contain them.

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

## Remaining closure

B4 remains blocked, not passed. An authorized maintainer must execute the
reviewed, committed manual workflow and record its immutable outputs. That run
must:

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
