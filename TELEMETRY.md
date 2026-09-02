# Telemetry

BAML collects telemetry data about general CLI usage and the environment in which the CLI runs. Some collected fields, such as Git identity, repository identity, hostname, and home directory, may identify a person or organization. Participation is optional, and you may opt out if you would not like to share this information.

## Why is telemetry collected?

BAML is a small team building a fast-moving developer tool. Prior to telemetry collection, making decisions about how to improve BAML was a very manual process.

We actively engage with the community — Discord, GitHub issues, direct conversations — to gather feedback. However, that approach only lets us collect feedback from a self-selected subset of users. That subset may have different needs and use cases than you.

Telemetry lets us accurately gauge BAML feature usage, pain points, and customization across everyone who uses the CLI, not just the people with time to write us. This data lets us better tailor BAML to the masses, ensuring its continued growth, relevance, and best-in-class developer experience. It also lets us verify whether improvements we ship are actually improving the baseline of all users' workflows.

Environment and Git metadata help us distinguish real users and repositories from our own development, CI, automation, and ephemeral agent sandboxes. This makes activation and retention measurements less likely to count one environment as many new users.

## What is being collected?

We track the following:

- Command invoked (`baml test`, `baml check`, `baml fmt`, …). Not the arguments.
- Version of BAML and release channel.
- General machine information (operating system, CPU architecture, CPU count, whether the command was run inside CI, Docker, or WSL).
- The detected coding-agent harness name, if a known harness appears to be running the command.
- The machine hostname and the value of the `HOME` environment variable.
- The Git origin host, top-level organization, and repository path (for example, `github.com`, `BoundaryML`, and `BoundaryML/baml`). The raw origin URL is not sent because it may contain credentials.
- The effective Git author name and email and committer name and email that Git would use for a new commit.
- A random per-machine ID, and a salted, one-way hash of the project root. The salt never leaves your machine, so the hash is not reversible.

This list is regularly audited to ensure its accuracy.

You can view exactly what is being collected by setting the following environment variable: `BAML_TELEMETRY_DEBUG=1`.

When this environment variable is set, data will not be sent to us. The data will only be printed out to the stderr stream, prefixed with `[telemetry]`.

## What about sensitive data (e.g. secrets)?

We do not collect BAML source code, prompts, model responses, API keys, logs, serialized errors, arbitrary command arguments, or arbitrary environment variables. We do collect the specific potentially identifying fields listed above, including Git names and emails, hostname, and `HOME`.

Git identity is collected only to understand user and environment attribution. We must never use Git author or committer names or email addresses to contact a user.

## Will this data be shared?

We use telemetry data internally to help us improve the product. We only share aggregated, de-identified reporting for business purposes such as marketing; we do not share raw Git identities, hostnames, home directories, or repository identities for those purposes.

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

You may also opt out by setting `BAML_TELEMETRY_DISABLED=1`. BAML also honors the cross-tool convention `DO_NOT_TRACK=1`. For backwards compatibility, the legacy `BAML_TELEMETRY=0` variable is honored as well.
