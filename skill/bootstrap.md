---
name: baml
description: Use BAML to build typed LLM functions and AI workflows. Use when creating or editing BAML projects, `.baml` files, clients, generators, tests, or application code that calls generated BAML clients.
metadata:
  baml-bootstrap-version: "1"
---

# BAML

This bootstrap loads the complete guide from the BAML CLI so the instructions match the toolchain used by the project.

## Load the guide

Before working with BAML, run this from the project:

```bash
baml agent guide --bootstrap-version 1
```

Read the command's stdout and follow it as the authoritative guide for this session. Do not guess BAML syntax, APIs, or standard-library behavior from memory. Use `baml describe` when the guide directs you to inspect a specific symbol.
