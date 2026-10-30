# Telemetry

BAML collects completely anonymous telemetry data about general usage.
Participation in this anonymous program is optional, and you may opt out
if you'd not like to share any information.

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

We do not collect any metrics which may contain sensitive data.

This includes, but is not limited to: environment variables, file paths,
contents of files, BAML source code, prompts, model responses, API keys,
logs, or serialized errors.

We take privacy and security seriously. The data we collect is completely
anonymous, not traceable to the source, and only meaningful in aggregate
form. No data we collect is personally identifiable.

## Will this data be shared?

We use telemetry data internally to help us improve the product and will
only share de-identified data in the aggregate for business purposes such
as marketing.

## How do I opt out?

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

The Rust and BAML toolchain wrappers have an independent PostHog pipeline for `wrapper_invocation`, `wrapper_resolution`, `wrapper_install`, `wrapper_selection`, `wrapper_update`, and `wrapper_self_update`. These events describe the wrapper's command, resolution decisions, and operation outcomes. They include schema/version/implementation identifiers, coarse OS/architecture/CPU/CI information, a random anonymous identity and session, and a salted project-root hash. Commands, source categories, toolchain kinds, and outcomes are bounded labels; argument values, selectors, raw paths, and error text are not sent. A delegated CLI command keeps its own separate telemetry; wrapper events do not report that command's final result.

Both wrappers send to the dedicated wrapper PostHog project, 698093. The Rust wrapper owns the public ingestion key and host constants, and the BAML wrapper build copies them into the packed executable. No PostHog secret is required in the build or runtime environment. The numeric PostHog project ID is for administration; ingestion uses the public key. The language CLI keeps its own PostHog configuration.

Both wrapper implementations share their identity and preferences at `$BAML_HOME/wrapper-telemetry.toml` and their queue at `$BAML_HOME/wrapper-telemetry/`. Set `enabled = false` in the wrapper file to disable wrapper telemetry alone. `DO_NOT_TRACK=1` and `baml telemetry disable` disable both wrapper and CLI telemetry; wrappers honor the global `enabled = false` preference in `$BAML_HOME/telemetry.toml`. An interactive, non-CI first invocation displays the wrapper notice once.

Wrapper events are written locally before a toolchain handoff. A detached helper sends them with a two-second per-request limit, retaining failed deliveries for retry, recovering abandoned live files after 30 minutes, and removing events older than 24 hours. The helper rechecks opt-out before sending. Telemetry failures are silent and do not change command output or status. `BAML_WRAPPER_TELEMETRY_HOST` can point capture delivery at an HTTPS endpoint or loopback HTTP receiver for isolated testing; it defaults to the compiled PostHog ingestion host.

The Rust wrapper sends only PostHog usage events. Release builds of the BAML-native wrapper additionally enable BCS runtime recording through an embedded public ingestion credential. BCS uses the existing packed-runtime policy: delivery runs on a background worker and shutdown waits for its normal bounded drain. The wrapper selects the existing `on_initial_telemetry_failure = "ignore"` policy. The PostHog queue retains its separate retry behavior. Set `BAML_WRAPPER_BCS_TELEMETRY=off` to disable the BAML wrapper’s BCS recording.
