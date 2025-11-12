# Boundary - hiring engineers that enjoy hard things

We build BAML -- a new programming language to build AI powered applications.

*What does the code look like when 50% of the business decisions is decided by an AI agent / prompts? How do you test these AI components? What tools should we build?*

Every computing paradigm, brought with a new language

| Paradigm | Why? | Language | 
| -- | -- | -- |
| Desktop Computer | Holepunching and ASM were far too inaccessible | C |
| Multiple OSs | Why write 3 variants of C | Java |
| A computer on every desk | Lets make software more readable | Python |
| Web browsers | Event driven / async native | JS |
| AI / Graphics | We need to do a lot of math, and fast | Cuda |
| Interactive Website | HTML + Logic is hard to do at scale | React |
| Non-determinism | My software is isn’t predictable because AI | ????? |

BAML is our answer, and it will let more developers than ever write AI pipelines without having to think about AI model reliability.

## Why you should join us

Most startups die quietly. Most programming languages never leave a compiler talk.

But every once in a generation, someone builds the next C, Java, Python, JS, React or Cuda.
We’re trying to build that: a language for reasoning with AI.

Most likely, we will fail spectacularly.
But if we don’t, it’ll be because a small group of curious, fearless builders decided to bet on beauty and correctness in an age of chaos. When writing code becomes cheap / free, its the developer experience around reading and debugging systems that becomes the most valuable.

If that excites you, then Boundary is your place.
But don’t take our word for it, try BAML first.

## Responsibilities

* Since we’re building a compiler, you’ll be able to solve some of the hardest and most interesting problems — e.g. how to suspend/resume AI workflows, how to support calling BAML functions from any language, how to create a graph visualization from BAML code users or AI agents write, and exploring unique syntaxes.
* Design and implement tooling to give users the best developer experience — this includes adding features to our LLM observability platform and scaling it to support handling billions of logs.
* Answer community questions and learn how they leverage BAML so we can make it even better

## To apply

The best words that describe the kinds of people we’ve hired to date are: Curious and Fearless

* Curious - never assume you already know, and challenge the status quo
* Fearless - build anything. Its just bits on a machine, make them do what you want

Send a message with subject: “Why I’m awesome” with 3 of your most incredible achievements in life (technical and/or personal achievements). E.g. “I ran an ultramarathon in XYZ hours” or “I wrote a Rust crate used by millions of devs each month”. Brag about yourself, and articulate what about it was hard and what outcome came from it.

This isn’t a trick question, we’re building a programming language, and communication is the most important trait for us.

[Past Examples](https://drive.google.com/file/d/1pFXmqQVnMmCdxdpcuF9Zwk6SRCMgJglL/view?usp=drive_link) from candidates we’ve hired. READ THESE so you know what we might look for. The best emails include metrics + links.

Where to apply:

Preferred: [YC's Work At a Startup Portal](https://www.ycombinator.com/companies/boundary/jobs/f31pAPu-engineers-that-enjoy-hard-things-like-compilers)

Acceptable: [vbv@boundaryml.com](mailto:vbv@boundaryml.com?subject=why%20i'm%20awesome)

## Our Tech Stack

BAML - Rust + FFI bindings to each language we interface with

BAML LSP / Editor Extensions (VSCode, Jetbrains, Zed, …) - Typescript/Nextjs, Rust + WASM

Boundary Cloud - Rust backend, Typescript / Nextjs frontend

## FAQ

**Do I need to know Rust?** No, but you should be able to learn it, and learn it fast.

**Do I need to know about programming languages / compilers?** No, we don’t need to hire language experts. We train language experts.

Generally speaking, no prior knowledge needed, but you should be able to take any problem, and solve it. If graph / tree problems are stressful, Boundary is probably not a good fit. If questions like the following seem fun, this is gonna be a heck of ride.

* what syntax ergonomics can make AI better at grepping for code?
* how does LLVM work?
* how does react’s re-rendering work?
* how do I design a package manager?
