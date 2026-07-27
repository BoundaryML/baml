# Internal design notes

These notes define the compiler, standard-library, and runtime contract behind
BEP-064. They are not part of the introductory reading path.

The public proposal should explain what users write and what they receive.
These notes answer the implementation questions:

- [LLM function lowering](./desugaring.md)
- [Core types and inference](./type-model.md)
- [Direct calls and loop ownership](./direct-calls-and-loop-ownership.md)
- [Tools and Agent invariants](./tools-and-agent-invariants.md)
- [Conversations and provider identity](./conversations-and-provider-identity.md)
- [Reliability and error facts](./reliability-and-errors.md)
- [Resources and cleanup](./resources-and-cleanup.md)
- [Proposed standard-library surface](./stdlib-surface.md)
- [Reference implementation differences](./implementation-status.md)
- [Conformance and testing](./conformance.md)

## Public versus internal

The following behavior is public and stable:

- every LLM function can create a typed task;
- a runner determines the lifecycle and result type;
- a direct call returns the function's declared type or throws;
- application tools are executed by the BAML Agent runner;
- provider-owned tools are executed inside a bounded provider operation;
- explicit Agent execution returns `AgentOutcome<T>`;
- provider state cannot be moved between providers as if it were portable
  messages; and
- resources have deterministic close APIs and cleanup fallbacks.

The exact names of compiler-generated symbols, hidden fields, opcodes, and
runtime frames are internal. The pseudocode in these notes specifies behavior,
not an ABI.
