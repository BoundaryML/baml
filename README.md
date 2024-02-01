<div align="center">
  <a href="https://app.trygloo.com?utm_source=github" target="_blank" rel="noopener noreferrer">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://www.trygloo.com/gloo-ai-square-256.png">
      <img src="https://www.trygloo.com/gloo-ai-square-256.png" height="64">
    </picture>
  </a>
  <h1>BAML</h1>
  <h2>A programming language to get structured data from LLMs<h2>
  <a href="https://discord.gg/ENtBB6kkXH"><img src="https://img.shields.io/discord/1119368998161752075.svg?logo=discord" /></a>
  <a href="https://twitter.com/intent/follow?screen_name=boundaryml"><img src="https://img.shields.io/twitter/follow/boundaryml?style=social"></a>
  <!-- <a href="https://docs.boundaryml.com"><img src="https://img.shields.io/badge/documentation-gloo-brightgreen.svg"></a> -->
  <br /> 
  <a href="https://docs.boundaryml.com">Documentation</a>
 • <a href="https://app.trygloo.com">Dashboard</a>
   <h4>Made by Boundary (formerly Gloo)</h4>
</div>

Most AI engineers use LLMs to get structured outputs (e.g. a json schema) from unstructured inputs (strings). For example, to extract a resume from a chunk of text.

Existing LLM python libraries aren't powerful enough for structured prompting nor do they have easy testing capabilities ([see our comparisons with other frameworks, like Pydantic](https://docs.boundaryml.com/v3/home/comparisons/pydantic)) -- so we decided to build a compiler.

## Introducing BAML + The first VSCode LLM Playground

**BAML** (Basically, A Made-Up Language) is a lightweight programming language to define AI functions with structured inputs and outputs using natural language.

The **BAML VSCode Playground** allows you to test prompts instantly with any LLM, without ever leaving VSCode.
<img src="docs/images/v3/testing_2.gif" />

<figure class="table w-full m-0 text-center image">
    <video
        style="max-width: 90%; margin: auto;"
        autoplay loop muted playsinline
        src="https://github.com/BoundaryML/baml/assets/5353992/4f221238-f0a0-4316-be9d-eb6e17377704"
    ></video>
    <figcaption></figcaption>
</figure>

[Alt video link](https://www.youtube.com/watch?v=dpEvGrVJJng)
Here’s a `.baml` AI function:

```rust example.baml
// example.baml
class Resume {
  name string
  skills string[]
}

function ExtractResume {
  input (resume_text: string)
  output Resume[]
}

impl<llm, ExtractResume> version1 {
  client GPT4Client // client definition not shown
  prompt #"
    Parse the following resume and return a structured representation of the data in the schema below.

    Resume:
    ---
    {#input.resume_text}
    ---
    Output in this JSON format:
    {#print_type(output)}

    Output JSON:
  "#
}
```

(We have better syntax highlighting in VSCode)

**BAML compiles to fully typed Python and TypeScript**. No matter how you change the prompt, or the LLM model, or fail-overs, the python code doesn’t change — unless you change your AI function’s signature.

```python
# app.py
from baml_client import baml as b

async def main():
  resume = await b.ExtractResume(resume_text="""John Doe
Python, Rust
University of California, Berkeley, B.S.
in Computer Science, 2020""")

  assert resume.name == "John Doe"
```

BAML can be deployed to any container, with only a single package dependency required (`e.g. pip install baml`).

<figure>
  <img src="docs/images/v3/baml_playground.png" width="100% alt="BAML Playground" />

  <figcaption>BAML VSCode Playground</figcaption>
</figure>

## Getting Started

Start by [installing BAML](https://docs.boundaryml.com/v3/home/installation) and reading our [Hello World Tutorial](https://docs.boundaryml.com/v3/guides/hello_world/level0).

Learning a new language may seem daunting, but it takes < 10 minutes to get started.

The VSCode extension provides auto-compiling on save, a realtime preview of the full prompt, syntax highlighting and great errors — every syntax error recommends a fix.

Making BAML easy to read and write is our core design philosophy.

#### What you get out-of-the-box

- **Typed Python/Typescript support**
- **VSCode Playground**: see the full prompt and run tests
- **Better code organization** — no scattered jinja templates or yaml files
- **Its fast!** BAML compiles into PY and TS in less than 50ms (We ❤️ Rust)
- **Full integration with** **Boundary Studio** - our observability dashboard
  - Turn live production data into a test-case with one click!
- **Get structured responses,** 11 natively supported types, including custom classes
- **Hallucination Checks**, when LLMs return something unexpected, we throw an exception
- **Works with any LLM,** even your own
- And best of all, **everything lives in your codebase.**

### [Documentation](https://docs.boundaryml.com)

See more at [our docs page](https://docs.boundaryml.com)

### Resources

- [Discord](https://discord.gg/ENtBB6kkXH)

## Security

Please do not file GitHub issues or post on our public forum for security vulnerabilities, as they are public!

Boundary takes security issues very seriously. If you have any concerns about BAML or believe you have uncovered a vulnerability, please get in touch via the e-mail address contact@boundaryml.com. In the message, try to provide a description of the issue and ideally a way of reproducing it. The security team will get back to you as soon as possible.

Note that this security address should be used only for undisclosed vulnerabilities. Please report any security problems to us before disclosing it publicly.
