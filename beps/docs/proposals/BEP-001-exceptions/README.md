---
id: BEP-001
title: "Exception Handling"
shepherds: Vaibhav Gupta <vbv@boundaryml.com>
status: Proposed
created: 2025-11-20
feedback: https://gloo-global.slack.com/docs/T03KV1PH19P/F098LMB0QK0
---

# BEP-001: Exception Handling

Leave comments on either

- [internal boundary slack thread](https://gloo-global.slack.com/archives/C0958DV7YPL/p1764615609844069)
- [public github discussion](https://github.com/orgs/BoundaryML/discussions/2761)

This proposal defines BAML's error handling mechanism: **Universal Catch**.

The core idea: `catch` is an operator that attaches to any scope—functions, loops, or expressions—without requiring structural changes to existing code.

## Document Structure

| Document | Purpose |
|:---------|:--------|
| [00_background.md](./00_background.md) | Error handling landscape and requirements |
| [01_proposal.md](./01_proposal.md) | Universal Catch syntax specification |
| [02_learn.md](./02_learn.md) | Practical guide and FAQ |
| [03_alternatives.md](./03_alternatives.md) | Rejected designs and rationale |
| [04_tooling.md](./04_tooling.md) | IDE and compiler capabilities |
| [05_deviations_from_ts.md](./05_deviations_from_ts.md) | Differences from TypeScript/JavaScript |
| [06_proposal_safe.md](./06_proposal_safe.md) | The `safe` keyword for strict exhaustiveness |

Start with `00_background.md` for context, then `01_proposal.md` for the syntax.

