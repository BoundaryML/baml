**Highest Priority**
1. **VSIX multi-project/toolchain model is underspecified**  
   [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:603), [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:1000)  
   The doc says the VSIX launches `baml lsp` and should support projects/toolchains without reinstalling the VSIX, but it does not define whether the extension starts one LSP per workspace folder, one per `baml.toml` project root, or one global LSP. Since the wrapper resolves toolchain at process start from cwd, a single LSP launched from “first workspace folder” cannot safely support multiple open projects with different `[toolchain]` pins. This needs a concrete process model.

   **Answer:** Start one lazy VS Code `LanguageClient` / LSP process per BAML project root, not one global LSP and not one process per file. A BAML project root is the nearest ancestor of the active/open `.baml` file containing `baml.toml`; if none exists, fall back to the containing VS Code workspace folder; if the file is outside any workspace folder, use the file's directory as an ad-hoc root or run in limited-support mode.

   The VSIX maintains a `Map<ProjectRootPath, LanguageClient>`. When a `.baml` file opens or becomes active, it finds that file's BAML project root, starts a client if one does not already exist, and launches it as `baml lsp` with `cwd = <project root>`. This makes wrapper toolchain resolution match the CLI behavior for that project: `$BAML_VERSION`, nearest `baml.toml [toolchain]`, user default, then canary fallback.

   Project-root-per-client is required for monorepos where two sibling BAML projects may pin different toolchains. Workspace-folder-per-client is not sufficient because one workspace can contain multiple `baml.toml` roots. Nested roots are routed to the nearest BAML project root. To guard against VS Code document-selector leakage, each LSP receives its intended `projectRoot` in `initializationOptions.baml.projectRoot` and ignores documents whose nearest `baml.toml` root differs from that declared root.

   Playground ownership is per LSP process. Each project-root LSP owns its own playground server/port, and `baml.openPlayground` uses the client associated with the active editor's project root. Restart only the affected project-root client when that project's `[toolchain]` changes, the configured `baml` executable path changes, compatibility metadata changes after a wrapper/toolchain update, or that LSP crashes. Restart all clients only for truly global setting changes.

2. **Package-manager bootstrap may be impossible or inconsistent as written**  
   [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:96), [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:936)  
   It says package-manager artifacts install only the wrapper, but first-time `brew install baml` should leave a working canary toolchain. The doc does not say whether Homebrew post-install runs `baml toolchain install canary`, whether that is acceptable for Homebrew tap policy, and what AUR does since package scripts run as root and cannot safely write a user’s `~/.baml`. This needs per-package-manager mechanics.

   **Answer:** Package-manager installs must remain wrapper-only in v1. `brew install baml`, AUR `baml`, and AUR `baml-bin` install only the `baml` wrapper and must not download, install, select, or update a BAML language toolchain during package install, reinstall, or upgrade.

   The previous goal that `brew install baml` should leave the user with a working canary toolchain is removed. The replacement promise is: package managers put a working `baml` wrapper on `PATH` and print the exact user-scoped follow-up command to install/select the canary toolchain.

   Package-manager caveats/post-install messages should say:

   ```text
   BAML wrapper installed.

   To install and select the current canary language toolchain:
     baml toolchain use canary

   To use nightly:
     baml toolchain use nightly

   IDE extension setup is explicit:
     baml ide install --cursor
   ```

   Rationale:

   - AUR/package install scripts are system package lifecycle hooks and must not write user-scoped `~/.baml` state.
   - Homebrew supports post-install work, but using it to download and select a mutable language toolchain would blur the wrapper/toolchain boundary and make `brew install baml` depend on channel state at install time.
   - `brew upgrade baml` and `paru -Syu baml-bin` must update only the wrapper. They must not silently change the user's active toolchain/channel/project behavior.
   - Curl installers remain different: because they are explicitly user-scoped BAML installers, they may still install the wrapper and bootstrap the requested toolchain unless `--wrapper-only` is passed.

   This decision depends on point 3: `baml toolchain use <selector>` should be the one-command user setup path. If the selected concrete toolchain is missing, `use` should install it and then mark it active. `install` should download a toolchain without making it active.

3. **`baml toolchain use` vs `install` semantics are still ambiguous**  
   [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:274), [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:322)  
   The first-run error suggests `baml toolchain use canary`, but package-manager bootstrap uses both `install` and `use`. Network policy says `use` may fetch channel metadata, but does not clearly say whether `use` also installs the selected toolchain if missing. This is a core UX behavior and should be exact.

   **Answer:** `baml toolchain use <selector>` is the one-command setup path. It resolves the selector, ensures the selected concrete toolchain is installed, and then records that selector as the user's default. If the concrete toolchain is missing, `use` downloads/verifies/installs it before marking it active.

   `baml toolchain install <selector-or-version>` downloads/verifies/installs the selected concrete toolchain but does not change the user's active default selector. It is for prefetching, CI image setup, and installing alternate versions side by side.

   `baml toolchain update` refreshes the currently active default selector when it is a channel (`canary` or `nightly`) and advances it to the latest concrete version. If the active default is an exact version, `update` should report that exact versions do not advance automatically and suggest `baml toolchain use canary` or `baml toolchain use nightly`.

   `baml toolchain list` is local-only by default. A separate explicit remote mode may be added, but plain `list` must not hit the network.

   Network behavior:

   - `use <channel>` may fetch channel metadata when the channel cache is missing or expired, then install the selected concrete toolchain if missing.
   - `use <exact-version>` fetches the immutable per-version manifest only if that version is not installed locally or its cached manifest is missing.
   - `install <channel>` always resolves the latest channel pointer and installs that concrete version, but does not make it active.
   - `install <exact-version>` fetches that immutable version manifest only when needed.
   - Normal pass-through commands never install or update toolchains.

   First-run package-manager caveat therefore uses one command:

   ```text
   baml toolchain use canary
   ```

   First-run no-toolchain error should also recommend `baml toolchain use canary` as the primary fix. This command both installs and selects the canary toolchain.

