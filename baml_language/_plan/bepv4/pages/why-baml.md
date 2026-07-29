# BAML vs. Other AI Frameworks

Vercel AI SDK, Claude Agent SDK, and PydanticAI are all capable systems — and
so is the raw OpenAI Python SDK plus hand-rolled parsing. BAML's
pitch is not that those libraries cannot build agents. It is that BAML can
describe the model-facing contract once, keep it typed, and reuse it across
providers, languages, and execution lifecycles.

## The same typed tool agent

This example has a typed input, a typed structured result, two application
tools, and a multi-step tool loop.


| Part                | BAML                                                 | Vercel AI SDK                                 |
| ------------------- | ---------------------------------------------------- | --------------------------------------------- |
| Result contract     | `function ResolveTicketWithTools(...) -> Resolution` | `Output.object({ schema: resolutionSchema })` |
| Tool contract       | `function search_knowledge(...) -> json`             | `tool({ inputSchema, execute })`              |
| Agent configuration | Inside the named LLM function                        | Inside `new ToolLoopAgent(...)`               |
| Run it              | `ResolveTicketWithTools(ticket)`                     | `await supportAgent.generate(...)`            |


Here are the complete versions.

### BAML

```baml
enum TicketPriority {
  Low
  Normal
  Urgent
}

class SupportTicket {
  id: string,
  subject: string,
  body: string,
  customer_tier: string,
}

class Resolution {
  category: string,
  priority: TicketPriority,
  summary: string,
  reply: string,
}

/// Search the support knowledge base.
function search_knowledge(
  query: string,
) -> json throws never {
  {
    "query": query,
    "article": "Duplicate charges are normally pending authorizations.",
  }
}

/// Look up a customer account.
function lookup_account(
  customer_id: string,
) -> json throws never {
  {
    "customer_id": customer_id,
    "status": "active",
    "tier": "pro",
  }
}

function ResolveTicketWithTools(
  ticket: SupportTicket,
) -> Resolution {
  provider: fast_model()
  prompt: `
    Resolve ticket ${ticket.id}. Use the available tools before answering.

    ${ctx.output_format}
  `
  tools: [
    search_knowledge,
    lookup_account,
  ]
}

let resolution: Resolution =
  ResolveTicketWithTools(sample_ticket())
```

### Vercel AI SDK

```typescript
import {
  Output,
  ToolLoopAgent,
  tool,
} from "ai";
import { z } from "zod";

type SupportTicket = {
  id: string;
  subject: string;
  body: string;
  customerTier: string;
};

const resolutionSchema = z.object({
  category: z.string(),
  priority: z.enum([
    "Low",
    "Normal",
    "Urgent",
  ]),
  summary: z.string(),
  reply: z.string(),
});

const searchKnowledge = tool({
  description:
    "Search the support knowledge base.",
  inputSchema: z.object({
    query: z.string(),
  }),
  execute: async ({ query }) => ({
    query,
    article:
      "Duplicate charges are normally " +
      "pending authorizations.",
  }),
});

const lookupAccount = tool({
  description:
    "Look up a customer account.",
  inputSchema: z.object({
    customerId: z.string(),
  }),
  execute: async ({ customerId }) => ({
    customerId,
    status: "active",
    tier: "pro",
  }),
});

const supportAgent = new ToolLoopAgent({
  model: "openai-responses/gpt-5.6-luna",
  instructions:
    "Resolve the ticket. Use the " +
    "available tools before answering.",
  tools: {
    searchKnowledge,
    lookupAccount,
  },
  output: Output.object({
    schema: resolutionSchema,
  }),
});

const { output: resolution } =
  await supportAgent.generate({
    prompt: JSON.stringify(ticket),
  });
```

Both versions are typed. The practical difference is where the types come
from:


| Concern                             | BAML                                                       | Vercel AI SDK                                        |
| ----------------------------------- | ---------------------------------------------------------- | ---------------------------------------------------- |
| Final schema                        | The LLM function's return type                             | A separate runtime schema                            |
| Tool schema                         | The BAML function signature                                | `tool(...)` plus an input schema                     |
| Prompt, provider, and default tools | One named LLM function                                     | Agent configuration plus a call                      |
| Normal result                       | The declared `Resolution`                                  | `GenerateTextResult.output` inferred from the schema |
| Other lifecycle                     | Run `ResolveTicketWithTools@task(ticket)` with another typed runner | Use another method or compose another SDK/runtime    |


This shows the specific  
duplication BAML removes: BAML types/declarations are simultaneously source types,  
model schemas, executable tools, generated SDK contracts, and graph nodes.

## Capability comparison

The middle column deliberately gives the TypeScript stack two libraries:
[Vercel AI SDK](https://ai-sdk.dev/docs/agents) for application agents and
[Claude Agent SDK](https://code.claude.com/docs/en/agent-sdk/overview) for a
coding harness. The Python column includes both
[PydanticAI](https://pydantic.dev/docs/ai/core-concepts/agent/) and its
[official Harness](https://pydantic.dev/docs/ai/harness/).

As of July 2026:

- ✅ means the reviewed public API treats the capability as first-class.
- ◐ means it is available through another integration, provider-specific API,
or application code.
- — means there is no comparable documented surface in the reviewed stack.

For BAML, a check means the capability is part of this BEP's proposed standard
library. It does not claim equal implementation maturity or ecosystem size.


| Capability                                              | BAML | Vercel AI SDK + Claude Agent SDK | PydanticAI + Harness |
| ------------------------------------------------------- | :----: | :--------------------------------: | :--------------------: |
| Typed structured output                                 | ✅    | ✅                                | ✅                    |
| Tool schemas derived from typed function signatures     | ✅    | ◐                                | ✅                    |
| Multi-step application tool loop                        | ✅    | ✅                                | ✅                    |
| Streaming events and final typed output                 | ✅    | ✅                                | ✅                    |
| Dynamic tools and MCP during a run                      | ✅    | ✅                                | ✅                    |
| Tool approval and lifecycle hooks                       | ✅    | ✅                                | ✅                    |
| Resume and fork conversation or harness state           | ✅    | ✅                                | ✅                    |
| Provider routing, retry, and fallback                   | ✅    | ✅                                | ✅                    |
| Replay policy aware of visible output and side effects  | ✅    | ◐                                | ◐                    |
| Background jobs, batches, and caches as typed resources | ✅    | ◐                                | ◐                    |
| Managed realtime voice lifecycle                        | ✅    | ◐                                | ◐                    |
| Sandboxed coding-agent harness                          | ◐    | ✅                                | ✅                    |
| Graphs, traces, and typed run events                    | ✅    | ✅                                | ✅                    |
| Generated TypeScript and Python SDKs from one source    | ✅    | —                                | —                    |


The matrix is intentionally conservative. For example, Vercel AI SDK has
first-class [tool approval](https://ai-sdk.dev/docs/ai-sdk-core/tools-and-tool-calling),
[MCP](https://ai-sdk.dev/docs/ai-sdk-core/mcp-tools), and agent workflows.
Claude Agent SDK supplies sessions, permissions, hooks, MCP, and workspace
tools. PydanticAI supplies typed agents, toolsets,
[durable-execution integrations](https://pydantic.dev/docs/ai/capabilities/durable_execution/overview/),
and a broad harness capability library.

## Where BAML is actually different

Raw feature count is not the strongest argument. The other stacks cover a lot.
BAML's sharper advantages are:

1. **One model-facing declaration.** The prompt, provider, output type, and
default tools have one stable identity.
2. **One typed task, several lifecycles.** Completion, streaming, agents,
background work, and harnesses can consume the same task while returning
different exact types.
3. **Provider requirements are visible to the type checker.** A runner can ask
for streaming, tool calling, realtime, or another capability without
discovering incompatibility after a request starts.
4. **The workflow remains visible to BAML.** Graphs, tests, logs, generated
clients, and errors can all name `ResolveTicketWithTools` even when a generic
runner executes it.
5. **The orchestration is portable.** The same BAML source can generate typed
TypeScript and Python entry points without rewriting the agent in both host
languages.
