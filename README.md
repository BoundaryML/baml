<div align="center">
<a href="https://boundaryml.com?utm_source=github" target="_blank" rel="noopener noreferrer">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="fern/assets/baml-lamb-white.png">
    <img src="fern/assets/baml-lamb-white.png" height="64" id="top">
  </picture>
</a>

# BAML

[![BAML Version](https://img.shields.io/pypi/v/baml-py?color=006dad&label=BAML%20Version)](https://pypi.org/project/baml-py/)

[Homepage](https://www.boundaryml.com/) | [Explore BAML](https://www.boundaryml.com/explore) | [Docs](https://docs.boundaryml.com) | [Discord](https://discord.gg/BTNBeXGuaS)

</div>

BAML is the programming language for agents.

It is statically typed like Rust, flexible like TypeScript, and parallel like Go. Every feature is built so agents make fewer mistakes: types persist at runtime, there is no `any`, errors are checked, and the filesystem is the namespace. It drops into your existing stack and calls into Python, TypeScript, Ruby, Go, and more.

It looks like this:

```baml
function ChatAgent(message: string, tone: "happy" | "sad") -> string {
  client "openai/gpt-4o-mini"
  prompt #"
    Be a {{ tone }} bot.

    {{ message }}
  "#
}
```

For everything else, see the website: <https://www.boundaryml.com>.

## Try it out

```bash
brew install boundaryml/tap/baml
baml init
baml agent install
baml ide install --code
```

Or read the [quickstart](https://docs.boundaryml.com/get-started/quickstart).

## Contributing

See our [guide on getting started](/CONTRIBUTING.md).

## Citation

```bibtex
@software{baml,
  author = {Boundary ML},
  title = {BAML},
  url = {https://github.com/boundaryml/baml},
  year = {2024}
}
```

---

Made with ❤️ by Boundary. HQ in Seattle, WA.

We're hiring software engineers who love Rust. [Email us](mailto:founders@boundaryml.com) or reach out on [Discord](https://discord.gg/ENtBB6kkXH).