4. **Wrapper state schema is not concrete enough**  
   [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:322), [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:344)  
   The text refers to “locally recorded concrete version” for channels, but `config.toml` only shows `[default] channel = "canary"`. Where is `canary -> 0.11.0` stored? In manifest cache? In config? In a separate channel state file? This matters for offline behavior and atomic updates.

   **Answer:** Use separate files for user intent, wrapper-owned machine state, remote metadata cache, and installed toolchain inventory. Do not make the manifest cache authoritative for normal command resolution.

   Final layout:

   ```text
   ~/.baml/
     config.toml              # user-authored/user-editable intent
     state.toml               # wrapper-owned active channel resolutions
     manifest-cache/          # fetched remote metadata, cache only
       canary.json
       nightly.json
       version/
         0.11.0.json
     toolchains/
       0.11.0/
         VERSION
         install.json
         bin/baml-cli
         bin/baml-pack-host
         assets/baml-vscode.vsix
   ```

   `config.toml` stores what the user wants:

   ```toml
   [default]
   selector = "canary" # "canary" | "nightly" | "<exact-version>"

   [update]
   auto_check = false
   ```

   `state.toml` stores the last successfully installed/activated concrete version for each channel:

   ```toml
   [channels.canary]
   active_version = "0.11.0"
   resolved_at = "2026-06-02T12:00:00Z"
   manifest_path = "manifest-cache/version/0.11.0.json"

   [channels.nightly]
   active_version = "0.11.1-nightly.20260602.a"
   resolved_at = "2026-06-02T12:30:00Z"
   manifest_path = "manifest-cache/version/0.11.1-nightly.20260602.a.json"
   ```

   `manifest-cache/` stores remote JSON plus fetch metadata where useful (`fetched_at`, `etag`, etc.). It is not the active-version authority. A remote/list operation may refresh `manifest-cache/canary.json` and discover a newer version, but normal commands must continue using `state.toml` until a toolchain-management command successfully installs and activates the newer concrete version.

   `toolchains/<version>/VERSION` contains the exact canonical version and is read as a tamper/sanity check on every wrapper invocation. `toolchains/<version>/install.json` should record install metadata such as source manifest URL, archive URL, archive sha256, installed_at, and target triple.

   Resolution invariants:

   - Normal pass-through commands never hit the network and never resolve directly from `manifest-cache`.
   - Exact-version selectors resolve directly to `toolchains/<version>/` and require that directory to exist and pass the `VERSION` sanity check.
   - Channel selectors (`canary`, `nightly`) resolve through `state.toml` to an installed concrete version.
   - If `config.toml` selects a channel but `state.toml` has no active version for that channel, normal commands fail with the primary fix `baml toolchain use <channel>`.
   - If `state.toml` points to a missing or corrupt toolchain directory, normal commands fail with a corrupt/missing state diagnostic and suggest `baml toolchain use <channel>` or `baml toolchain install <version> --force`.

   Mutation rules:

   - `baml toolchain use <channel>` fetches channel metadata if needed, installs the selected concrete version if missing, then atomically updates `state.toml` and records `[default].selector = "<channel>"` in `config.toml`.
   - `baml toolchain use <exact-version>` installs the version if missing, then records `[default].selector = "<exact-version>"` in `config.toml`. It does not update any channel entry in `state.toml`.
   - `baml toolchain install <channel>` fetches and installs the latest concrete version for the channel, but does not update `[default].selector` and does not update that channel's `active_version` in `state.toml`.
   - `baml toolchain install <exact-version>` installs only that version and does not mutate `config.toml` or `state.toml`.
   - `baml toolchain update` only advances `state.toml` when the active default selector is a channel. It installs the latest concrete version for that channel, then atomically swaps the channel's `active_version`. If the active selector is an exact version, it reports that exact versions do not advance automatically.

   Atomicity rule: write `state.toml` and `config.toml` through temp files in the same directory, validate the serialized TOML, fsync where practical, then rename. Only update state after the toolchain archive is downloaded, verified, extracted, `VERSION`-checked, and fully materialized under `toolchains/<version>/`.

   This gives the wrapper a clean invariant:

   ```text
   user intent (config.toml)
     + active local channel state (state.toml)
     + installed toolchain metadata (toolchains/<v>/VERSION)
   = normal command resolution
   ```

   Remote manifests are only inputs to explicit toolchain-management commands.

