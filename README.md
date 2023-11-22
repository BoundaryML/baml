# BAML Lang

A Domain Specific Language for AI models.

(BAML) Basically A Made-up Language

BAML is a Domain Specific Language (DSL) for building AI applications and interacting with AI models.

BAML helps you define your ML model inputs and outputs, and generates Python / Typescript (soon) code to call your models. Rather than interacting with LLMs using raw strings, BAML adds a thin but powerful layer between your LLM and your code that translates all strings to your strongly-typed model inputs and outputs.

Because BAML can track every type sent and received, that means you can query it more powerful ways -- so you can train other custom models using LLM-generated data. This language has also allowed us to build powerful features like an **integrated VSCode BAML playground**.

If you’re building a chatbot, or any LLM application, BAML is a good way to decompose your task into specific more measurable tasks (classification, entity extraction, etc).

BAML is inspired by **[Prisma](https://www.prisma.io/)**, a DSL for databases. BAML is designed to be a **DSL for AI model tasks**.

### [Documentation](https://docs.trygloo.com)

See more at [our docs page](https://docs.trygloo.com)

## BAML Components

1. **VSCode Extension** - A VSCode extension that provides a playground, syntax highlighting and static analysis for BAML files.
2. **BAML CLI** - A CLI that provides code generation for BAML files to python (and soon typescript)
3. **BAML Python Library** - Utilities that enables generated BAML functions to trace inputs and outputs. We build on top of the **OpenTelemetry** standard.
4. **Gloo Dashboard** - A dashboard that allows you to view and query your BAML executions, run test suites, label data, and soon train new models.
