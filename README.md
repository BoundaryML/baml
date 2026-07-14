<div align="center">
<a href="https://boundaryml.com?utm_source=github" target="_blank" rel="noopener noreferrer">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="fern/assets/baml-logo-white.png">
    <img src="fern/assets/baml-logo-white.png" height="220" id="top" alt="BAML logo">
  </picture>
</a>

# BAML: Basically A Made-up Language

BAML is the programming language for agents.

[![BAML Version](https://img.shields.io/pypi/v/baml-py?color=006dad&label=BAML%20Version)](https://pypi.org/project/baml-py/)

[Homepage](https://www.boundaryml.com/) | [Explore BAML](https://www.boundaryml.com/explore) | [Discord](https://www.boundaryml.com/discord)

</div>

BAML looks like TypeScript, but every feature is built so agents make fewer mistakes:

- Statically typed like Rust, with colorless concurrency like Go.
- Types persist at runtime. There is no `any`.
- Errors are typed and statically analyzed.
- The filesystem describes the modules/namespaces.
- Run BAML standalone, or call it from any language of your choice (Python, TypeScript, Go, and more).

[Explore the website and examples](https://www.boundaryml.com/explore).

## Try it out

```bash
brew install boundaryml/tap/baml
baml agent install
baml init
baml ide install --code
```

Or read the [quickstart](https://boundaryml.com/quickstart).

## Telemetry

The `baml` CLI sends anonymous usage telemetry so we can see which commands are used and on which platforms. It is sent on each CLI invocation to [PostHog](https://posthog.com) (US cloud) and includes:

- the subcommand run (e.g. `generate`, `test`)
- `baml` version and release channel
- OS and CPU architecture
- whether the invocation looks like CI
- a randomly-generated anonymous ID

It does **not** include your BAML source, prompts, file contents, or any personal information.

### Opting out

Set either environment variable to disable it:

- `DO_NOT_TRACK=1` (the [Console Do Not Track](https://consoledonottrack.com) convention), or
- `BAML_TELEMETRY=0`

## Contributing

See our [guide on getting started](/CONTRIBUTING.md).

---

Made with ❤️ by Boundary. HQ in Seattle, WA.

We're hiring software engineers who love Rust. [Email us](mailto:founders@boundaryml.com) or reach out on [Discord](https://www.boundaryml.com/discord).
