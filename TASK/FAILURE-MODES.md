# BAML Failure Modes

This document lists the user-facing failures we expect from the wrapper/toolchain/runtime split.

## Compatibility Policy

| Surface | Compatibility Rule |
|---|---|
| `baml-wrapper` | Must understand the manifest schema and project `[toolchain]` schema. It may be older than the toolchain if schemas are still supported. |
| Internal toolchain binary / LSP | Must match the project-selected concrete toolchain version. The wrapper should prevent accidental skew. User-facing docs and diagnostics should say `baml`, not `baml-cli`, except when describing internal implementation. |
| Canary SDK runtimes, such as Python `baml_core` | Compatible across the same major/minor line. Patch skew is allowed unless a release explicitly marks a patch as breaking. |
| Nightly SDK runtimes | Prefer exact canonical version match unless we explicitly define nightly ranges later. |
| VSIX / IDE client | Compatible by explicit LSP/playground protocol range and capability flags, not by exact BAML toolchain version. Protocol metadata is exchanged through existing LSP initialize and playground WebSocket startup paths, without network checks. |

## Expected User Errors

| Case | Tool That Fails | Expected Error |
|---|---|---|
| `baml.toml` pins `0.3.0`, but the wrapper is too old to understand the project or manifest schema | `baml` wrapper | `This project/manifest requires wrapper schema support newer than this baml wrapper. Update the wrapper with: baml self-update` or the package-manager command, such as `brew upgrade baml`. |
| `baml.toml` pins `0.3.0`, but only toolchain `0.4.0` is installed | `baml` wrapper | `Project pins BAML 0.3.0, but that toolchain is not installed. Installed toolchains: 0.4.0. Run: baml toolchain install 0.3.0`. The wrapper must not fall forward to `0.4.0`. |
| `baml.toml` selects `nightly`, local nightly is installed, network is unavailable | `baml` wrapper | Normal commands keep using the installed concrete nightly. `baml toolchain update` reports that the latest nightly could not be checked. |
| `baml.toml` selects `nightly`, but no concrete nightly is installed and network is unavailable | `baml` wrapper | `No installed nightly toolchain is available, and the latest nightly could not be fetched. Connect to the network or install a concrete version.` |
| User runs the internal toolchain binary directly inside a project pinned to another version | Internal toolchain binary | Print a warning to stdout before continuing or failing: `Using the internal BAML toolchain binary directly is not recommended. Use baml instead.` If it detects a version mismatch: `This project pins BAML 0.3.0, but this toolchain is 0.4.0. Run: baml toolchain install 0.3.0`. |
| Installed toolchain directory exists but is missing `baml-cli` or checksum metadata | `baml` wrapper | `Installed toolchain 0.3.0 is corrupt. Reinstall with: baml toolchain install 0.3.0 --force`. |
| Manifest points to an archive whose checksum does not match | `baml` wrapper | `Checksum verification failed for BAML 0.3.0. The toolchain was not installed.` |
| Project/generated Python code expects `0.3.x`, but `baml_core` is `0.2.0` | Python runtime / generated client | `BAML runtime mismatch: generated with 0.3.x, but baml_core is 0.2.0. Update the Python runtime with: uv add 'baml_core~=0.3' or pip install 'baml_core~=0.3'. Then regenerate with: baml generate.` |
| Project/generated Python code expects `0.3.0`, but `baml_core` is `0.3.2` | Python runtime / generated client | Allowed for canary releases. No error if major/minor match and the generated compatibility range permits it. |
| Project/generated Python code expects `0.3.x`, but `baml_core` is `0.4.0` | Python runtime / generated client | `BAML runtime mismatch: generated for 0.3.x, but baml_core is 0.4.0. Downgrade the runtime with: uv add 'baml_core~=0.3' or pip install 'baml_core~=0.3'. Or update the toolchain with: baml toolchain update, then run: baml generate.` |
| Nightly generated code expects `0.3.1-nightly.20260529.a`, but `baml_core` is another nightly | Python runtime / generated client | Fail exact-match by default: `Nightly runtime mismatch. Install the exact baml_core version with your Python package manager, or regenerate with: baml generate.` |
| Node/TypeScript runtime package does not match generated-code compatibility range | Node/TypeScript runtime / generated client | Same policy as Python: canary patch skew allowed within range; major/minor mismatch fails; nightly exact-match unless changed later. |
| VSIX launches an LSP whose BAML LSP protocol range is unsupported | VSIX / IDE client | Show an IDE diagnostic and avoid BAML custom requests: `This BAML extension is not compatible with the selected BAML toolchain's LSP protocol. Update the extension or switch toolchains.` |
| Playground webview connects to an LSP playground server whose playground protocol range is unsupported | VSIX / IDE client | Keep LSP/editor language features running, but disable or fail only the playground: `This BAML extension is not compatible with the selected BAML toolchain's playground protocol. Update the extension or switch toolchains.` |
| VSIX cannot find `baml` on `PATH` and no configured path exists | VSIX / IDE client | `BAML command not found. Install BAML or set the BAML executable path in extension settings.` |
| User invokes `baml self-update` from a Homebrew/AUR-managed wrapper | `baml` wrapper | Refuse: `This wrapper is managed by Homebrew/AUR. Update with: brew upgrade baml` or the detected manager's command. |
| User invokes future package-style `baml install <package>` before package management exists | `baml` wrapper | `BAML package installation is not implemented yet. For toolchains, use baml toolchain install <version>.` |

## Error Ownership

| Tool | Owns These Failures |
|---|---|
| `baml` wrapper | Toolchain resolution, install/update, manifest/schema/checksum, corrupt cache, managed self-update refusal, missing local toolchain. |
| Internal toolchain binary | Direct-binary warning/version mismatch, language commands, generation, `pack`, `lsp`, and `ide install`. User-facing docs should describe these as `baml <command>` because the wrapper is what users run. |
| SDK runtimes (`baml_core`, Node runtime, future runtimes) | Generated-code/runtime compatibility checks at import/init/call time. They never self-update. |
| VSIX / IDE client | Finding `baml`, launching LSP, constant-based LSP/playground protocol compatibility diagnostics, extension install/update UX. It must not perform network checks or spawn extra `baml` commands merely to determine compatibility. |
| Package managers | Wrapper package installation and wrapper package upgrades. |

## Release Coupling Decision

We decouple the wrapper from the language toolchain.

| Product | Release Cadence | Why |
|---|---|---|
| `baml-wrapper` | Rare, independent | It only resolves, installs, updates, and forwards. Shipping it through package managers on every language release would create unnecessary churn. |
| `baml-toolchain` | Every canary/nightly language release | It contains the versioned language behavior: `baml-cli`, LSP, pack host, SDK runtimes, generated-code compatibility metadata, and VSIX artifact. |

We should not decouple `baml-cli`, Python `baml_core`, Node runtime/codegen surfaces, and VSIX artifacts inside the language release. They should be produced from one release plan and one canonical BAML version.

Reason: decoupling those surfaces creates a cross-product compatibility matrix for users and support. The plan should only have one compatibility question for a project: "Which BAML language version does this project use?"