5. **Dry-run release testing lacks a wrapper override mechanism**  
   [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:1156), [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:1172)  
   The doc says dry-run may write to a `dryrun/` prefix and merge criteria include `baml toolchain install <v>` against a dry-run manifest. But the wrapper only knows `https://pkg.boundaryml.com/manifest/v1/...`. We need an explicit env var or flag like `BAML_MANIFEST_BASE_URL` for tests and mirrors.

   **Answer:** A dry run means running the release graph as realistically as possible without publishing to production release paths consumed by users. Production users read:

   ```text
   https://pkg.boundaryml.com/manifest/v1/canary.json
   https://pkg.boundaryml.com/manifest/v1/nightly.json
   https://pkg.boundaryml.com/manifest/v1/version/<version>.json
   ```

   Dry-run releases publish to an isolated namespace instead, for example:

   ```text
   https://pkg.boundaryml.com/dryrun/<github-run-id>/manifest/v1/nightly.json
   https://pkg.boundaryml.com/dryrun/<github-run-id>/manifest/v1/version/<version>.json
   https://pkg.boundaryml.com/dryrun/<github-run-id>/artifacts/<archive>
   ```

   The wrapper therefore needs a manifest-base override so validation can point at the dry-run namespace:

   ```text
   BAML_MANIFEST_BASE_URL=https://pkg.boundaryml.com/dryrun/<github-run-id>/manifest/v1
   ```

   Default remains:

   ```text
   https://pkg.boundaryml.com/manifest/v1
   ```

   Maintainer workflow:

   - A maintainer runs `.github/workflows/release-baml-language.yml` manually with `workflow_dispatch`.
   - Inputs include at least `channel` and `dry_run`.

     ```text
     channel: nightly | canary
     dry_run: true
     ```

   - Equivalent `gh` command:

     ```bash
     gh workflow run release-baml-language.yml \
       -f channel=nightly \
       -f dry_run=true
     ```

   - The workflow builds, packages, and smoke-tests exactly like a production release.
   - In dry-run mode, publish jobs do not write production channel pointers and do not create production user-visible release state.
   - Dry-run manifests and artifacts are uploaded under `pkg.boundaryml.com/dryrun/<github-run-id>/...`, or kept as workflow artifacts when public HTTP access is not required. If wrapper install validation is required, the dry-run manifest must point at HTTP-accessible dry-run artifact URLs.
   - The workflow summary prints the dry-run manifest base URL and example validation command.

   Validation command shape:

   ```bash
   BAML_HOME="$(mktemp -d)" \
   BAML_MANIFEST_BASE_URL="https://pkg.boundaryml.com/dryrun/<github-run-id>/manifest/v1" \
   baml toolchain use nightly
   ```

   Use a temporary `BAML_HOME` for dry-run validation so production user/developer state is not polluted.

   Wrapper override rules:

   - `BAML_MANIFEST_BASE_URL` is required for dry-run wrapper validation and useful for mirrors/internal test fixtures.
   - An optional explicit CLI flag may also be added for toolchain-management commands:

     ```text
     baml toolchain install <selector> --manifest-base-url <url>
     baml toolchain use <selector> --manifest-base-url <url>
     baml toolchain update --manifest-base-url <url>
     ```

   - Precedence is: `--manifest-base-url` -> `BAML_MANIFEST_BASE_URL` -> production default.
   - The override applies only to wrapper/toolchain manifest reads. Normal pass-through commands still do not hit the network.
   - The override is never persisted into `config.toml`.
   - Cache entries are namespaced by manifest base URL so dry-run manifests cannot poison the production cache. Production may use `manifest-cache/prod/`; overrides use `manifest-cache/override/<hash-of-base-url>/`.
   - Channel state written under an override records the manifest base URL/hash in `state.toml`. A later command using a different manifest base must not silently treat that channel state as valid; it should either use a temporary `BAML_HOME` in CI or print a clear diagnostic telling the user to run `baml toolchain use <channel>` under the current source.

6. **Release concurrency is not specified**  
   [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:178), [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:529)  
   Nightly suffix selection reads existing GitHub releases and picks the next letter. Without a workflow concurrency group or locking rule, two successful `canary` runs close together can compute the same letter. The PyPI collision check catches some fallout, but the release graph should prevent the race.

   **Answer:** GitHub merge queue is the first line of defense: all normal writes to `canary` should go through the queue, and the release workflow should publish only from actual `push` events / successful CI on `refs/heads/canary`, not from PR or `merge_group` branches. However, merge queue is not the release lock. Manual dispatches, retries, admin bypasses, and slow publish jobs can still overlap.

   Add a non-cancelling GitHub Actions concurrency group to the release graph entrypoint:

   ```yaml
   concurrency:
     group: baml-language-release-canary
     cancel-in-progress: false
   ```

   This serializes all production BAML language release runs. Do not include the version, channel, run id, or commit SHA in this group; those would allow overlapping runs and defeat the lock. `cancel-in-progress: false` is required so GitHub queues later releases instead of cancelling a run that may already be halfway through publishing.

   Manual `workflow_dispatch` production publishes use the same concurrency group. Dry-run releases may either use the same group for maximum simplicity or a separate dry-run group if we want dry-run validation not to block production:

   ```yaml
   concurrency:
     group: ${{ inputs.dry_run == 'true' && 'baml-language-release-dryrun' || 'baml-language-release-canary' }}
     cancel-in-progress: false
   ```

   Production publish jobs must still perform idempotency/uniqueness checks after acquiring the concurrency slot:

   - Recompute/read the frozen `release-plan.json`.
   - Check whether `baml-language-<version>` already exists.
   - If the tag/release exists and all expected artifacts/manifests match, treat rerun as success/idempotent repair.
   - If the tag/release exists but content differs, hard-fail.
   - If publishing nightly, choose the nightly letter inside the serialized release run, after the concurrency slot is acquired.

   With this rule, merge queue serializes branch state, and the release workflow serializes publishing state.

