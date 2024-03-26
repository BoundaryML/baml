<a href="https://boundaryml.com?utm_source=github" target="_blank" rel="noopener noreferrer">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://www.boundaryml.com/gloo-ai-square-256.png">
    <img src="https://www.boundaryml.com/gloo-ai-square-256.png" height="64">
  </picture>
</a>

## BAML: A programming language to get structured data from LLMs</h2>

## Resources

<a href="https://discord.gg/ENtBB6kkXH"><img src="https://img.shields.io/discord/1119368998161752075.svg?logo=discord&label=Discord%20Community" /></a>
<a href="https://twitter.com/intent/follow?screen_name=boundaryml"><img src="https://img.shields.io/twitter/follow/boundaryml?style=social"></a>

- [Discord Office Hours](https://discord.gg/ENtBB6kkXH) - Come ask us anything! We hold office hours most days (9am - 12pm PST).
- Documentation - [Learn BAML](https://docs.boundaryml.com/v3/guides/hello_world/writing-ai-functions)
- Documentation - [BAML Syntax Reference](https://docs.boundaryml.com/v3/syntax/generator)
- Documentation - [Prompt engineering tips](https://docs.boundaryml.com/v3/syntax/generator)
- [Boundary Studio](https://app.boundaryml.com) - Observability and more

## Motivation

Calling LLMs in your code is frustrating:

- your code uses types everywhere: classes, enums, and arrays
- but LLMs speak English, not types

BAML makes calling LLMs easy by taking a type-first approach that lives fully in your codebase:

1. Define types for your inputs and outputs in BAML files
2. Define prompt templates using these types in BAML
3. Define retries and fallbacks in BAML
4. Use a generated Python/Typescript client to call LLMs with those types and templates

We were inspired by similar patterns for type safety: [protobuf] and [OpenAPI] for RPCs, [Prisma] and [SQLAlchemy] for databases.

BAML guarantees type safety for LLMs and comes with tools to give you a great developer experience:

Jump to [BAML code](#show-me-the-code) or read how we provide type safety without additional LLM calls using [Flexible Parsing](#flexible-parsing).

[protobuf]: https://protobuf.dev
[OpenAPI]: https://github.com/OpenAPITools/openapi-generator
[Prisma]: https://www.prisma.io/
[SQLAlchemy]: https://www.sqlalchemy.org/

<img src="docs/images/v3/prompt_view.gif" />

| BAML Tooling                                                                              | Capabilities                                                                                                                                                                                                                                                                                                                       |
| ----------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| BAML Compiler [install](#installation)                                                    | Transpiles BAML code to a native Python / Typescript library <br />(you only need it for development, never for releases)<br />Works on Mac, Windows, Linux <br /><img src="https://img.shields.io/badge/Python-3.8+-default?logo=python" /><img src="https://img.shields.io/badge/Typescript-Node_18+-default?logo=typescript" /> |
| VSCode Extension [install](https://marketplace.visualstudio.com/items?itemName=gloo.baml) | Syntax highlighting for BAML files<br /> Real-time prompt preview <br /> Testing UI                                                                                                                                                                                                                                                |
| Boundary Studio [open](https://app.boundaryml.com)<br />(not open source)                 | Type-safe observability <br />Labeling                                                                                                                                                                                                                                                                                             |

</p>

## Developer experience

✅ Type-safe guarantee<br />
_The LLM will return your data model, or we'll raise an exception. We use [Flexible Parsing](#flexible-parsing)_

✅ Fast iteration loops ([code](#comparing-multiple-prompts--llms))<br />
_Compare multiple prompts / LLM providers in VSCode_

🚧 Streaming partial jsons ([code](#streaming))<br />
✅ Python
🚧 Typescript<br />
_BAML parses incomplete jsons as they come in_

✅ LLM Robustness for production ([code](#robust-llm-calls))<br />
_Retry policies, Fallback strategies, Round-robin selection_

✅ Many LLM providers<br />
_OpenAI, Azure, Anthropic out-of-the-box. Reach out to get beta access for Mistral and more_

✅ Comments in prompts ([code](#comments-in-prompts))<br />
_Your future self will thank you_

| Use Cases                                                                                            | Prompt Examples                                                                                                                                        |
| ---------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| ✅ Function calling ([code](#function-calling))<br />_Using tools_                                   | ✅ Chain of thought ([code](#chain-of-thought))<br />_Using techniques like reasoning_                                                                 |
| ✅ Classification ([code](#classification))<br />_Getting intent from a customer message_            | ✅ Multi-shot ([code](#multi-shot))<br/>_Adding examples to the prompt_                                                                                |
| ✅ Extraction ([code](#show-me-the-code))<br />_Extracting a Resume data model from unstructed text_ | ✅ Symbol tuning ([code](#symbol-tuning))<br/>_Using symbolic names for data-types_                                                                    |
| ✅ Agents <br />_Orchestrating multiple prompts to achieve a goal_                                   | ✅ Multiple chat roles ([code](#comparing-multiple-prompts--llms))<br />_Use system / assistant / whatever you want. We standardize it for all models_ |
| 🚧 Images<br />_Coming soon_                                                                         |                                                                                                                                                        |

## Installation

### 1. Download the BAML Compiler

Mac:

```bash
brew install boundaryml/baml/baml
```

Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/BoundaryML/homebrew-baml/main/install-baml.sh | bash
```

Windows (with [Scoop](https://scoop.sh/)):

```
scoop bucket add baml-bucket https://github.com/boundaryml/homebrew-baml
scoop install baml
```

### 2. Download VSCode extension

Search for "BAML" or [Click here](https://marketplace.visualstudio.com/items?itemName=gloo.BAML)

> If you are using python, enable typechecking in VSCode’s settings.json:
>
> "python.analysis.typecheckingMode": "basic"

### 3. Add BAML to any existing project

```bash
cd my_project
baml init
```

## Show me the code...

> For now this readme use rust syntax highlighting, but once we have 200 repos using BAML, [Github will support BAML](https://github.com/github-linguist/linguist/blob/master/CONTRIBUTING.md#adding-a-language)!

```rust
// extract_resume.baml

// 1. Define the type for the output
class Resume {
  name string
  // Use an array to get multiple education histories
  education Education[]
}

// A nested class
class Education {
  university string
  start_year int
  // @description injects context into the prompt about this field
  end_year int? @description("unset if still in school")
}

// 2. Define the function signature
// This function takes in a single paramater
// Outputs a Resume type
function ExtractResume {
  input (resume_text: string)
  output Resume
}

// 3. Use an llm to implement ExtractResume.
// We'll name this impl 'version1'.
impl<llm, ExtractResume> version1 {
  client GPT4Client
  // BAML will automatically dedent and strip the
  // prompt for you. You can see the prompt fully
  // in the VSCode preview (including whitespace).

  // We provide some macros like {#input} and {#print_type}
  // to use the types you defined above.
  prompt #"
    Extract the resume from:
    ###
    {#input.resume_text}
    ###

    Output JSON Schema:
    {#print_type(output)}
  "#
}

// Define a reuseable client for an LLM
client<llm> GPT4Client {
  provider "baml-openai-chat"
  options {
    model "gpt-4"
    // Use an API key safely!
    api_key env.OPENAI_API_KEY
    // temperature 0.5 // Is 0 by default
  }
}
```

### See a live prompt preview in VSCode

<img src="docs/images/v3/prompt_view.gif" />

### Run test in VSCode

<img src="docs/images/v3/test-extract.gif" />

### Use your BAML function in your Python/Typescript app

#### Python

```python
# baml_client is autogenerated
from baml_client import baml as b
# BAML also auto generates types for all your data models
from baml_client.baml_types import Resume

async def get_resume(resume_url: str) -> Resume:
  resume_text = await load_resume(resume_url)

  # Call the generated BAML function (this uses 'version1' by default)
  resume = await b.ExtractResume(resume_text=resume_text)

  # Not required, BAML already guarantees this!
  assert isinstance(resume, Resume)

  return resume
```

#### Typescript

```typescript
// baml_client is autogenerated
import baml as b from "@/baml_client";
// BAML also auto generates types for all your data models
import { Resume } from "@/baml_client/types";

function getResume(resumeUrl: string): Promise<Resume> {
  const resume_text = await loadResume(resumeUrl);
  // Call the BAML function (this uses 'version1' by default)
  // This will raise an exception if we don't find
  // a Resume type.
  return b.ExtractResume({ resumeText: content });
}
```

## Classification

```rust
// This will be available as an enum in your Python and Typescript code.
enum Category {
    Refund
    CancelOrder
    TechnicalSupport
    AccountIssue
    Question
}

function ClassifyMessage {
  input string
  // Could have used Category[] for multi-label
  output Category
}

impl<llm, ClassifyMessage> version1 {
  client GPT4

  // BAML also provides a {#print_enum} macro in addition to
  // {#input} or {#print_type}.
  prompt #"
    Classify the following INPUT into ONE
    of the following Intents:

    {#print_enum(Category)}

    INPUT: {#input}

    Response:
  "#
}
```

## Function Calling

With BAML, this is just a simple extraction task!

```rust
class GithubCreateReleaseParams {
  owner string
  repo string
  tag_name string
  target_commitish string
  name string
  body string
  draft bool
  prerelease bool
}

function BuildGithubApiCall {
  input string
  output GithubCreateReleaseParams
}

impl<llm, BuildGithubApiCall> v1 {
  client GPT35
  prompt #"
    Given the user query, extract the right details:

    Instructions
    {#input}

    Output JSON Schema:
    {#print_type(output)}

    JSON:
  "#
}
```

If you wanted to call multiple functions you can combine enums with classes. Almost 0 changes to the prompt.

```diff
+ enum Intent {
+  CreateRelease
+  CreatePullRequest
+ }
+
+ class GithubCreatePullRequestParams {
+   ...
+ }
+
+ class Action {
+   tag Intent
+   data GithubCreateReleaseParams | GithubCreatePullRequestParams
+ }

 function BuildGithubApiCall {
  input string
-  output GithubCreateReleaseParams
+  output Action
 }

impl<llm, BuildGithubApiCall> v1 {
  client GPT35
  prompt #"
    Given the user query, extract the right details:

    Instructions
    {#input}

+   {#print_enum(Intent)}

    Output JSON Schema:
    {#print_type(output)}

    JSON:
  "#
}
```

## Agents

With BAML you combine AI functions with regular code to create powerful agents. That means you can do everything purely in python or typescript!

```python
from baml_client import baml as b
from baml_client.baml_types import Intent

async def handle_message(msg: str) -> None:
    intent = await b.GetIntent(msg)
    if

```

<details>
<summary>Supporting BAML code</summary>
</details>

## Robust LLM calls

We make it easy to add [retry policies] to your LLM calls (and also provide other [resilience strategies]):

[retry policies]: https://docs.boundaryml.com/v3/syntax/client/retry
[resilience strategies]: https://docs.boundaryml.com/v3/syntax/client/client#fallback

```rust
client<llm> GPT4Client {
  provider "baml-openai-chat"
  retry_policy SimpleRetryPolicy
  options {
    model "gpt-4"
    api_key env.OPENAI_API_KEY
  }
}

retry_policy SimpleRetryPolicy {
    max_retries 5
    strategy {
      type exponential_backoff
      delay_ms 300
      multiplier 1.5
    }
}
```

## Chain-of-thought

To do planning with BAML, just tell the LLM what planning steps to do. BAML will automatically find your data objects and convert them automatically.

```rust
impl<llm, GetOrderInfo> version1 {
  client GPT4
  prompt #"
    Given the email below:

    Email Subject: {#input.subject}
    Email Body: {#input.body}

    Extract this info from the email in JSON format:
    {#print_type(output)}

    Schema definitions:
    {#print_enum(OrderStatus)}

    Before you output the JSON, please explain your
    reasoning step-by-step. Here is an example on how to do this:
    'If we think step by step we can see that ...
     therefore the output JSON is:
    {
      ... the json schema ...
    }'
  "#
}
```

## Multi-shot

To add examples into your prompt with BAML, you can use a second parameter:

```rust
function DoSomething {
  input (my_data: int, examples: string)
  output string
}

impl<llm, DoSomething> v1 {
  client GPT4
  prompt #"
    Given DATA do something cool!

    DATA: {#input.my_data}

    Examples:
    {#input.examples}
  "#
}
```

## Symbol-tuning

Sometimes using abstract names as "symbols" (e.g. k1, k2, k3…) allows the LLM to focus on your rules better.

- [research paper](https://arxiv.org/abs/2305.08298)
- Also used by OpenAI for their [content moderation](https://openai.com/blog/using-gpt-4-for-content-moderation).

```rust
// Enums will still be available as Category.Refund
// BAML will auto convert k1 --> Category.Refund for you :)
enum Category {
    Refund @alias("k1")
    @description("Customer wants to refund a product")

    CancelOrder @alias("k2")
    @description("Customer wants to cancel an order")

    TechnicalSupport @alias("k3")
    @description("Customer needs help with a technical issue unrelated to account creation or login")

    AccountIssue @alias("k4")
    @description("Specifically relates to account-login or account-creation")

    Question @alias("k5")
    @description("Customer has a question")

    // Skip this category for the LLM
    Bug @skip
}

// whenever you now use:
// {#print_enum(Category)}
// BAML will substitute in the alias and description automatically
// and parse anything the LLM returns into the appropriate type
```

## Comparing multiple prompts / llms

In BAML you do this by declaring multiple `impls`. The VSCode Extension will also let you run the tests side by side.

```rust
// Same signature as above
function ExtractResume {
  input (resume_text: string)
  output Resume
  // Declare which impl is the default one my python/ts code calls
  default_impl version1
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

## Streaming

BAML is able to offer streaming for partial jsons out of the box. No changes to BAML files, just call a different python function (TS support coming soon).

```python
async def main():
    async with b.ExtractResume.stream(resume_text="...") as stream:
        async for output in stream.parsed_stream:
            if output.is_parseable:
              # name is typed with None | str (in case we haven't gotten the name field in the response yet.)
              name = output.parsed.name
              print(f"streaming: {output.parsed.model_dump_json()}")

            # You can also get the current delta. This will always be present.
            print(f"streaming: {output.delta}")

        # Resume type
        final_output = await stream.get_final_response()
        if final_output.has_value:
            print(f"final response: {final_output.value}")
        else:
            # A parsing error likely occurred.
            print(f"final resopnse didnt have a value")
```

## Observability

Analyze, label, and trace each request in [Boundary Studio](https://app.boundaryml.com).

<img src="docs/images/v3/pipeline_view.png" width="80%" alt="Boundary Studio">

## Comments in prompts

You can add prompts in comments wrapped around `{// comment //}`.

```rust
#"
    Hello world.
    {// this won't show up in the prompt! //}
    Please {// 'please' works best, don't ask.. //} enter your name:
"#
```

Comments can be multiline

```rust
#"
    {//
        some giant
        comment
    //}
"#
```

## Flexible Parsing

> "be conservative in what you send, be liberal in what you accept". The principle is also known as Postel's law, after Jon Postel, who used the wording in an early specification of TCP.
>
> In other words, programs that send messages to other machines (or to other programs on the same machine) should conform completely to the specifications, but programs that receive messages should accept non-conformant input as long as the meaning is clear. [[1]](https://en.wikipedia.org/wiki/Robustness_principle#:~:text=It%20is%20often%20reworded%20as,an%20early%20specification%20of%20TCP.)

LLMs are prone to producing non-conformant outputs. Instead of wasting tokens and time getting the prompt perfect to your needs, we built a parser that handles many of these scenarios for you. The parser uses 0 LLMs, instead relies on the types you define in BAML.

### Example of flexible parsing

<details open>

<summary>BAML Data model</summary>

```typescript
class Quote {
  author string @alias("name")
  quote string[] @description("in lower case")
}
```

</details>

<details open>

<summary>Raw LLM Output</summary>

<pre>
The principal you were talking about is Postel's Law by Jon Postel. 

Your answer in the schema you requested is:
```json
{
   "name": "Jon Postel",
   "quote": "be conservative in what you send, be liberal in what you accept"
}
```
</pre>

</details>

<details open>

<summary>What BAML parsed as</summary>

```json
{
  "author": "Jon Postel",
  "quote": ["be conservative in what you send, be liberal in what you accept"]
}
```

</details>

<details open>
<summary>What the parser did</summary>

1. Stripped all the prefix and suffix text around the object

```json
{
  "name": "Jon Postel",
  "quote": "be conservative in what you send, be liberal in what you accept"
}
```

2. Replaced the alias `name` --> `author`

```json
{
  "author": "Jon Postel",
  "quote": "be conservative in what you send, be liberal in what you accept"
}
```

3. Converted `quote` from `string` -> `string[]`

```json
{
  "author": "Jon Postel",
  "quote": ["be conservative in what you send, be liberal in what you accept"]
}
```

</details>

## FAQ

### Why make a new programming language?

We started building SDKs for TypeScript and Python (and even experimented with YAML), and it worked, but they weren't fun. Writing software should be fun, not frustrating. We set out to build the best developer experience for AI. No boilerplate, dead-simple syntax, great errors, auto-complete.

<img src="https://imgs.xkcd.com/comics/standards.png" />

### Does BAML use LLMs to generate code?

No, BAML uses a custom-built compiler. Takes just a few milliseconds!

### What does BAML stand for?

Basically, A Made-up Language

### What is the BAML compiler written in?

Rust 🦀

### How do I deploy with BAML?

BAML files are only used to generate Python or Typescript code. You don’t need to install the BAML compiler in your actual production servers. Just commit the generated code as you would any other python code, and you're good to go

### Is BAML secure?

Your BAML-generated code never talks to our servers. We don’t proxy LLM APIs -- you call them directly from your machine. We only publish traces to our servers if you enable Boundary Studio explicitly.

### How do you make money?

BAML and the VSCode extension will always be 100% free and open-source.

Our paid capabilities only start if you use Boundary Studio, which focuses on Monitoring, Collecting Feedback, and Improving your AI pipelines. Contact us for pricing details at [contact@boundaryml.com](mailto:contact@boundaryml.com?subject=I'd%20love%20to%20learn%20more%20about%20boundary).

### Why not use Pydantic / Instructor or Langchain?

Here’s our detailed comparison vs [Pydantic and other frameworks](https://docs.boundaryml.com/v3/home/comparisons/pydantic).
TL;DR: BAML is more than just a data-modeling library like Pydantic.

1. Everything is typesafe
2. The prompt is also never hidden from you
3. It comes with an integrated playground
4. can support any model

## Security

Please do not file GitHub issues or post on our public forum for security vulnerabilities, as they are public!

Boundary takes security issues very seriously. If you have any concerns about BAML or believe you have uncovered a vulnerability, please get in touch via the e-mail address contact@boundaryml.com. In the message, try to provide a description of the issue and ideally a way of reproducing it. The security team will get back to you as soon as possible.

Note that this security address should be used only for undisclosed vulnerabilities. Please report any security problems to us before disclosing it publicly.

<hr />

Made with ❤️ by Boundary

HQ in Seattle, WA

P.S. We're hiring for software engineers. [Email us](founders@boundaryml.com) or reach out on [discord](https://discord.gg/ENtBB6kkXH)!
