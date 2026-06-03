# Release Plan Amendments

These amendments capture the latest team feedback around commands, IDE install, VSIX behavior, and channel-cache behavior. They have been folded into `TASK/TASK.md` and `TASK/USER-PATHS.md`; this file remains as the review-comment rationale.

## 1. Command Surface

Use `TASK/COMMANDS.md` as the source of truth for the proposed command additions.

The key correction is:

- do not add `baml setup-ide`;
- do not make top-level `baml install` the toolchain/channel command;
- keep toolchain management under `baml toolchain ...`;
- add only one IDE command to the selected CLI payload: `baml-cli ide install`;
- allow users to run that IDE command as `baml ide install` through wrapper pass-through.

This keeps the wrapper clean while preserving room for future package-manager verbs such as `baml install <package>` or `baml add <package>`.

## 2. VSIX And LSP Compatibility

The VSIX should not force users to install a new extension every time they switch BAML toolchain versions.

Current pre-wrapper implementation reality:

- The old VSIX path started the internal CLI's LSP directly and read the LSP `initializeResult.serverInfo.version` for display.
- The native playground WebSocket sends a simple `ready` message when the playground server is connected.
- The browser/WASM playground worker also sends `ready` plus build-time metadata.
- This is readiness/version plumbing, not an explicit compatibility handshake.

Engine prior art:

- Engine's old VSIX listened for `baml_src_generator_version`, downloaded/resolved a matching `baml-cli`, and restarted the LSP when the project generator version differed.
- Engine also checked generator/runtime/client versions before generation.
- This is useful prior art for version alignment, but it is not enough for the new model because it fixes mismatches by switching/restarting the LSP. The new VSIX should tolerate users moving between projects/toolchains without requiring extension reinstalls or routine IDE restarts.

Desired behavior:

- The VSIX remains a thin IDE client that launches `baml lsp` through the configured `baml` path or `baml` from `PATH`; it should not invoke the internal toolchain binary directly.
- The wrapper resolves the project-specific toolchain before launching the selected toolchain's LSP.
- Two projects open in the same IDE can use different BAML toolchains without requiring the user to reinstall the VSIX or restart the IDE for each switch.
- Add explicit compatibility metadata between the IDE client/playground and the LSP: BAML toolchain version for display/debugging, plus integer protocol ranges for LSP and playground compatibility.
- Compatibility is not BAML semver equality. A new BAML release should not imply a new VSIX install unless the BAML-specific LSP or playground protocol has a breaking change.
- If the LSP and IDE playground are incompatible, the IDE shows a clear diagnostic with the exact action needed. This should be an exceptional extension/toolchain compatibility issue, not the normal per-release path.

Implementation shape:

- VSIX/LSP compatibility is piggybacked on the existing LSP `initialize` request/result. Do not add an extra process spawn, extra post-initialize request, or network check just to verify compatibility.
- VSIX sends `initializationOptions.bamlClient` with extension version, supported LSP protocol range, supported playground protocol range, and optional capability flags.
- LSP returns normal `serverInfo.version` plus `capabilities.experimental.baml` metadata with the selected toolchain version, current LSP protocol, current playground protocol, minimum supported client protocol, and capability flags.
- Playground WebSocket compatibility is checked only when the playground opens. In v1, the server sends a `hello` message with playground protocol metadata and then sends the existing `ready` message without waiting for a client round trip; the webview validates `hello` locally and suppresses playground behavior if incompatible. A client that only understands `ready` is treated as legacy protocol 0. `clientHello` is optional/future-facing and must not be required on the v1 startup path.
- The wrapper is not responsible for VSIX compatibility. It only resolves/installs/selects the toolchain and execs `baml-cli lsp`.

Performance constraints:

- No compatibility check may run `baml --version`, `baml toolchain list`, `baml toolchain update`, or any network command from VSIX activation.
- No compatibility check may block normal editor startup on playground-only compatibility.
- Compute compatibility once per LSP session and once per playground WebSocket session; do not validate on every message.

We may still generate a VSIX artifact for every toolchain release for reproducibility, smoke testing, and manual installs. That does not mean users are expected to install that VSIX for every toolchain version. Marketplace or normal extension updates should happen only when the IDE client/protocol actually needs to change.

## 3. IDE Install Timing

Do not automatically install the IDE extension as a side effect of installing the wrapper or installing/updating a toolchain.

Instead:

- `brew install baml`, curl install, and `baml toolchain use/install ...` should get the wrapper/toolchain into a working CLI state.
- When a user runs any `baml` command from a supported IDE terminal, BAML may detect that context and check whether the IDE extension appears to be installed.
- If the extension is missing, print a short one-time recommendation:

```text
BAML IDE extension is not installed for this editor.
Run: baml ide install
```

- The recommendation must not block the command being run.
- Non-interactive contexts should not prompt.
- The prompt should be rate-limited or recorded so repeated terminal commands do not spam the user.

The install command itself remains owned by `baml-cli ide install`; `baml ide install` is just wrapper resolution plus pass-through.

## 4. Channel And Manifest Cache

Normal pass-through commands must not perform a blocking network check just because the user is on `nightly`.

The wrapper should store enough local state to run offline:

- active selector, such as `canary`, `nightly`, or a concrete version;
- resolved concrete version currently in use;
- installed toolchain directories;
- cached channel manifests under `~/.baml/manifest-cache/`.

Initial cache policy:

- Channel metadata cache TTL is 24 hours.
- Only allowlisted commands make network requests:
  - `baml toolchain update`: always checks the network and advances the active channel when possible.
  - `baml toolchain install`: always resolves/downloads/verifies the requested selector or version.
  - `baml toolchain use`: checks the network only when the relevant channel cache is missing or expired.
  - `baml toolchain list`: local-only by default; a remote/latest mode may check the network when the relevant cache is missing or expired.
- All other commands, including `baml generate`, `baml run`, `baml describe`, `baml pack`, and `baml lsp`, use local state only and never hit the network.

Network fetch behavior:

- Treat the old cache entry as stale once a command decides to make a network request.
- Fetch to a temporary file, validate schema and checksums, then atomically replace the cache entry.
- If validation fails, delete the fetched temporary file and do not change the active concrete toolchain.
- If the network fails, keep running the already-installed concrete toolchain and report that the latest remote version could not be checked.

This gives nightly users predictable local execution while still making `baml toolchain update` and remote status commands honest about freshness.