7. **Rollback is too optimistic**  
   [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:1190)  
   “Rollback is a code rollback” restores workflow files, but it does not address already-published mutable pointers like `canary.json`, `nightly.json`, `wrapper.json`, install scripts, or a bad GitHub release. The rollback playbook needs explicit behavior for channel pointers and whether to advance to a fixed version, repoint, or leave immutable versions alone.

   **Answer:** Rollback is code revert plus mutable pointer repair. `git revert <merge-commit>` restores workflow files, but it does not undo external release state. The rollback playbook must distinguish immutable release artifacts from mutable pointers.

   Immutable artifacts are never overwritten or deleted during normal rollback:

   ```text
   manifest/v1/version/<version>.json
   GitHub release baml-language-<version>
   GitHub release baml-wrapper-<version>
   PyPI baml_core <version>
   ```

   If a bad version was published, leave the immutable record intact and move the channel pointer away from it. This preserves reproducibility and avoids users seeing disappearing versions.

   Mutable pointers may be repaired:

   ```text
   manifest/v1/canary.json
   manifest/v1/nightly.json
   manifest/v1/wrapper.json
   install.sh
   install.ps1
   index.html
   Homebrew formula / AUR metadata, for wrapper releases
   ```

   Channel pointer repair flow:

   1. Choose the last known good concrete version.
   2. Download `manifest/v1/version/<version>.json`.
   3. Validate schema and target completeness.
   4. Upload that JSON as `manifest/v1/canary.json` or `manifest/v1/nightly.json`.
   5. Use mutable cache headers:

      ```text
      Cache-Control: public, max-age=60, must-revalidate
      ```

   Wrapper rollback is separate. If a bad wrapper release was published, repair `manifest/v1/wrapper.json` and any package-manager definitions that point to it. Do not delete the bad `baml-wrapper-<version>` GitHub release. Prefer publishing a newer fixed wrapper when package-manager version ordering makes repointing awkward.

   PyPI rollback is fix-forward. Do not try to delete/reuse a published `baml_core` version. Move channel pointers away from the bad release when possible and publish the next nightly/canary with a fix.

   Add an explicit rollback script or manual workflow, for example:

   ```bash
   scripts/baml-release-rollback point-channel \
     --channel nightly \
     --version 0.11.1-nightly.20260601.b

   scripts/baml-release-rollback point-channel \
     --channel canary \
     --version 0.11.0

   scripts/baml-release-rollback point-wrapper \
     --version 0.1.0
   ```

   The rollback script must not publish new immutable version artifacts. It only validates existing immutable manifests/releases and repairs mutable pointers.

   If the release graph itself is broken:

   1. Stop or avoid further production publishes.
   2. Repair user-facing mutable pointers if users are affected.
   3. Revert the merge commit.
   4. Confirm old workflow files are restored.
   5. Run one controlled release or dry run if needed.

   Maintainers have these rollback tools available:

   - `git revert <merge-commit>` for repo/workflow rollback.
   - `scripts/baml-release-rollback point-channel` for `canary.json` / `nightly.json`.
   - `scripts/baml-release-rollback point-wrapper` for `wrapper.json`.
   - Fix-forward release for immutable package mistakes.
   - Short cache TTL on mutable pointers so pointer repair reaches wrappers quickly.

**Medium Priority**
8. **Wrapper release version source is vague**  
   [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:83), [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:963)  
   It mentions wrapper crate version / `BAML_WRAPPER_VERSION` and a wrapper-version-changed check, but not the authoritative file or comparison rule. Is it `baml_language/crates/baml/Cargo.toml`, a constant, or workflow metadata?

   **Answer:** The authoritative wrapper version is the literal package version in `baml_language/crates/baml/Cargo.toml`, once the wrapper crate exists. Remove `BAML_WRAPPER_VERSION` from the plan; do not introduce an environment-variable version authority for the wrapper.

   Future wrapper crate shape:

   ```toml
   # baml_language/crates/baml/Cargo.toml
   [package]
   name = "baml"
   version = "0.1.0"
   publish = false
   ```

   The wrapper crate must not inherit the workspace version:

   ```toml
   # Do not do this for the wrapper:
   version.workspace = true
   ```

   Reason: `baml_language/Cargo.toml` workspace `version = "0.0.0-beta"` is intentionally not the public BAML language/toolchain version and is also not the wrapper version. The wrapper is the one Rust package in this plan whose `CARGO_PKG_VERSION` is intentionally meaningful as a public product version.

   `baml --version` may use `env!("CARGO_PKG_VERSION")` because the wrapper's Cargo package version is the wrapper product version. This is different from `baml-cli`, LSP, SDK runtimes, and generated-code surfaces, which must use the stamped BAML language version instead.

   Release graph rule:

   ```text
   wrapper_version = parse baml_language/crates/baml/Cargo.toml [package].version
   latest_wrapper_version = fetch manifest/v1/wrapper.json version if it exists
   wrapper_changed = wrapper_version != latest_wrapper_version
   ```

   If `wrapper_changed` is true, publish the wrapper release:

   ```text
   GitHub release baml-wrapper-<wrapper_version>
   wrapper archives
   manifest/v1/wrapper.json
   Homebrew formula
   AUR packages
   install.sh / install.ps1 only if changed
   ```

   If `wrapper_changed` is false, build and smoke-test the wrapper in the release graph, but do not publish wrapper artifacts or package-manager updates.

   Required checks:

   - Fail if `baml_language/crates/baml/Cargo.toml` uses `version.workspace = true`.
   - Fail if the wrapper version is not valid SemVer.
   - Fail if the wrapper version is lower than the latest published `manifest/v1/wrapper.json` version.
   - Fail if any workflow step tries to set or consume `BAML_WRAPPER_VERSION`.
   - Fail if `baml --version` does not print exactly `baml <wrapper_version>` in smoke tests.
   - Keep `BAML_RELEASE_VERSION` removed for the language/toolchain path; do not replace it with a wrapper-specific env var.

   Bump UX:

   - V1 may document a manual edit to `baml_language/crates/baml/Cargo.toml [package].version`.
   - Prefer adding a helper so maintainers do not hand-edit SemVer incorrectly:

     ```bash
     scripts/baml-wrapper-version bump --patch
     scripts/baml-wrapper-version bump --minor
     scripts/baml-wrapper-version check
     ```

   - The helper edits only `baml_language/crates/baml/Cargo.toml` and runs the wrapper-version checks above. It does not touch `baml_language/release.toml`, `baml_version`, `baml_core`, VSIX version, SDK versions, or any BAML language/toolchain version surface.

   Final invariant:

   ```text
   Wrapper version source: baml_language/crates/baml/Cargo.toml [package].version
   Language/toolchain version source: baml_language/release.toml + release-plan.json stamping
   ```

