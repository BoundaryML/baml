The goal is to build the layer that lets programs in C# to call BAML functions and move values, errors, callbacks, streams, and other behavior across the language boundary.

Python is our reference bridge. The goal is consistency: every bridge should expose and test the same BAML capabilities wherever the target language allows it.

**Before you start**

Open [[state-of-python-completeness]], copy and paste its capability table into your own document for your language, and use that as your working checklist. Keep it updated as you implement and test capabilities.

There are also example documents in the repo. We've already branched based on that, and can be found in:

```bash
baml_language/sdks/agent-docs
# Reference documents describing how the bridges work
# ref-python-examples.md has examples covering every sdkgen case
# It will be _very_ useful to point your agents at these docs
└── bridge-ref
    ├── ref-python-examples.md
    ├── ref-python-inbound-encoding.md
    ├── ref-python-outbound-decoding.md
    ├── ref-python-state-of-completeness.md
    ├── ref-python-type-mappings.md
    ├── ref-ts-examples.md
    ├── ref-ts-inbound-encoding.md
    ├── ref-ts-outbound-decoding.md
    └── ref-ts-type-mappings.md
# The first batch of prompts I used to implement bridge-node
# We recommend implementing things in a different order
# (see "Recommended impl order" below)
└── bridge-node
    ├── 00b-overview.md
    ├── 01-phase1-plan.md
    ├── 02-phase2-plan.md
    ├── 03-phase3-plan.md
    └── 04-phase4-plan.md
```

**Ground rules**

- Copy the Python tests whenever possible—same names, cases, inputs, and assertions.
- If a shared capability is missing coverage, add the test to Python first, then port it.
- Put genuinely language-specific tests in `sdk_tests/crates/<generator_name>/`.
- Don’t mark something supported until the matching parity test passes.
- We’ll have an automated checker comparing the test suites, so keeping names and structure aligned matters.
- **Remember: you should not have to write any new Rust code for the runtime layer.** Native bridges should use the existing CFFI boundary like Python does. TypeScript for WASM/web should use the existing WASM boundary. Most of your work should be in *generating* the target language’s bindings, generated API, tests, and packaging.

**Recommended capability order**

1. Port all the `sdk_tests` over to your target language.
    1. Make sure that when you’re done porting, `cargo nextest run -p sdk_test_$language` runs the tests, so that CI picks them up - see [README.md](http://README.md) and [DEVELOPMENT.md](http://DEVELOPMENT.md) for more details.
2. Generated code layout and free functions with basic types:
    - Solve how BAML namespaces map to the target language’s packages/modules
    - Establish a stable, idiomatic generated-code layout
    - Call free functions with basic types (float, int, string, bool, null) and required arguments only - get these working *before* anything else, so you have a working core
    - When translating type expressions (e.g. function arg types, return types) from BAML to the target language, just generate stubs for enums, classes, unions, etc
3. Required (aka positional) args, and optional (aka named) args
4. Add enums, classes, lists, maps to type expression translation
5. Get enums, classes, lists, maps working as function outputs and as function inputs
6. Type aliases for languages that support type aliases (if your lang supports trivially)
7. Unions for languages that support union types (if your lang supports trivially)
8. 🚨Packaging and publishing (paulo’s CI doc):
    - Package the bridge using the language’s standard package manager
    - Publish it automatically from CI as part of our nightly releases
    - A user should be able to install a nightly build without cloning the repo or compiling the bridge locally
9. Unions types (For languages that don’t natively support union types)
    1. sync with Sam / Aaron / Vaibhav
    2. We’ll have a shared decision on names by Monday 5 pm
10. Async functions and error/panic behavior
11. Static and instance methods
12. Callbacks
13. Generics:
    - Valid type safety for generics in the generated host-language API
    - Plumbing concrete generic types from the host language through the bridge to the runtime
14. Streaming and $build_request
15. Translate types: Cancellation, media, resource types, json, datetime

Get a narrow end-to-end slice working first, then move down the list and complete as much of the capability table as possible.
