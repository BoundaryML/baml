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

You can view exactly what is being collected by setting the following
environment variable: `BAML_TELEMETRY_DEBUG=1`.

When this environment variable is set, data will not be sent to us. The
data will only be printed out to the stderr stream, prefixed with
`[telemetry]`.

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

You may also opt out by setting an environment variable:
`BAML_TELEMETRY_DISABLED=1`. BAML also honors the cross-tool convention
`DO_NOT_TRACK=1`. For backwards compatibility, the legacy
`BAML_TELEMETRY=0` variable is honored as well.
