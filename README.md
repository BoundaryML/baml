<a href="https://boundaryml.com?utm_source=github" target="_blank" rel="noopener noreferrer">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://www.boundaryml.com/gloo-ai-square-256.png">
    <img src="https://www.boundaryml.com/gloo-ai-square-256.png" height="64">
  </picture>
</a>

## BAML: A programming language to get structured data from LLMs</h2>

<div>
<a href="https://discord.gg/ENtBB6kkXH"><img src="https://img.shields.io/discord/1119368998161752075.svg?logo=discord&label=Discord%20Community" /></a>
<a href="https://twitter.com/intent/follow?screen_name=boundaryml"><img src="https://img.shields.io/twitter/follow/boundaryml?style=social"></a>

<a href="https://docs.boundaryml.com"><img src="https://img.shields.io/badge/Docs-Language_Tour-blue?logo=readthedocs" /></a>
<a href="https://docs.boundaryml.com"><img src="https://img.shields.io/badge/Docs-Syntax_Reference-blue?logo=readthedocs" /></a>
<a href="https://docs.boundaryml.com"><img src="https://img.shields.io/badge/Docs-Prompt_Engineering_Tips-blue?logo=readthedocs" /></a>

Supporting Tools

<a href="https://docs.boundaryml.com/v3/home/installation"><img src="https://img.shields.io/badge/BAML_Compiler-Mac_Windows_Linux-default?logo=rust" /></a>
<a href="https://marketplace.visualstudio.com/items?itemName=gloo.baml"><img src="https://img.shields.io/badge/BAML_Extension-Testing_&_Live_Prompt_Preview-orange?logo=visualstudiocode" /></a>
<a href="https://app.boundaryml.com"><img src="https://img.shields.io/badge/Boundary_Studio-Observability_for_BAML-orange" /></a>

<hr />
</div>

<p>

Prompt engineering sucks because:

1. The rest of software uses data models/types (classes, enums, arrays, etc.), not english
2. English is not precise

BAML solves that problem by building prompts using a schema-first approach — where you primarily prompt engineer using data models. [See an example]().

BAML works natively with
<img src="https://img.shields.io/badge/Python-3.8+-default?logo=python" />
<img src="https://img.shields.io/badge/Typescript-Node_18+-default?logo=typescript" />

Our language comes with the following tools:

| BAML Tooling                            | Capabilities                                                                                                              |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| BAML Compiler                           | Transpiles BAML code to a native Python / Typescript library <br />(you only need it for development, never for releases) |
| VSCode Extension                        | Syntax highlighting for BAML files<br /> Real-time prompt preview <br /> Testing UI                                       |
| Boundary Studio <br />(not open source) | Type-safe observability <br />Labeling                                                                                    |

</p>

## Works for

✅ Function calling

✅ Classification (i.e. taking a customer message and getting their intent)

✅ Extraction (i.e. OCRing a resume then parsing into a specific `Resume` class)

