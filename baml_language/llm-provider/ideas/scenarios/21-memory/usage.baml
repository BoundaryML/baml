// =============================================================================
// 21-memory — usage.baml  (application-author code)
//
// Shows the three postures an app author actually picks between, and how little
// they write for each. Two memory layers (mem0-curated and raw) prove the
// MemoryStore interface is portable; two model providers (Anthropic, OpenAI)
// prove the Provider side is portable.
// =============================================================================


// -----------------------------------------------------------------------------
// Shared building blocks the app author declares once.
// -----------------------------------------------------------------------------

// The disk (vector DB). Swappable; the layers above never name Qdrant again.
function Disk() -> VectorStore {
  Qdrant { url: env.QDRANT_URL, collection: "memories", embed_model: "text-embedding-3-small" }
}

// mem0-style curated memory: extractor + decider are ordinary providers. THIS is
// the payoff — the memory layer consumes providers, it isn't one.
function CuratedMemory() -> MemoryStore {
  Mem0 {
    disk: Disk(),
    extractor: OpenAI { model: "gpt-4o-mini", api_key: env.OPENAI_API_KEY },
    decider:   Anthropic { model: "claude-haiku-4-5", api_key: env.ANTHROPIC_API_KEY, max_tokens: 1024 },
  }
}

// LangGraph-BaseStore-style raw memory: no curation, policy is ours.
function RawMemory() -> MemoryStore {
  RawStore { disk: Disk() }
}

// A scope is constructed per-request from whoever is logged in.
function ScopeFor(user: string) -> Scope {
  Scope { user_id: user, agent_id: "support", run_id: null, app_id: "myapp" }
}


// =============================================================================
// POSTURE A — middleware. The app author writes essentially nothing extra: wrap
// the chat client in with_memory(...). retrieve-before / store-after happen
// inside .call. This is the Vercel system+onFinish shape.
// =============================================================================

// A plain chat client (Anthropic).
client Assistant() {
  Anthropic { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 2048 }
}

// Same function, memory wrapped around it, scoped to the caller. Because
// with_memory returns a Provider (WithMemory implements Provider), it drops
// straight into the `client:` slot — like .with_retry / .cached.
function ChatWithMemory(user: string, message: string) -> string {
  client: with_memory(Assistant(), CuratedMemory(), ScopeFor(user))
  prompt #"{{ message }}"#
}
// caller: ChatWithMemory("alice", "what should I cook tonight?")
//   -> WithMemory.call: search("what should I cook tonight?", user=alice) finds
//      "User is vegetarian", injects it, calls Claude, then add() extracts &
//      curates the new exchange. The app author never saw the pipeline.

// Swap the memory layer with a one-line change — same function body. This is the
// "swapping vendors is a rewrite" pain reduced to swapping the factory call.
// (It only works because both Mem0 and RawStore implement MemoryStore.)
function ChatWithRawMemory(user: string, message: string) -> string {
  client: with_memory(Assistant(), RawMemory(), ScopeFor(user))
  prompt #"{{ message }}"#
}

// Swap the MODEL too (OpenAI instead of Anthropic) — memory wrapping is
// identical, proving the Provider side is portable under the same middleware.
client AssistantGpt() {
  OpenAI { model: "gpt-4o", api_key: env.OPENAI_API_KEY }
}
function ChatGptWithMemory(user: string, message: string) -> string {
  client: with_memory(AssistantGpt(), CuratedMemory(), ScopeFor(user))
  prompt #"{{ message }}"#
}

// Combinator chain still reads cleanly: memory then retry then fallback. Each
// returns a Provider, so they stack like Iterator adapters.
function ResilientMemChat(user: string, message: string) -> string {
  client: with_memory(Assistant(), CuratedMemory(), ScopeFor(user))
            .with_retry(2)
            .fallback_to(AssistantGpt())
  prompt #"{{ message }}"#
}


// =============================================================================
// POSTURE B1 — memory as a TOOL the model decides to call. The app author hands
// search_memory into the Tools loop. The model recalls only when it wants to.
// =============================================================================