9. **Direct `baml-cli` warning detection needs a mechanism**  
   [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:228)  
   It says `baml-cli` should warn when invoked outside the wrapper. How does it know? The wrapper likely needs to set an env var such as `BAML_WRAPPER_EXEC=1`, and direct invocation warns when absent.

   **Answer:** The wrapper sets an advisory environment variable before execing the selected internal toolchain binary:

   ```text
   BAML_WRAPPER_EXEC=1
   ```

   Wrapper pass-through behavior:

   ```text
   baml generate
   ```

   becomes approximately:

   ```text
   BAML_WRAPPER_EXEC=1 \
   BAML_WRAPPER_VERSION=<wrapper-version> \
   BAML_WRAPPER_RESOLVED_TOOLCHAIN=<canonical-toolchain-version> \
   ~/.baml/toolchains/<version>/bin/baml-cli generate
   ```

   Only `BAML_WRAPPER_EXEC=1` is required for warning suppression. `BAML_WRAPPER_VERSION` and `BAML_WRAPPER_RESOLVED_TOOLCHAIN` are optional diagnostic/logging aids and must not become version authorities.

   `baml-cli` startup behavior:

   - If `BAML_WRAPPER_EXEC=1`, do not warn.
   - If `BAML_CLI_ALLOW_DIRECT=1`, do not warn. This escape hatch is for tests, packaging smoke tests, and maintainers debugging the internal binary directly.
   - Otherwise, print a once-per-process warning to stderr:

     ```text
     warning: using the internal BAML toolchain binary directly is not recommended. Use `baml` instead.
     ```

   Use stderr, not stdout. stdout may be machine-readable for commands like `--version`, `describe`, future JSON output, or scripts. With stderr, `baml-cli --version` stdout remains exact.

   This is advisory, not security. A user can set `BAML_WRAPPER_EXEC=1`; that is acceptable because the goal is to guide users toward the wrapper, not enforce a trust boundary.

   Direct project/toolchain mismatch detection is separate and best-effort. If `baml-cli` is invoked directly inside a project whose nearest `baml.toml [toolchain]` selects a different concrete version/channel than the binary's own `baml_version::CANONICAL_VERSION`, it may print an additional local warning:

   ```text
   warning: this project selects BAML 0.11.0, but this internal toolchain is 0.11.1-nightly.20260602.a. Run commands through `baml` so the wrapper can select the right toolchain.
   ```

   That mismatch check must not hit the network, read wrapper `state.toml`, install toolchains, or attempt to repair anything. It only reads local project metadata and compares against the current binary's stamped toolchain version.

   Implementation locations:

   - Wrapper: in the pass-through exec path in the new `baml_language/crates/baml/` crate, immediately before replacing the process with the selected `baml-cli`.
   - CLI: early in the `baml-cli` entrypoint, before command dispatch, likely in `baml_language/crates/baml_cli/src/main.rs` or the equivalent top-level command bootstrap.
   - Version comparison: use the stamped BAML language version (`baml_version::CANONICAL_VERSION`), not `CARGO_PKG_VERSION`.

   Tests:

   - Direct `baml-cli --version` without `BAML_WRAPPER_EXEC` prints the warning to stderr and exact version output to stdout.
   - Direct `baml-cli --version` with `BAML_WRAPPER_EXEC=1` prints no warning.
   - Direct `baml-cli --version` with `BAML_CLI_ALLOW_DIRECT=1` prints no warning.
   - Wrapper pass-through command sets `BAML_WRAPPER_EXEC=1`, so invoking `baml --version` or `baml generate` through the wrapper does not show the direct-binary warning.
   - Machine-readable stdout tests assert the warning never appears on stdout.
   - Optional mismatch test: create a temporary project with `baml.toml [toolchain]` selecting a different version and invoke `baml-cli` directly; assert the mismatch warning appears without any network access.