✅ Agents (examples pending, but reach out <a href="https://discord.gg/ENtBB6kkXH">Boundary's Discord</a> if you want to do this before we publish them)

## Supports

✅ Many LLM providers (openai, azure, anthropic + bring your own like mistral)

✅ Comparing multiple prompts / LLM providers

✅ Streaming partial jsons (Python works! TS support coming soon)

✅ Retry policies + Falling back on a different model when things

✅ Multiple chat roles

🚧 Images (In progress!)

## Prompt engineering techniques supported natively

✅ Chain of thought

✅ Reasoning

✅ Symbol tuning

## Show me the code...

> For now this readme use rust syntax highlighting, but once we have 200 repos using BAML, [Github will support BAML](https://github.com/github-linguist/linguist/blob/master/CONTRIBUTING.md#adding-a-language)!

```rust
// extract_resume.baml

// 1. Defining the data model.
class Resume {
  name string
  education Education[]
}

class Education {
  university string
  start_year int
  end_year int? @description("unset if still in school")
}

// 2. Define the function signature
function ExtractResume {
  input (resume_text: string)
  output Resume
}

// 3. Write an implementation of ExtractResume
impl<llm, ExtractResume> version1 {
  client GPT4Client
  // BAML will automatically dedent and strip the
  // prompt for you.
  prompt #"
    Extract the resume from:
    ###
    {#input.resume_text}
    ###

    Output JSON Schema:
    {//
      This is macro for injecting your data model
      And yes, this is a comment in the prompt.
      Scroll down to see what it looks like with
      BAML syntax highlighting.
    //}
    {#print_type(output)}
  "#
}

// You can define clients either in the same file or different files.
client<llm> GPT4Client {
  provider "baml-openai-chat"
  options {
    model "gpt-4"
    api_key env.OPENAI_API_KEY
    // temperature 0.5 // Is 0 by default
  }
}
```

### See the prompt and test it in VSCode

<img src="docs/images/v3/testing_2.gif" />

### Use your BAML function in your Python/Typescript app

```python
# The baml_client library is auto generated with the `baml build` command.
# Its usable w/o any dependency on baml.
from baml_client import baml as b
# BAML also auto generates types for all your data models.
from baml_client.baml_types import Resume
import typing

async def get_resume(file_names: typing.List[str]) -> typing.List[Resume]:
  resumes = []
  for name in file_names:
    with open(name, 'r') as file:
      content = file.read()
      # Call your baml function
      # (This is a type-safe function w/ autocomplete!)
      resume = await b.ExtractResume(resume_text=content)
      assert isinstance(resume, Resume) # Not required, BAML already guarantees this!
      resumes.append(resume)
  return resumes
```

```typescript
// The baml_client library is auto generated with the `baml build` command.
// Its usable w/o any dependency on baml.
import baml as b from "@/baml_client";
import { Resume } from "@/baml_client/types";
import fs from "fs";

function getResume(fileNames: string[]): Promise<Resume[]> {
  // Using map to transform fileNames into an array of Promises of Resume
  const resumePromises: Promise<Resume>[] = fileNames.map((name) => {
    // Read file content synchronously
    const content = fs.readFileSync(name, { encoding: "utf-8" });
    // Call your BAML function
    // BAML guarantees this function will return a resume type.
    // Type-safe w/ autocomplete
    return b.ExtractResume({ resumeText: content });
  });

  // Using Promise.all to wait for all promises to resolve
  return Promise.all(resumePromises);
}
```

## Comparing multiple prompts / llms

In BAML you do this by declaring multiple `impls`.

```rust
// Same signature as above
function ExtractResume {
  input (resume_text: string)
  output Resume
  // Declare which impl is the default one my python/ts code calls
  default_impl "version1"
}

// My original impl
impl<llm, ExtractResume> version1 {
  client GPT4Client
  prompt #"
    Extract the resume from:
    ###
    {#input.resume_text}
    ###

    Output JSON Schema:
    {#print_type(output)}
  "#
}

// My new and super improved impl
impl<llm, ExtractResume> with_chat_roles {
  // Since resumes are faily easy, i'll try claude here
  client ClaudeClient
  prompt #"
    {#chat(system)}
    You are an expert tech recruiter.
    Extract the resume from TEXT.

    {#chat(user)}
    TEXT
    ###
    {#input.resume_text}
    ###

    {#chat(assistant)}
    Output JSON Schema:
    {#print_type(output)}
  "#
}

// another client definition
client<llm> ClaudeClient {
  provider "baml-anthropic-chat"
  options {
    model "claude-3-haiku-20240307"
    api_key env.ANTHROPIC_API_KEY
  }
}
```

### Test them both

<img src="docs/images/v3/testing_2.gif" />

##

Streaming partial JSONs

Most AI engineers use LLMs to get structured outputs (e.g. a json schema) from unstructured inputs (strings). For example, to extract a resume from a chunk of text.

Existing LLM python libraries aren't powerful enough for structured prompting nor do they have easy testing capabilities ([see our comparisons with other frameworks, like Pydantic](https://docs.boundaryml.com/v3/home/comparisons/pydantic)) -- so we decided to build a compiler.

## Introducing BAML + The first VSCode LLM Playground

**BAML** (Basically, A Made-Up Language) is a lightweight programming language to define AI functions with structured inputs and outputs using natural language.

BAML comes with a **VSCode Playground**, which allows you to test prompts instantly with any LLM, without ever leaving VSCode.
<img src="docs/images/v3/testing_2.gif" />

<!-- <figure class="table w-full m-0 text-center image">
    <video
        style="max-width: 90%; margin: auto;"
        autoplay loop muted playsinline
        src="https://github.com/BoundaryML/baml/assets/5353992/4f221238-f0a0-4316-be9d-eb6e17377704"
    ></video>
    <figcaption></figcaption>
</figure> -->

<!-- [Alt video link](https://www.youtube.com/watch?v=dpEvGrVJJng) -->

Here’s a `.baml` AI function:

```rust
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
    (see the syntax highlighted prompt in the video)
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

### Language Support

Because we have our own language and our compiler generates native Python/TS code from BAML files, we are able to treat both languages as first class citizens in the ecosystem.

| Language Support | Status | Notes                                                         |
| ---------------- | ------ | ------------------------------------------------------------- |
| Python           | ✅     |                                                               |
| TypeScript       | 🚧     | Pending Retry and Streaming Support, and some stability fixes |

Contact us on Discord if you have a language you'd like to see supported.

### [Documentation](https://docs.boundaryml.com)

See more at [our docs page](https://docs.boundaryml.com)

### Resources

- [Discord](https://discord.gg/ENtBB6kkXH)

## Security

Please do not file GitHub issues or post on our public forum for security vulnerabilities, as they are public!

Boundary takes security issues very seriously. If you have any concerns about BAML or believe you have uncovered a vulnerability, please get in touch via the e-mail address contact@boundaryml.com. In the message, try to provide a description of the issue and ideally a way of reproducing it. The security team will get back to you as soon as possible.

Note that this security address should be used only for undisclosed vulnerabilities. Please report any security problems to us before disclosing it publicly.

<hr />

Made with ❤️ by Boundary