function HelpDesk(user: string, message: string) -> string {
  client: Assistant()                 // a Tools-capable provider (Anthropic)
  prompt #"{{ message }}"#
}
// driven via the .run_tools companion, handing in the memory tool + dispatcher:
//
//   let mem = MemoryTools { store: CuratedMemory(), scope: ScopeFor("alice") };
//   let ctx = ExecutionContext.with_handlers([ (mem.search_tool(), call => mem.handle(call)) ]);
//   HelpDesk.run_tools("alice", "what do you know about me?", [mem.search_tool()], ctx)
//
// The model emits a search_memory ToolCall iff it judges recall useful; the
// canonical Tools.run_tools loop (begin/step/submit) handles the threading.


// =============================================================================
// POSTURE B2 — Anthropic memory-tool: the MODEL drives the whole filesystem.
// No extract->decide pipeline. The app supplies storage; Anthropic supplies the
// protocol + the system prompt that tells the model to read /memories first.
// =============================================================================

function AgentWithFsMemory(user: string, task: string) -> string {
  client: Assistant()                 // Anthropic; build_request emits memory_20250818
  prompt #"{{ task }}"#
}
// driven via .run_tools with the memory-FS tool + its dispatcher:
//
//   let fs = AnthropicMemoryTool { store: RawMemory(), scope: ScopeFor("alice") };
//   let ctx = ExecutionContext.with_handlers([ (fs.descriptor(), call => fs.handle(call)) ]);
//   AgentWithFsMemory.run_tools("alice", "summarize last week's tickets",
//                               [fs.descriptor()], ctx)
//
// The model issues view/create/str_replace calls; OUR handler executes them
// against the store. Same Tools loop as any other tool — only the handler differs
// from posture B1 (one tool that multiplexes FS commands vs one search tool).


// =============================================================================
// WORKING / CORE memory — Letta core blocks. Always-in-context + self-editing.
// This is the THIRD thing: not searched, always injected; edited via tools.
// =============================================================================

// Core blocks are injected every turn (system suffix), independent of search.
function CoreChat(core: CoreMemory, message: string) -> string {
  // We splice core.render() into the system position ourselves — it is always
  // present, so it is NOT a search/retrieve step.
  client: Assistant()
  prompt #"
    {{ core.render() }}
    ---
    {{ message }}
  "#
}
// the self-editing loop = run_tools with the core-memory edit tools:
//
//   let core = CoreMemory { blocks: { "human": "User's name is Alice.",
//                                     "persona": "I am a helpful assistant." },
//                           limit: 2000 };
//   let ctx = ExecutionContext.with_handlers([
//       (core.append_tool(),  call => core.handle(call)),
//       (core.replace_tool(), call => core.handle(call)),
//   ]);
//   CoreChat.run_tools(core, "remember I prefer metric units", [core.append_tool(), core.replace_tool()], ctx)
//
// Under memory pressure the model calls core_memory_replace to compress a block
// — exactly Letta's self-editing behavior, expressed as the canonical Tools loop.


// =============================================================================
// What this scenario CANNOT express in usage code (see evaluation):
//
//   - Letta's server-authoritative agent. Here the app holds the store and
//     re-supplies scope each call; Letta owns the *agent object* on a server and
//     resumes it by agent_id. There is no `client:` form for "an opaque handle to
//     remote stateful agent #42" — the closest is a Provider whose .call proxies
//     to the Letta server, with all three memory tiers hidden behind it.
// =============================================================================

// Letta-as-a-remote-provider: the only honest fit is a Provider that proxies the
// whole stateful agent. Memory is then INVISIBLE to BAML — it lives server-side.
client LettaAgent(agent_id: string) {
  LettaProxy { base_url: env.LETTA_URL, agent_id: agent_id }
}
function AskLetta(agent_id: string, message: string) -> string {
  client: LettaAgent(agent_id)
  prompt #"{{ message }}"#
}