10. **Installer path/profile behavior is too light**  
   [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:491)  
   Flags are listed, but not which shell profiles are edited, how PATH modification is skipped/detected, whether writes are atomic, how Windows PATH mutation works, or what exit codes mean. This is less architectural, but installers are sharp edges.

   **Answer:** Curl/PowerShell installers are user-scoped BAML installers. They may install/update the wrapper under `BAML_HOME`, optionally bootstrap a requested toolchain, and optionally add `BAML_HOME/bin` to the user's PATH. They must not use `sudo`, write system directories, mutate Homebrew/AUR/package-manager paths, install IDE extensions automatically, or write outside `BAML_HOME` except for user profile/PATH configuration.

   Defaults:

   ```text
   Unix BAML_HOME:    $HOME/.baml
   Windows BAML_HOME: %USERPROFILE%\.baml
   Unix wrapper:      $BAML_HOME/bin/baml
   Windows wrapper:   %USERPROFILE%\.baml\bin\baml.exe
   ```

   Required flags:

   ```text
   --channel <canary|nightly>   # default canary
   --version <version>          # exact version; wins over --channel
   --wrapper-only               # install/update wrapper but skip toolchain bootstrap
   --no-modify-path             # do not edit shell profile / user PATH
   --yes                        # disable prompts / accept defaults; explicit consent for profile/PATH edits in piped/non-interactive installs
   --help
   ```

   Flag rules:

   - `--version` wins over `--channel`.
   - `--wrapper-only` skips toolchain bootstrap.
   - `--no-modify-path` disables profile/PATH edits even when `--yes` is supplied.
   - `--yes` disables prompts, accepts defaults, and is the explicit consent signal for profile/PATH edits in piped or otherwise non-interactive installs.
   - Do not add `--modify-path` in v1. If PATH modification is the default in interactive installs, the only necessary opposite flag is `--no-modify-path`.
   - Piped installs such as `curl ... | sh -s` are treated as non-prompting because stdin is the script stream. They install wrapper + default canary toolchain, do not edit shell profiles by default, and print PATH instructions.

   Unix `install.sh` flow:

   1. Resolve `BAML_HOME`.
   2. Detect platform/target triple.
   3. Fetch `manifest/v1/wrapper.json`.
   4. Download wrapper archive to a temp directory.
   5. Verify sha256 before extraction/execution.
   6. Extract into a temp directory.
   7. Validate archive layout contains the expected wrapper executable at `bin/baml` and does not contain `baml-cli`, `baml-pack-host`, VSIX, absolute paths, `..` paths, or unsafe symlinks.
   8. `chmod +x` the wrapper.
   9. Atomically replace `$BAML_HOME/bin/baml` (write/extract to temp path in same directory, fsync where practical, rename into place).
   10. Optionally update PATH.
   11. Unless `--wrapper-only`, bootstrap the requested toolchain with:

       ```bash
       "$BAML_HOME/bin/baml" toolchain use <canary|nightly|version>
       ```

       Do not run both `toolchain install` and `toolchain use`; point 3 defines `use` as "install if missing, then select".

   PATH behavior on Unix:

   - Installer writes a generated env file at:

     ```text
     $BAML_HOME/env
     ```

   - File contents:

     ```sh
     export BAML_HOME="$HOME/.baml"
     case ":$PATH:" in
       *":$BAML_HOME/bin:"*) ;;
       *) export PATH="$BAML_HOME/bin:$PATH" ;;
     esac
     ```

   - Shell profile files only source that env file:

     ```sh
     . "$HOME/.baml/env"
     ```

   - Profile edits are idempotent: if the exact source line already exists, do not add another.
   - If `BAML_HOME/bin` is already on PATH, do not modify profile files unless `--yes` is explicitly supplied and the source line is missing.
   - Interactive install: update PATH by default unless `--no-modify-path`.
   - Piped/non-interactive install without `--yes`: do not edit profile files; print manual PATH instructions.
   - Piped/non-interactive install with `--yes`: edit profile files by default unless `--no-modify-path` is also supplied.
   - CI/Docker examples should use `--no-modify-path` and set PATH explicitly.

   Explicit piped install behavior:

   ```bash
   curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s
   ```

   This installs/updates the wrapper, bootstraps the default canary toolchain with `baml toolchain use canary`, does not edit shell profiles, and prints:

   ```text
   BAML installed at ~/.baml/bin/baml

   Add BAML to your PATH by adding this to your shell profile:

     . "$HOME/.baml/env"

   Or run for this shell session:

     export PATH="$HOME/.baml/bin:$PATH"
   ```

   ```bash
   curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s -- --yes
   ```

   This installs/updates the wrapper, bootstraps the default canary toolchain, and edits shell profile/PATH configuration by default.

   ```bash
   curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s -- --yes --no-modify-path
   ```

   This accepts defaults but still skips profile/PATH edits.

   Unix shell targets:

   - `zsh`: add source line to `~/.zshrc`; on macOS, also add to `~/.zprofile` if that file already exists.
   - `bash`: add source line to `~/.bashrc`; on macOS, also add to `~/.bash_profile` if that file already exists.
   - `fish`: write `~/.config/fish/conf.d/baml.fish` instead of editing generic shell files. The fish file should set `BAML_HOME` and prepend `$BAML_HOME/bin` only if missing.
   - Unknown shell: do not edit profiles; print manual PATH instructions.

   Windows `install.ps1` flow:

   1. Resolve `BAML_HOME` from env or default `%USERPROFILE%\.baml`.
   2. Detect architecture.
   3. Fetch `manifest/v1/wrapper.json`.
   4. Download wrapper archive to a temp directory.
   5. Verify sha256 before extraction/execution.
   6. Extract and validate archive layout contains `bin/baml.exe` only for the wrapper payload.
   7. Replace `%USERPROFILE%\.baml\bin\baml.exe` safely. If the executable is in use, use a safe replace/deferred replace path and print the action taken.
   8. Optionally update the user's PATH.
   9. Unless `--WrapperOnly`, bootstrap with:

      ```powershell
      & "$env:BAML_HOME\bin\baml.exe" toolchain use <canary|nightly|version>
      ```

   Windows PATH behavior:

   - Update user PATH only, never machine PATH.
   - Do not require Administrator.
   - Use `[Environment]::SetEnvironmentVariable("Path", ..., "User")`.
   - Avoid duplicate entries.
   - If PATH changes, tell the user to restart the terminal.
   - `--NoModifyPath` skips PATH mutation.
   - Non-interactive mode does not mutate user PATH unless `--Yes` is passed.

   Stable exit codes:

   ```text
   0  success
   1  general failure
   2  unsupported platform
   3  download/network failure
   4  checksum verification failure
   5  archive validation/extraction failure
   6  PATH/profile update failure
   7  toolchain bootstrap failure
   ```

   Failure behavior:

   - If wrapper install succeeds but PATH update fails, leave the wrapper installed, print manual PATH instructions, and exit `6`.
   - If wrapper install succeeds but toolchain bootstrap fails, leave the wrapper installed, print the exact `baml toolchain use <selector>` command to retry, and exit `7`.
   - If checksum or archive validation fails, do not replace the existing wrapper.
   - Re-running the installer is idempotent: it refreshes the wrapper from `wrapper.json`, repairs PATH/env file entries if requested, and only bootstraps the requested toolchain when `--wrapper-only` is not passed.

