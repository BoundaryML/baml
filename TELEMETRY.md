# Telemetry

The BAML CLI and toolchain wrappers collect anonymous telemetry about general usage. Participation in this usage-analytics program is optional, and you may opt out if you would not like to share it.

The usage-analytics description and assurances below apply to CLI/wrapper usage events. BCS runtime recording is a separate path, including project/SDK recording and recording enabled by release builds of the BAML-native wrapper. It can contain caller-provided trace and context data. Its automatic environment metadata, including the explicit `AI_AGENT` property and its collection limits, is described in [BCS environment metadata](#bcs-environment-metadata).

## Why is telemetry collected?

BAML is a small team building a fast-moving developer tool. Prior to
telemetry collection, making decisions about how to improve BAML was a
very manual process.

We actively engage with the community — Discord, GitHub issues, direct
conversations — to gather feedback. However, that approach only lets us
collect feedback from a self-selected subset of users. That subset may
have different needs and use cases than you.

Telemetry lets us accurately gauge BAML feature usage, pain points, and
customization across everyone who uses the CLI, not just the people with
time to write us. This data lets us better tailor BAML to the masses,
ensuring its continued growth, relevance, and best-in-class developer
experience. It also lets us verify whether improvements we ship are
actually improving the baseline of all users' workflows.

## What is being collected?

We track general usage information. Specifically, we track the following
anonymously:

- Command invoked (`baml test`, `baml check`, `baml fmt`, …). Not the arguments.
- Version of BAML and release channel.
- General machine information (operating system, CPU architecture, CPU
  count, whether the command was run inside CI, Docker, or WSL).
- A random per-machine anonymous ID, and a salted, one-way hash of the
  project root. The salt never leaves your machine, so the hash isn't
  reversible.

This list is regularly audited to ensure its accuracy.

## What about sensitive data (e.g. secrets)?

The anonymous CLI/wrapper usage analytics described above selects coarse usage and environment fields instead of taking a general environment snapshot. It does not intentionally read credential variables or record command arguments, file paths, source contents, prompts, model responses, API keys, logs, or serialized errors.

These usage events use a random anonymous identity and salted project hash for aggregate product analysis. This usage-analytics policy does not describe BCS recordings, including those enabled by release builds of the BAML-native wrapper: their trace/context values and explicit `AI_AGENT` name can contain caller-provided data. In particular, a bounded `AI_AGENT` value can be transmitted even if a path or credential is placed there; the bounds are not secret redaction. See [BCS environment metadata](#bcs-environment-metadata) for that separate policy.

## Will this data be shared?

We use telemetry data internally to help us improve the product and will
only share de-identified data in the aggregate for business purposes such
as marketing.

## How do I opt out?

The CLI commands and `DO_NOT_TRACK` control in this section disable CLI/wrapper usage analytics only. Source-based CLI runs and the released BAML-native wrapper honor `BAML_TELEMETRY=off` for runtime recording. Generated SDK bridges and packed binaries honor it only when the publisher preserves the standard recording-level binding: a renamed binding requires setting that named variable to `off`, while an omitted or `false` binding retains the publisher's fixed recording level and ignores `BAML_TELEMETRY`. See the [artifact binding policy](baml_language/crates/bcs_api/README.md#embedded-telemetry-in-artifacts) and [BCS environment metadata](#bcs-environment-metadata) for the applicable configuration and limits.

You may opt out by running `baml telemetry disable`:

```bash
baml telemetry disable
```

You may check the status of telemetry collection at any time by running
`baml telemetry`:

```bash
baml telemetry
```

You may re-enable telemetry if you'd like to re-join the program by
running:

```bash
baml telemetry enable
```

You may also opt out by setting the cross-tool environment variable
`DO_NOT_TRACK=1`.

## Wrapper telemetry

The Rust and BAML toolchain wrappers share an independent PostHog event schema. Each invocation emits one canonical command event, such as `baml`, `baml check`, `baml toolchain status`, `baml toolchain update`, or `baml self-update`, with `event_kind = "invocation"`. All flags and argument values are discarded: `baml --help` becomes `baml`, and `baml toolchain update --help` becomes `baml toolchain update`. Only recognized command names enter telemetry; unknown commands collapse to the nearest recognized parent. Delegated CLI commands use their recognized top-level name, without inspecting the CLI's nested arguments. Internal telemetry and replacement helpers are excluded.

Every wrapper event includes `schema_version = 2`, its canonical `command`, and a `client` discriminator: `rust-wrapper` or `baml-in-baml`. Both implementations also emit `wrapper_resolution`, `wrapper_install`, `wrapper_selection`, `wrapper_update`, and `wrapper_self_update` for bounded decisions and outcomes. `wrapper_unhandled_error` records errors escaping the wrapper command handler, including argument parsing and failed handoffs. Recovered errors and delegated CLI nonzero exit codes do not count as unhandled wrapper errors. A delegated CLI keeps its separate telemetry.

Events include coarse OS/architecture/CPU/CI information, a random anonymous identity and session, and a salted project-root hash. Source categories, toolchain kinds, and outcomes are bounded labels; argument values, selectors, raw paths, panic payloads, and error text are not sent.

Both wrappers send to the dedicated wrapper PostHog project, 698093. The Rust wrapper owns the public ingestion key and host constants, and the BAML wrapper build copies them into the packed executable. No PostHog secret is required in the build or runtime environment. The numeric PostHog project ID is for administration; ingestion uses the public key. The language CLI keeps its own PostHog configuration.

Both wrapper implementations share their identity and preferences at `$BAML_HOME/wrapper-telemetry.toml` and their queue at `$BAML_HOME/wrapper-telemetry/`. Set `enabled = false` in the wrapper file to disable wrapper telemetry alone. `DO_NOT_TRACK=1` and `baml telemetry disable` disable both wrapper and CLI telemetry; wrappers honor the global `enabled = false` preference in `$BAML_HOME/telemetry.toml`. An interactive, non-CI first invocation displays the wrapper notice once.

Wrapper events are written locally before a toolchain handoff. If a handoff fails, the later error is sealed into a separate batch without overwriting the invocation batch. A detached helper sends them with a two-second per-request limit, retaining failed deliveries for retry, recovering abandoned live files after 30 minutes, and removing events older than 24 hours. The helper rechecks opt-out before sending. Telemetry failures are silent and do not change command output or status. `BAML_WRAPPER_TELEMETRY_HOST` can point capture delivery at an HTTPS endpoint or loopback HTTP receiver for isolated testing; it defaults to the compiled PostHog ingestion host.

The Rust wrapper sends only PostHog usage events. Release builds of the BAML-native wrapper additionally enable BCS runtime recording through an embedded public ingestion credential. BCS uses the existing packed-runtime policy: delivery runs on a background worker and shutdown waits for its normal bounded drain. The wrapper selects the existing `on_initial_telemetry_failure = "ignore"` policy. The PostHog queue retains its separate retry behavior. Set `BAML_TELEMETRY=off` to disable the BAML wrapper’s BCS recording.

## BCS environment metadata

Enabled cloud runtime recordings automatically include the following scalar properties under the system-populated `baml.ctx.` prefix in the recorded process context's `metadata` map. The recording header's existing `initial_context_cas_id` references this context, which travels through the existing recording/CAS upload pipeline. This does not add a network request or change the recording format. The properties describe the process running BAML, including SDK calls, CLI commands and packed executables that select BCS delivery.

| Property | Type | Meaning |
| --- | --- | --- |
| `baml.ctx.os` | string | Unmodified `std::env::consts::OS`, such as `linux`, `macos`, `windows` or `freebsd`. Presentation labels belong in read/query clients. |
| `baml.ctx.os_version` | optional string | Best-effort OS version from `os_info`; Linux distribution version, macOS product version, or Windows version/build. Omitted when unknown. |
| `baml.ctx.os_distro` | optional string | Recognized Linux distribution name, such as `Ubuntu` or `Debian`; omitted elsewhere and when unknown. |
| `baml.ctx.architecture` | string | Rust build-target architecture, such as `aarch64` or `x86_64`; under emulation this describes the BAML binary. |
| `baml.ctx.is_ci` | boolean | Whether the standard `CI` variable is exactly `true`. Unset, non-Unicode, `false`, `1`, `TRUE`, and arbitrary provider names yield `false`; raw values are never sent. |
| `baml.ctx.agent_harness` | optional string | Trimmed explicit `AI_AGENT` harness name; otherwise `claude`, `codex`, `pi`, `opencode`, `cursor`, `replit`, or `unknown` for the `AGENT` compatibility marker. Omitted when no agent is detected or the name fails the metadata bounds. |

SDK identity/version already have native equivalents in the recording header: `baml_version` identifies the BAML engine version, and `host` identifies the launcher when known (`baml`, `pack`, `bridge`, etc.). The bridge does not currently supply the calling language or its interpreter/JVM version, so these are not guessed from executable paths, environment variables, or installed interpreters.

Harness detection is shared with CLI output policy in `baml_env::agent`. It reuses the Vercel-derived detector from [BAML PR #4726](https://github.com/BoundaryML/baml/pull/4726), including [its review fixes](https://github.com/BoundaryML/baml/blob/3c89421458a11134544a6d7f1a4e5ee9d2b0b1e3/baml_language/crates/baml_cli/src/output.rs). Following the `@vercel/detect-agent` convention, a nonempty, trimmed Unicode `AI_AGENT` value other than `0` explicitly names the harness and takes precedence over inherited markers. Otherwise, named markers are checked in order: `CLAUDECODE`, `CODEX_SANDBOX`, `PI_CODING_AGENT`, `OPENCODE_CLIENT`, `CURSOR_TRACE_ID`, `REPL_ID`. These markers only need to be nonempty and different from `0`; their contents are never reported and need not be valid Unicode. Finally, a nonempty `AGENT` value other than `0` yields `unknown`. The existing `DEV_BAML_CLI_DISABLE_AGENT_DETECTION=true` escape hatch suppresses detection in both consumers. An invalid override still errors in CLI output policy; telemetry omits the harness without failing SDK startup.

System probing is cached after the first enabled cloud recording; CI and harness signals are sampled when a recording starts. Probe failures and unsupported values are best-effort omissions. Optional probe strings are limited to 128 bytes and reject control characters. The explicit `AI_AGENT` value is caller-provided harness-name metadata: any trimmed value meeting these bounds can be transmitted, including a path or credential if one is placed in that variable. Length and control-character checks bound the payload; they are not secret redaction. Named-marker contents and raw CI values are never transmitted. The collector does not enumerate environment variables, read credential variables, query hostname/username/working-directory/browser-URL APIs, collect command arguments, or create a new identity. These six keys are owned by the automatic collector in the recorded process context; unrelated caller metadata and `distinct_id` remain intact. Execution/span contexts and local recordings are unchanged.

Collection runs only through enabled BCS recording startup, after the host's recording policy has resolved. Existing credential selection, artifact policies, telemetry levels, failure handling and permitted opt-outs remain unchanged. Source-based CLI runs and the released BAML-native wrapper honor `BAML_TELEMETRY=off`. For generated SDK bridges and packed binaries, that variable works only when the publisher preserves the standard recording-level binding; use the publisher-selected name if renamed, and an omitted or `false` binding permits no environment-level override of the fixed recording level. See the [artifact binding policy](baml_language/crates/bcs_api/README.md#embedded-telemetry-in-artifacts). `DO_NOT_TRACK` and `baml telemetry disable` retain their existing scope for CLI/wrapper PostHog usage analytics, independently of BCS runtime recording.

### PostHog parity and platform limits

The baseline is the environment information automatically added to ordinary capture events, rather than exception integrations, application-supplied properties, feature flags, or server-side GeoIP enrichment. Official sources were inspected on 2026-11-10; links below pin the inspected revisions because SDK defaults vary across languages and change over time.

| PostHog SDK / BAML runtime | Automatically collected environment baseline | BCS parity / limit |
| --- | --- | --- |
| [Python](https://github.com/PostHog/posthog-python/blob/79f3559ca90bb157f3dce64b2b422b9d7c45eb0f/posthog/utils.py) | `$os`, `$os_version`, Linux `$os_distro`, `$python_runtime`, `$python_version`; SDK identity/version in capture. | Shared OS information and BAML version. OS labels retain Rust's raw values instead of PostHog display names; read/query clients can normalize them for presentation. Python implementation/version are unavailable to the shared native bridge. Distribution names/version formatting can differ from Python's `distro` package. |
| [Go](https://github.com/PostHog/posthog-go/blob/5f936d6e13fdc8c89fd7d981ac5b8230a6cc6c24/system_context.go) | OS/version/distribution and `$go_version`, plus SDK identity/version and the configured server flag. | Shared OS equivalents and BAML version. Go runtime version is not passed across the bridge. |
| [Node](https://github.com/PostHog/posthog-js/blob/7cbe71b301be00f61cb9de598ea6d8fdb5adc1ed/packages/node/src/client.ts) / [core](https://github.com/PostHog/posthog-js/blob/7cbe71b301be00f61cb9de598ea6d8fdb5adc1ed/packages/core/src/posthog-core-stateless.ts) | `$lib`, `$lib_version`, and `$is_server` by default. | BAML identity/version plus native OS information. BAML may run as a CLI or embedded application, so no server classification is inferred. |
| [Ruby](https://github.com/PostHog/posthog-ruby/blob/43d2b92ce78d31b8aa14a8cdfb2aaa46e9aa9c22/lib/posthog/field_parser.rb), [Rust](https://github.com/PostHog/posthog-rs/blob/a48373a784ebcf4ff9d31490cdac530b64a15e89/src/event.rs), [Java server](https://github.com/PostHog/posthog-android/blob/88112924a5d2bd7f0c863f537aac66e09da1f4e3/posthog-server/src/main/java/com/posthog/server/internal/PostHogServerContext.kt), [.NET](https://github.com/PostHog/posthog-dotnet/blob/5517dd559c255ed81f11d943f7c2971090fdda9a/src/PostHog/Api/CapturedEvent.cs) | Ordinary capture includes SDK identity/version; other processing controls are not environment metadata. | BAML identity/version plus native OS information for native SDKs, including C++ and C#. Compiler/toolchain versions are not inferred. |
| [JavaScript browser](https://github.com/PostHog/posthog-js/blob/7cbe71b301be00f61cb9de598ea6d8fdb5adc1ed/packages/browser-common/src/utils/event-utils.ts) | OS, browser/version/language, device/type, screen/viewport, timezone, URL/referrer/campaign context, SDK identity/version. | Browser and Cloudflare Workers WASM builds currently have no BCS recording publisher. No browser/page metadata or fabricated native environment is added there. |
| [Swift / Apple](https://github.com/PostHog/posthog-ios/blob/d237d54a4a411966d99f95e54d7149e0d9ba1932/PostHog/PostHogContext.swift) | OS, app/bundle version, device/model, SDK identity/version; locale/timezone and screen/network details where available. | Native BAML gets OS/version and BAML identity/version. App, device, display and UI properties require the platform-specific SDK and are outside the native engine collector. |

The extra CI and harness fields are BAML-specific. This is native environment parity with explicit gaps, not a claim that all PostHog SDKs collect the same fields or that BAML reproduces browser/mobile analytics.
