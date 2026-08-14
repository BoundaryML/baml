

function ConvertToMarkdown(input: string) -> string {
  client CustomGPT4oMini
  prompt ##"
    Take the following input and convert it to a mdx markdown.

    Don't include new lines between lists.
    Use

    Don't include start or end
    ```mdx
    ---
    ---
    ```

    {{ _.role("user") }}
    INPUT:
    ---
    {{ input }}
    ---
 "##
}

test ConvertToMarkdownTest {
  functions [ConvertToMarkdown]
  args {
    input ##"
      Disclaimer, we build BAML (https://www.github.com/boundaryml/baml), a new syntax for building AI applications and agents.

\#\# How Syntax Shapes Thinking

The concepts a syntax exposes fundamentally changes how developers think about problems and solve them. Let’s explore an example from web development.

We used to write websites like this:

```python
def home():
  return """
<button onClick="() => console.log('Button clicked!')">Click me</button>
"""
```

This approach of mixing JavaScript, HTML, CSS, and backend logic proved unmaintainable. A single unclosed tag could break an entire site – an error that a compiler could catch in milliseconds, yet our tooling at that time didn't.

\#\#\# React Entered The Scene

React transformed web development by asking: "What if we brought HTML and JavaScript into the same syntax?" This led to JSX (and later TSX), which solved several critical problems:

- Enforced component-based architecture
- Standardized state management
- Enabled compile-time error checking

This wasn't just a theoretical improvement. Facebook literally made React because their newsfeed had become too complex to maintain using traditional approaches.

Tailwind CSS followed a similar pattern by bringing CSS into React components. Traditional `style.css` files were full of custom 1-off classes that, in practice, were rarely reused. Turns out, the answer was always inline css, just with different syntax and amazing dev-x.

```tsx
// inline CSS
<div style="{ 'background': 'red' }"/>
// Tailwind
<div className="bg-red-300" />
```

https://www.swyx.io/why-tailwind

![Screenshot 2024-12-10 at 9.49.00 PM.png](https://prod-files-secure.s3.us-west-2.amazonaws.com/bfebc62c-55b5-46d2-9c18-8e5d5f1613dd/e0727a0c-bfb6-4179-b2db-1a3d465584e3/Screenshot_2024-12-10_at_9.49.00_PM.png)

5 years ago, I felt the exact same way as @swyx about React. Being a C++ dev, I didn’t want to learn React. It felt like bloatware. So, I wrote my own React [https://github.com/vaibhav-trq/SecretSanta/blob/main/public/src/js/models/page.ts], router and statemanagement and all. I was wrong. React is better. Please forgive my sins.

\#\# Engineering problems with Agents

- **Prompts should be easy to find and read.** Fundamentally they are strings like HTML, but often they are incredibly dynamic - closer to TSX. Strings everywhere are horribly unmaintable.
    - Why not TSX? Not everyone in the world uses Javascript/Typescript. But everyone will use AI.
- **Swapping models should be easy.** Models get better every week, your app should too.
- **You need a hot-reload loop.** When building a frontend, you change your code, cmd+s, the browser reloads (while persisting state from parent components)! Whats the equivalent for agents?
- **It should work with any language (C++, Java, Ruby, …).** Every existing piece of software will eventually be agentic. We shouldn’t have to stand up a python microservice to build anything LLM related.
- **You need structured outputs**. Not every model will support this, and traditional implementations degrade quality.
- **Agents stream** - and streaming requires a different type-system. If the LLM returns a `T[]`, while streaming I likely want a `Partial<T>[]` . This is even more complicated for multi-step agents.
- **No internet dependency** - I don’t want my app to require a paid SAAS to call a REST API.

\#\# Our Proposal: Basic-Ass Machine Learning (BAML)

*or “Basically a Made-up Language” for your boss*

This is our attempt at a syntax that starts to address some of these problems.

In some ways the problem of creating a new syntax is even harder than the ones listed above. Everything is fair game when making new syntax. If you can code it, it can be yours. We needed a design philosophy to help restrict ideas:

\#\#\# BAML’s Design Philosophy

**1:** Avoid invention when possible

Yes, prompts need versioning — we have a great versioning tool: git

Yes, you need to save and iterate on prompts — we have a great storage: filesystems

**2:** Any file editor and any terminal should be enough to use it

**3:** Be fast

**4:** A first year university student should be able to understand it

\#\#\# The core BAML principle: LLM Prompts are functions

The fundamental building block in BAML is a function. Every prompt is a function that takes in parameters and returns a type.

```tsx
function ChatAgent(message: Message[], tone: "happy" | "sad") -> string
```

Every function additionally defines which models it uses and what its prompt is.

```tsx
function ChatAgent(message: Message[], tone: "happy" | "sad") -> string {
  client "openai/gpt-4o"

  prompt \#"
    Be a {{ tone }} bot.

    {% for m in message %}
    {{ _.role(m.role) }}
    {{ m.content }}
    {% endfor %}
  "\#

}

class Message {
  role string
  content string
}
```

- Introducing Strings 2.0

    The best prompting framework is one that gives us the flexibility of english, but with the structure of code.

    - Strings are automatically dedented and trimmed. Great for readability, and exactly what you want in your prompts.
    - Since our strings start with `\#"` and end with `"\#`  you need virtually no escape characters.
    - Yes, you can using `"\#` in your strings. Just use `\#\#\#"` `"\#\#\#` to start and stop your strings.
- Making Jinja better
    - The FIRST and only static analyzer for Jinja, AKA: prompt strings (typescript level)
    - We disabled macros — instead type-safe `template_string`

        ```python
        template_string MyReusableString(topic: string) \#"
          Talk about {{ topic }}.
        "\#

        function ChatAgent(...) -> string {
          client "openai/gpt-4o"
        	prompt \#"
        	  {{ MyReusableString("horses") }}
        	"\#
        }
```


\#\#\# Making prompts easy to find and read

Since every prompt is a function, you can use standard LSP (Language Server Protocols) tools to find every prompt you’ve written. But we’ve taken BAML one step further and built native tooling for VSCode ( jetbrains + neovim coming soon).

1. You can see the full prompt (including any multi-modal assets).

<gif of prompt preview with an image. Open .baml file (with image). click open playground. scroll through prompt (inlcude 2 paramaters so the input string also shows highlighted)>

2. You can see the exact network request we are making

<gif of swapping to curl raw curl (in baml playgroaund already open, switch to raw curl)>

3. You can see every function you’ve ever written

<image of dropdown showing every function>


\#\#\# **Swapping models: 1-line changes**

Its just 1 line.

<image of git diff going from o1 → anthropic sonnet + some funny comment, use shorthand syntax>

Retries and fallbacks? All statically defined.

<image left: code showing retry + fallback defintions, right: client graph. maybe gif of retry count changing, and client graph changing>

Want to do this dynamically at runtime? <link to Client Registry>

\#\#\# **Hot-reloading for prompts**

Using AI is all about iteration speed.

If testing your pipeline takes 2 minutes, in 20 minutes, you can only test 10 ideas.

If testing your pipeline took 5 seconds, in 20 minutes, you can test 240 ideas.

Introducing testing, for prompts.

<gif of code on the left, playground already open on the right. You hit “run test” above the test case>

\#\#\# Plugging into every programming language

< architecture diagram , BAML file → baml client in the language of your choice → how baml parses the result for you>

Since BAML is a configuration file format, the BAML toolchain allows you to transpile BAML into native Python, Typescript, Ruby, etc code. So you get autocomplete and static analysis without thinking about it. Plus, our compiler is fast - it takes less than 30 ms to generate code!

<gif of autocomplete / errors in python + typescript next to a BAML function definition>

All interfaces use the same Rust core, so no more “typescript is broken!”. Every language we natively support is Tier 1 and “works”. Every feature ships to all of them at the same time.

For languages we don’t yet have native support for, BAML can also turn into a local REST API Server and use OpenAPI to generate a typesafe client. Devs are using this for C++, Java, Erlang, Go, PHP etc). However, we do plan on adding more languages with native codegen! Just takes time. (PRs Welcome! 🙃)

\#\#\# **Structured outputs - but better**

JSON is amazing for REST APIs, but way too strict and verbose for LLMs. LLMs need something flexible. We created the SAP (schema-aligned parsing) algorithm to support the flexible outputs LLMs can provide, like markdown within a json blob or chain-of-thought prior to answering.

<gif of some schema from an LLM with chain of thought + no quotes + triple quote strings for multiline markdown>

SAP works with any model on day-1, without depending on tool-use or function-calling APIs. And it all runs without any additional

<link to R1 blog>

<link to O1 blog>

\#\#\# Streaming (when it’s a first class citizen)

Streaming is way harder than it should be. With our [python/typescript/ruby] generated code, here’s what it looks like

<gif coding b.stream.*, show the type during streaming is different than when done>

\#\#\# No strings attached

- 100% open-source (Apache 2)
- 100% private. AGI will not require an internet connection, neither will BAML
- No network requests beyond the model call you explicitly set
- Not stored or used for any training data
- BAML files can be saved locally on your machine and checked into Github for easy diffs.

\#\# Conclusion

As models get better, we’ll continue expecting even more out of them. But what will never change is that we’ll want a way to write maintainable code that uses those models. The current way we all just assemble strings is very reminiscent of the early days PHP/HTML soup in web development. We hope some of the ideas we shared today can make a tiny dent in helping us all shape the way we all code tomorrow.

\#\# Trying BAML

- Online - no installs - [promptfiddle.com](http://promptfiddle.com)
- Learn BAML fast - boundaryml.com/baml
- Locally - follow our guide based on your setup preference
- Link to docs

"##
}
}