11. **`baml ide install` fallback is risky as stated**  
   [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:970)  
   “Fall back to dropping into the editor’s extensions directory and unzipping” needs exact per-editor directories, extension IDs, version replacement behavior, and whether that is actually supported. I’d either specify it tightly or remove fallback in v1.

   **Answer:** Remove the manual extension-directory unzip fallback from v1. `baml ide install` installs only through supported editor CLIs. If no supported editor CLI is available, print the VSIX path and the exact manual command the user can run.

   V1 supports only Cursor and VS Code out of the box:

   ```text
   baml ide install
   baml ide install --cursor
   baml ide install --code
   ```

   No v1 flags for `--windsurf`, `--all`, `--editor`, `--force`, or `--dry-run`.

   Behavior:

   - Resolve the active selected toolchain.
   - Find the VSIX at:

     ```text
     <toolchain_root>/assets/baml-vscode.vsix
     ```

   - `--cursor`: run `cursor --install-extension <vsix>`.
   - `--code`: run `code --install-extension <vsix>`.
   - No flag:
     - if only `cursor` is found on PATH, install to Cursor;
     - if only `code` is found on PATH, install to VS Code;
     - if both are found and the terminal is interactive, prompt the user to choose;
     - if both are found and the terminal is non-interactive, error and require `--cursor` or `--code`;
     - if neither is found, error with manual install commands.

   Implementation may internally pass editor-specific flags needed for update/reinstall, such as `--force` if the editor CLI supports it, but those are not user-facing BAML flags in v1.

   Failure message shape:

   ```text
   No supported editor CLI found.

   Install manually:
     code --install-extension <toolchain_root>/assets/baml-vscode.vsix

   Or choose an editor if its CLI is installed:
     baml ide install --cursor
     baml ide install --code
   ```

   Implementation locations:

   - Add the command variant in `baml_language/crates/baml_cli/src/commands.rs`.
   - Implement behavior in `baml_language/crates/baml_cli/src/ide_command.rs`.
   - The wrapper does not own this command. `baml ide install --cursor` is wrapper pass-through to the selected `baml-cli ide install --cursor`.

   Tests:

   - Finds VSIX relative to active toolchain root.
   - `--cursor` invokes `cursor --install-extension <vsix>`.
   - `--code` invokes `code --install-extension <vsix>`.
   - No flag + only Cursor detected installs to Cursor.
   - No flag + only VS Code detected installs to VS Code.
   - No flag + both detected + non-interactive errors and requires a flag.
   - No supported CLI detected prints manual install commands and VSIX path.
   - No test or implementation path manually unzips into editor extension directories.

12. **Homebrew/AUR publishing details are abstract**  
   [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:168), [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:936)  
   The desired package shape is clear, but credentials, target repos, dispatch vs direct commit, AUR update mechanism, and source/archive URL conventions are not as concrete as the GitHub/S3/PyPI parts.

   **Answer:** Homebrew and AUR publishing are wrapper-release-only jobs. They never run for `baml-toolchain` nightly/canary releases, never track BAML language versions, never bootstrap toolchains during package install/upgrade, and never write user-scoped `~/.baml` state.

   Homebrew:

   - Repository: `BoundaryML/homebrew-tap`.
   - Formula path: `Formula/baml.rb`.
   - Package contents: installs only `bin/baml`.
   - Formula version: wrapper version from `baml_language/crates/baml/Cargo.toml [package].version`.
   - Formula source/archive URL:

     ```text
     https://github.com/BoundaryML/baml/releases/download/baml-wrapper-<version>/baml-wrapper-<version>-<target>.tar.gz
     ```

   - Formula must not install, symlink, or reference `baml-cli`, `baml-pack-host`, or the VSIX.
   - Formula must not run `baml toolchain install`, `baml toolchain use`, or any toolchain bootstrap in `post_install`.
   - Publish method: release graph commits directly to `BoundaryML/homebrew-tap` using the existing `HOMEBREW_BAML_DISPATCH_TOKEN`, kept for compatibility with the current secret naming. The token needs contents write access to `BoundaryML/homebrew-tap`.
   - Caveats should print:

     ```text
     BAML wrapper installed.

     To install and select the current canary language toolchain:
       baml toolchain use canary

     To use nightly:
       baml toolchain use nightly

     IDE extension setup is explicit:
       baml ide install --cursor
     ```

   AUR:

   - Packages: `baml` and `baml-bin`.
   - `baml-bin` downloads the prebuilt `baml-wrapper-<version>-<target>.tar.gz` wrapper archive from GitHub Releases and installs only `bin/baml`.
   - `baml` source-builds the wrapper from the `baml-wrapper-<version>` source archive/tag, builds only `--bin baml`, and installs only `/usr/bin/baml`.
   - Both AUR package versions track the wrapper version only.
   - No AUR nightly/canary package stream in v1.
   - No AUR install hook writes `~/.baml` or runs toolchain install/use.
   - AUR install messages should point users to `baml toolchain use canary`.
   - Publish method: update the AUR package repositories over SSH, regenerate `.SRCINFO`, commit, and push.
   - Required maintainer-configured AUR remotes:

     ```text
     ssh://aur@aur.archlinux.org/baml.git
     ssh://aur@aur.archlinux.org/baml-bin.git
     ```

   - Required CI secret:

     ```text
     AUR_SSH_PRIVATE_KEY
     ```

   Publish guards for both Homebrew and AUR:

   ```text
   wrapper_changed == true
   dry_run == false
   github.ref == refs/heads/canary
   ```

   A `baml-toolchain` release must not dispatch or run Homebrew/AUR publishing. A wrapper release may occur in the same release graph codebase, but it is a separate publish decision keyed only off wrapper version changes.

   Dry-run behavior:

   - Generate the Homebrew formula and AUR `PKGBUILD` / `.SRCINFO` files.
   - Upload generated package files as workflow artifacts.
   - Do not commit to `BoundaryML/homebrew-tap`.
   - Do not push to AUR.

   Required checks before publishing:

   - Homebrew formula points at `baml-wrapper-<version>`, not `baml-language-<version>`.
   - Homebrew formula version equals wrapper version.
   - Homebrew formula installs only `bin/baml`.
   - Homebrew formula has no `post_install` toolchain bootstrap.
   - AUR `pkgver` equals wrapper version.
   - AUR package sources point at wrapper release/source.
   - AUR `PKGBUILD` installs only `baml`.
   - AUR install hooks do not run `baml toolchain use` or `baml toolchain install`.
   - Caveats/install messages include `baml toolchain use canary`.

