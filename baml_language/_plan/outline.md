# BEP: AI Functions and Agents — Outline

Full header map. Refer to sections in review as `guides/08 Policies >
Middleware`.

```
_plan/
├── outline.md
├── readme.md
├── style.md          ← prose and structure rules for pages in this BEP
└── pages/
    ├── 01_introduction/
    │   ├── 01_getting_started.md
    │   ├── 02_why.md
    │   └── 03_concepts.md
    ├── 02_guides/
    │   ├── 01_agents.md
    │   ├── 02_sessions.md
    │   ├── 03_configuration.md
    │   ├── 04_steering.md
    │   ├── 05_models.md
    │   ├── 06_tools.md
    │   ├── 07_mcp.md
    │   ├── 08_skills.md
    │   ├── 09_subagents.md
    │   ├── 10_policies.md
    │   ├── 11_journal.md
    │   ├── 12_durability.md    (design notes, not a user guide)
    │   └── 13_serving.md
    ├── 03_examples/
    │   ├── 01_claude_code.md
    │   └── 02_background_jobs.md
    ├── 04_advanced/
    │   ├── 01_errors_and_retries.md
    │   ├── 02_evals.md
    │   └── 03_observability.md
    └── 05_appendix/
        ├── 01_comparisons.md
        └── 02_alternatives_considered.md
```

## Introduction

### 01_getting_started.md
- A typed LLM call
- Make it an agent
- Make it a conversation
- Run it from Python or TypeScript
- Where to go next

### 02_why.md
- The problem
- The approach
- What you do not get
- Relation to other systems

### 03_concepts.md
- The pieces
- Who owns what
- The two laws
- Glossary

## Guides

### 01_agents.md
- An LLM function is a typed function
- An agent is a function with tools
- Task mode
- Configuring a run
- Step budgets
- Errors
- Every call is recorded

### 02_sessions.md
- Creating a session
- Arguments are session constants
- Messages are events
- Running turns
- Configuring a session
- Snapshots
- Named instances

### 03_configuration.md
- Three kinds of configuration
- `$` parameters
- Changing configuration mid-run
- Per-turn knobs
- Precedence
- Defaults in the function block

### 04_steering.md
- The two lanes
- Queued messages
- Interrupts
- Sending events

### 05_models.md
- What a client is
- The three duties
- Same-provider fidelity
- Switching providers mid-session
- Writing a client

### 06_tools.md
- A tool is a function
- How the model sees tools
- Argument validation
- Tool errors
- Tools with state
- Tools and the session
- Dynamic toolboxes

### 07_mcp.md
- MCP servers are toolboxes
- Dynamic discovery
- Schemas and validation
- Replay caveat

### 08_skills.md
- Skills are not a primitive
- The data
- Always-on skills
- Loaded on demand
- What falls out for free
- Design notes

### 09_subagents.md
- Calling an agent from an agent
- Child sessions
- Concurrency
- Cancellation

### 10_policies.md
- What a policy is
- Commands
- The runner
- Middleware
- Composing a stack
- Testing policies

### 11_journal.md
- What the journal is
- Built-in events
- Custom events
- Journal stores
- Compaction

### 12_durability.md (design notes)
- The tiers
- Implementation options for tier 2
- Things to look out for

### 13_serving.md
- Generated SDKs
- In-process sessions
- Stateless sessions
- baml serve
- The wire protocol

## Examples

### 01_claude_code.md
- The agent
- Custom events
- The policy stack
- The app
- Tracing the flows

### 02_background_jobs.md
- Starting a job
- Polling
- From another service
- Job vs. task vs. session
- Provider-side background execution
- Progress from inside the job

## Advanced

### 01_errors_and_retries.md
- The error model
- The error catalog
- Retries, layer by layer
- Letting the agent fail well
- Handling failure at the call site
- Everything lands in the journal

### 02_evals.md
- What to test where
- Scripted clients
- Replaying recorded journals
- Evals

### 03_observability.md
- The journal is the trace
- Two streams
- Usage and cost
- The session tree is the trace tree
- Watching a live session from another process

## Appendix

### 01_comparisons.md
- Pydantic AI
- OpenAI Agents SDK
- Flue
- pi
- LangGraph

### 02_alternatives_considered.md
- One entry point; the runner option picks the handle type
- Runners are infrastructure; identity is per-open
- Passing function arguments versus run configuration
- Static templates; mid-session change is a journaled command
- The journal owns all state
- Two lanes: data queues, control preempts
- Tools are plain functions — no context parameter
- `send` in, `emit` inside
- Arguments are constants; messages are events
- Event unions bind on the policy
- Durability is tiered; tier 3 is out of scope
- Streaming is not history
- One journal per session, sessions form a tree
