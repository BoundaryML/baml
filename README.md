<div align="center">
  <a href="https://app.trygloo.com?utm_source=github" target="_blank" rel="noopener noreferrer">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://www.trygloo.com/gloo-ai-square-256.png">
      <img src="https://www.trygloo.com/gloo-ai-square-256.png" height="64">
    </picture>
  </a>
  <h1>BAML</h1>
  <h2>A programming language to build type-safe natural language Functions<h2>
  <a href="https://discord.gg/ENtBB6kkXH"><img src="https://img.shields.io/discord/1119368998161752075.svg?logo=discord" /></a>
  <a href="https://twitter.com/intent/follow?screen_name=tryGloo"><img src="https://img.shields.io/twitter/follow/tryGloo?style=social"></a>
  <!-- <a href="https://docs.boundaryml.com"><img src="https://img.shields.io/badge/documentation-gloo-brightgreen.svg"></a> -->
  <br /> 
  <a href="https://docs.boundaryml.com">Documentation</a>
 • <a href="https://app.trygloo.com">Dashboard</a>
   <h4>Made by Boundary (formerly Gloo)</h4>
</div>

The Boundary Toolchain is a suite of tools that enable test-driven AI development using strongly typed function interfaces.

## Our Inspiration

The first problem LLM developers had to solve was strings. In the LLM world, everything is a string and that sucks.

Strongly-typed systems are more robust and easier to maintain.

For example, Microsoft created TypeChat (7.1k stars) to get structured outputs out of LLMs. [See example](https://github.com/microsoft/TypeChat/blob/main/examples/sentiment/src/main.ts).

A python framework, [Marvin](https://github.com/PrefectHQ/marvin) (4.2k stars), also helped developers declare structured AI interfaces using their `@ai_fn` decorator. Under the hood, it calls openai for you. It's really elegant!

```python
from typing_extensions import TypedDict
from marvin import ai_fn

class DetailedSentiment(TypedDict):
    """A detailed sentiment analysis result.

    - `sentiment_score` is a number between 1 (positive) and -1 (negative)
    - `summary_in_a_word` is a one-word summary of the general sentiment
    """
    sentiment_score: float
    summary_in_a_word: str

@ai_fn
def get_detailed_sentiment(text: str) -> DetailedSentiment:
    """What do you think the sentiment of `text` is?"""

get_detailed_sentiment("I'ma Mario, and I'ma gonna wiiiiin!")
# {'sentiment_score': 0.8, 'summary_in_a_word': 'energetic'}
```

## But, types are not all you need

Again, type-safety is amazing. Providing guarantees on the output of the LLM helps a lot, but we think both of these didn’t go far enough. They left a few questions unanswered:

1. **What is the full prompt?** _You can’t see it until you run the code with debug settings. Does updating the library break me?_
2. **How do you test?** _Do you copy pasting prompts and json blobs into OpenAI’s playground or into boilerplate pytest code?_
3. **How do you fail-over** to Anthropic when GPT4 goes down?
4. **How do you test against that other LLMs?** _Copy and paste or do you build an abstraction layer?_

Answering these questions requires more than just a python library.

## Introducing BAML + The first VSCode LLM Playground

BAML is a lightweight programming language to define AI function interfaces, with a native VSCode extension.

Watch this 1-min video on how you can create and test AI functions without ever leaving VSCode.
(Click to watch)
<a href="https://www.youtube.com/embed/dpEvGrVJJng?si=6CPRTxil8WjQ_t5w" target="_blank">
<img src="https://img.youtube.com/vi/dpEvGrVJJng/mqdefault.jpg" alt="Watch the video" border="10" />
</a>

Here’s what a `.baml` AI function looks like (watch the video to see the prompt):

```rust
// example.baml
function GetDetailedSentiment {
    input string
    output DetailedSentiment
}

class DetailedSentiment {
    sentiment_score float
    summary_in_a_word string
}
```

**BAML compiles to fully typed Python and TypeScript**. No matter how you change the prompt, or the LLM model, or fail-overs, the python code doesn’t change — unless you change your AI function’s signature.

```python
# app.py
from baml_client import baml as b

async def main():
    message = "I'ma Mario, and I'ma gonna wiiiiin!"

    # Your AI function defined in .baml files
    response = await b.GetDetailedSentiment(message)

    # Response is automatically strongly typed and
    # works with auto complete!
    print(f"Score: {response.sentiment_score}")
    print(f"Summary: {response.summary_in_a_word}")
```

<figure>
  <img src="docs/images/v3/baml_playground.png" width="100% alt="BAML Playground" />

  <figcaption>BAML VSCode Playground</figcaption>
</figure>

## Getting Started

Start by [installing BAML](https://docs.boundaryml.com/v3/home/installation) and reading our [Hello World Tutorial](https://docs.boundaryml.com/v3/guides/hello_world/level0).

Learning a new language seems daunting, but it takes < 10 minutes to get started.

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