13. **Manifest target completeness has a subtle policy gap**  
   [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:358)  
   It defines required and best-effort targets, but not whether the publish workflow may publish a manifest missing a Tier 1 target due to a failed matrix job. I think Tier 1 failures should block publish, while best-effort failures can be omitted only with an explicit gate.

   **Answer:** Do not support optional/best-effort targets in v1. If a target is in the supported release matrix, it is required. Any target build, archive-layout, checksum, or smoke failure blocks the entire release. Maintainers can rerun the failed matrix job or rerun the workflow to recover.

   Release policy:

   - The release matrix is the required target set.
   - A `baml-toolchain` release publishes only when every target in the matrix succeeds.
   - A `baml-wrapper` release publishes only when every wrapper target in the matrix succeeds.
   - There is no `optional_targets`, `best_effort_targets`, or `omitted_targets` concept in v1.
   - If a platform is too flaky to block releases, remove it from the supported release matrix rather than marking it optional.

   Manifest publish validation:

   - The manifest's `artifacts` target set must exactly equal the release matrix target set for that product.
   - Every artifact entry must include HTTPS `url` and lowercase 64-character hex `sha256`.
   - Every included archive must have passed archive-layout validation.
   - No publish job runs if any target is missing, failed, or lacks a checksum.
   - `manifest/v1/version/<version>.json` is not uploaded until the full target set is complete.
   - `canary.json` / `nightly.json` pointers are not updated until the immutable per-version manifest is complete.

   Wrapper behavior for a missing target is defensive only. It should be treated as an incomplete/corrupt release manifest, not an expected optional-target case:

   ```text
   error: BAML 0.11.0 manifest is missing artifact for target aarch64-pc-windows-msvc.
   This release manifest is incomplete. Try `baml toolchain update`, or report this release.
   ```

   This keeps the release model simple: if a BAML version exists, all supported targets for that release exist.

**Lower Priority**
14. **Docs phase is intentionally broad**  
   [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:1106)  
   “New page: install matrix...” is fine for a milestone, but not enough if this is meant to hand directly to a docs implementer. It needs page paths, audience, and required examples.

   **Answer:** These docs refer to new docs that must be created as part of the feature branch. There is no established public `baml_language` documentation home yet, so v1 should create in-repo implementation/user-flow docs under `TASK/docs/`. Public product-docs migration is a later product/docs decision and should not block the release-infra implementation.

   Create:

   ```text
   TASK/docs/install.md
   TASK/docs/release-maintainer.md
   TASK/docs/toolchain-system.md
   ```

   `TASK/docs/install.md`

   - Audience: users, support, and people testing the new installer.
   - Required contents:
     - `brew install boundaryml/tap/baml` installs the wrapper only.
     - Package-manager installs do not bootstrap toolchains.
     - `curl ... | sh -s` installs the wrapper and default canary toolchain.
     - Docker/CI install example using `--no-modify-path`.
     - PATH behavior and `~/.baml/env`.
     - `baml toolchain use canary`.
     - `baml toolchain use nightly`.
     - `baml toolchain install <version>`.
     - `baml toolchain update`.
     - `baml self-update`.
     - Package-manager wrapper upgrade behavior (`brew upgrade baml`, AUR upgrade).
     - IDE setup:

       ```bash
       baml ide install --cursor
       baml ide install --code
       ```

   `TASK/docs/release-maintainer.md`

   - Audience: maintainers and implementation agents.
   - Required contents:
     - Release graph overview.
     - Canary vs nightly model.
     - How to bump canary.
     - How nightly suffixes are chosen.
     - Release workflow concurrency rule.
     - Dry-run release workflow and `BAML_MANIFEST_BASE_URL`.
     - Production publish guards.
     - PyPI trusted-publisher portal pause.
     - Homebrew/AUR wrapper release flow.
     - Rerun/idempotency behavior.
     - Rollback / mutable pointer repair flow.

   `TASK/docs/toolchain-system.md`

   - Audience: engineers.
   - Required contents:
     - Wrapper vs toolchain product boundary.
     - `~/.baml` layout.
     - `config.toml` user intent.
     - `state.toml` active channel resolutions.
     - manifest cache role.
     - manifest schema.
     - network/cache policy.
     - `toolchain use` vs `toolchain install` semantics.
     - VSIX/LSP/playground compatibility protocol.
     - one-LSP-per-BAML-project-root model.
     - direct `baml-cli` warning behavior.
     - `baml pack` fetcher unification.

   These docs are part of the handoff and release-infra branch. They can be moved or rewritten for the public docs site later.

15. **Marketplace/deprecation story is deferred but product-impacting**  
   [TASK.md](/Users/rossir/dev/baml-canary/TASK/TASK.md:1136)  
   It is correctly marked as product decision, but if implementation depends on extension ID, Marketplace publisher, or coexistence behavior, that decision may need to move earlier.

   **Answer:** Marketplace publishing and old-extension deprecation remain deferred to Phase 4, but the new VSIX identity is not deferred. V1 ships a stable, distinct, toolchain-bundled VSIX installed explicitly by `baml ide install`; it is not published to the VS Code Marketplace in v1.

   Stable new VSIX identity:

   ```json
   {
     "publisher": "Boundary",
     "name": "baml-language",
     "displayName": "BAML",
     "description": "Language support and playground for BAML."
   }
   ```

   Stable extension ID:

   ```text
   Boundary.baml-language
   ```

   The user-visible display name is simply `BAML`. The internal `name` is `baml-language` to avoid ambiguity with the existing Marketplace extension and to keep a distinct stable ID for the toolchain-bundled extension.

   V1 coexistence behavior:

   - The old Marketplace extension remains untouched.
   - The new extension has a distinct extension ID.
   - `baml ide install --cursor` and `baml ide install --code` install the new toolchain-bundled VSIX only.
   - No old-extension detection.
   - No warning if the old extension is also installed.
   - No automatic disable/uninstall/migration.
   - If a user has both extensions installed and sees duplicate behavior, docs/support guidance is to disable one manually.

   Deferred to Phase 4:

   - Publishing the new extension to the Marketplace.
   - Deprecating or replacing the old Marketplace extension.
   - Marketplace migration messaging.
   - Any old-extension detection, warning, disable, uninstall, or migration behavior.
