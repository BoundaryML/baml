// ============================================================================
// 14-multi-agent / implement.baml   — LIBRARY / STD-LIB AUTHOR CODE
// ============================================================================
//
// Multi-agent sits on a model-decides <-> code-decides spectrum:
//
//   (A) Handoff            : router model calls a synthesized transfer_to_* tool;
//                            the framework SWAPS the active provider.       model
//   (B) Sub-agent-as-tool  : a child Provider is exposed to the parent as a  |
//                            Tool; the parent stays in control.              |
//   (C) Deterministic      : plain BAML functions sequence / fan-out / loop. code
//
// The proposal already gives us the three primitives we lean on:
//   - `Provider` (the irreducible base: call<T>)
//   - `Tools` (begin/step/submit/run_tools, the ToolCall/ToolResult seam)
//   - `client` == `function ... -> Provider`  (so an agent IS a provider-factory)
//
// A handoff/sub-agent is "another agent", and an agent is a Provider. So the
// whole scenario is: how do you turn a `Provider[]` into ONE provider whose
// `.run_tools` loop can switch to (A) or call into (B) the others, and a couple
// of plain functions for (C). Everything below is a combinator over Providers.
//
// Net-new in this file (flagged in evaluation.md):
//   - `Agent`            : a Provider + a name/description/tools bundle (the unit handed around)
//   - `Handoff`          : a Tools-combinator that synthesizes transfer_to_* and swaps providers
//   - `agent_as_tool`    : adapts a Provider into a `Tool` (sub-agent-as-tool)
//   - `History` / `Shared` : the two incompatible state-threading models, made explicit types
//   - ExecutionContext.dispatch is reused as-is; we add an interception point for transfer_*.

// ----------------------------------------------------------------------------
// The unit that gets handed around. An Agent is "a Provider with a name, a
// charter, and its own tools" — exactly what OpenAI's Agent / ADK's LlmAgent are.
// It is NOT a new capability; it is a record carrying a Provider plus metadata
// the router needs to synthesize handoff tools and tool-descriptions.
// ----------------------------------------------------------------------------
class Agent {
  name: string
  description: string          // ADK uses this as the routing trigger text
  provider: Provider           // the existential base — any chat/harness/combinator
  tools: Tool[]                // this agent's own tools (billing-lookup, etc.)

  // Convenience: run this agent's own tool loop to completion against a prompt.
  // Just forwards to the provider's Tools capability (or errors if it has none).
  function run<T>(self, prompt: baml.llm.PromptAst, ctx: ExecutionContext) -> T throws baml.errors.LlmClient {
    match (self.provider) {
      let tp: Tools => tp.run_tools<T>(prompt, self.tools, ctx),
      _ => {
        // No tool loop — degrade to a plain single call (an agent with no tools).
        self.provider.call<T>(prompt)
      },
    }
  }
}

// ============================================================================
// (A) HANDOFF  — model-decides, provider-SWAPPING
// ============================================================================
//
// The hard part the background calls out:
//   - a handoff is lowered to a synthesized `transfer_to_<name>` tool;
//   - when the router calls it, the ACTIVE provider must change to the target;
//   - the parent's history must thread into the child (history model), OR a
//     shared state dict must carry across (ADK model) — two models that DON'T mix;
//   - handoff depth is uncapped -> ping-pong; we must add the cap ourselves.
//
// `Handoff` is a combinator: it wraps a router Agent + a roster of target Agents,
// and itself `implements Tools`. Its `.run_tools` loop is a custom dispatch
// policy: it watches for `transfer_to_*` calls, and instead of returning a tool
// RESULT, it tears down the current transcript and `begin`s a new one on the
// TARGET provider. That is the "swap the active provider" mechanic, expressed
// entirely inside one Tools impl.

// The state-threading model is an explicit choice, because the background says
// the two don't interoperate. We make the incompatibility a TYPE the author picks.
interface Threading {
  // Build the child's opening prompt from (the parent prompt, the run so far).
  function thread(self, parent_prompt: baml.llm.PromptAst, carry: Carry) -> baml.llm.PromptAst throws baml.errors.LlmClient
  type Carry
}

// History model (OpenAI Agents SDK): concatenate the parent's transcript text.
// `Carry = baml.llm.PromptAst` (the running conversation). This is the one that
// "grows unbounded and can blow the context window".
class History {
  max_chars: int?                       // optional truncation policy — the background says there usually ISN'T one
  implements Threading {
    type Carry = baml.llm.PromptAst
    function thread(self, parent_prompt: baml.llm.PromptAst, carry: baml.llm.PromptAst)
        -> baml.llm.PromptAst throws baml.errors.LlmClient {
      // concat parent's running conversation in front of the child charter.
      let joined = baml.llm.concat_prompts(parent_prompt, carry);
      match (self.max_chars) {
        let n: int => baml.llm.truncate_prompt(joined, n),   // the policy the SDKs lack
        _ => joined,
      }
    }
  }
}

// Shared-state model (ADK): carry a typed dict; the child reads keys, not history.
// `Carry = map<string, unknown>` (the session State, with output_key plumbing).
class Shared {
  scope: string                         // "user:" | "app:" | "temp:" — ADK's scoping
  implements Threading {
    type Carry = map<string, unknown>
    function thread(self, parent_prompt: baml.llm.PromptAst, carry: map<string, unknown>)
        -> baml.llm.PromptAst throws baml.errors.LlmClient {
      // child does NOT see history; it sees the scoped state injected as variables.
      baml.llm.inject_vars(parent_prompt, baml.collections.filter_prefix(carry, self.scope))
    }
  }
}

class Handoff {
  router: Agent
  targets: Agent[]
  threading: Threading        // History or Shared — the author commits to ONE
  max_depth: int              // OUR cap; the background: max_turns caps a single loop, NOT handoff depth

  // The transcript Handoff threads through its custom loop. Provider-owned/opaque
  // per the Tools contract, but we own it here so it can carry the depth counter
  // and the running carry. We stash the *delegate's* opaque transcript inside.
  implements Tools {
    type Transcript = HandoffState

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> HandoffState throws baml.errors.LlmClient {
      // The router gets the user's tools PLUS one synthesized transfer tool per target.
      let synthesized = self.synthesize_transfers();
      let active = self.router.provider;
      let inner = match (active) {
        let tp: Tools => tp.begin<T>(prompt, baml.collections.concat(tools, synthesized)),
        _ => throw baml.errors.LlmClient { message: "handoff router provider has no Tools capability" },
      };
      HandoffState {
        active: active,
        active_agent: self.router,
        inner: inner,
        depth: 0,
        user_tools: tools,
        carry: baml.llm.empty_prompt(),
        root_prompt: prompt,
      }
    }

    function step<T>(self, t: HandoffState) -> T | ToolCalls throws baml.errors.LlmClient {
      match (t.active) {
        let tp: Tools => tp.step<T>(t.inner),
        _ => throw baml.errors.LlmClient { message: "active provider lost its Tools capability" },
      }
    }

    // submit is where the SWAP happens. We intercept transfer_to_* calls before
    // they reach the user's dispatch. A transfer is NOT a tool result — it ends
    // the current agent's loop and re-begins on the target provider.
    function submit(self, t: HandoffState, results: ToolResult[]) -> HandoffState throws baml.errors.LlmClient {
      // (results already correspond to calls the OUTER run_tools fed us; but a
      //  transfer call produces no honest result, so the OUTER loop must route it
      //  here as a control signal. We model that by re-reading the pending calls.)
      let pending = baml.agent.pending_transfer(t.inner);     // host: was a transfer_to_* among the calls?
      match (pending) {
        let xfer: TransferCall => {
          if (t.depth + 1 > self.max_depth) {
            throw baml.errors.LlmClient {
              message: baml.strings.concat("handoff depth exceeded ", baml.strings.from_int(self.max_depth)),
            };
          }
          let target = self.find_target(xfer.to_name);
          // thread the parent's run into the child per the chosen model.
          let child_prompt = self.threading.thread(t.root_prompt, t.carry);
          let target_synth = self.synthesize_transfers();       // child can transfer onward too
          let child_inner = match (target.provider) {
            let tp: Tools => tp.begin<T>(child_prompt, baml.collections.concat(target.tools, target_synth)),
            _ => throw baml.errors.LlmClient { message: "transfer target has no Tools capability" },
          };
          HandoffState {
            active: target.provider,
            active_agent: target,
            inner: child_inner,
            depth: t.depth + 1,
            user_tools: t.user_tools,
            carry: baml.agent.extend_carry(t.carry, t.inner),   // accumulate for next handoff
            root_prompt: t.root_prompt,
          }
        },
        _ => {
          // ordinary tool results -> forward to the active provider's submit.
          let inner2 = match (t.active) {
            let tp: Tools => tp.submit(t.inner, results),
            _ => throw baml.errors.LlmClient { message: "active provider lost its Tools capability" },
          };
          HandoffState { active: t.active, active_agent: t.active_agent, inner: inner2,
                         depth: t.depth, user_tools: t.user_tools, carry: t.carry, root_prompt: t.root_prompt }
        },
      }
    }

    // We DO override run_tools, because the default loop in `Tools` would feed
    // transfer calls into ctx.dispatch (which doesn't know them). Our loop peels
    // transfer calls off and routes them to `submit` as control, dispatching the
    // rest normally.
    function run_tools<T>(self, prompt: baml.llm.PromptAst, tools: Tool[], ctx: ExecutionContext) -> T throws baml.errors.LlmClient {
      let t = self.begin<T>(prompt, tools);
      while (true) {
        match (self.step<T>(t)) {
          let value: T => { return value; },
          let calls: ToolCalls => {
            // partition: transfer_to_* (control) vs real tool calls (dispatch)
            let parts = baml.agent.partition_transfers(calls.calls);
            if (baml.collections.is_empty(parts.transfers)) {
              t = self.submit(t, ctx.dispatch(parts.tools));        // normal turn
            } else {
              // a transfer was requested. Dispatch any co-occurring real calls first
              // (rare, but the model may emit both), then swap.
              let t1 = match (baml.collections.is_empty(parts.tools)) {
                true => t,
                _ => self.submit(t, ctx.dispatch(parts.tools)),
              };
              // mark the transfer pending so `submit` performs the swap.
              t = self.submit(baml.agent.set_pending(t1, parts.transfers), []);
            }
          },
        }
      }
      baml.sys.panic("unreachable")
    }
  }

  // Forward .call as a best-effort one-shot of the router (Tools requires Provider).
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      self.router.provider.call<T>(prompt)
    }
  }

  // --- helpers (static-ish; take self) -------------------------------------
  function synthesize_transfers(self) -> Tool[] {
    let out: Tool[] = [];
    for (let a in self.targets) {
      out = baml.collections.push(out, Tool {
        name: baml.strings.concat("transfer_to_", a.name),
        description: a.description,        // ADK uses the child's description as trigger text
        parameters: reflect.type_of<TransferInput>(),   // optional structured "why" payload
      });
    }
    out
  }

  function find_target(self, name: string) -> Agent throws baml.errors.LlmClient {
    for (let a in self.targets) {
      if (a.name == name) { return a; }
    }
    throw baml.errors.LlmClient { message: baml.strings.concat("no such handoff target: ", name) }
  }
}

// Optional structured handoff input ("why was this invoked") — the SDK's on_handoff payload.
class TransferInput { reason: string? }

// A peeled-off transfer request: target derived from transfer_to_<name>.
class TransferCall { to_name: string, reason: string? }

// What `partition_transfers` returns.
class Partition { transfers: TransferCall[], tools: ToolCall[] }

// The Handoff combinator's opaque transcript.
class HandoffState {
  active: Provider
  active_agent: Agent
  inner: $rust_type            // the delegate provider's OWN opaque Transcript
  depth: int
  user_tools: Tool[]
  carry: $rust_type            // accumulated parent-run, shape depends on Threading.Carry
  root_prompt: baml.llm.PromptAst
}

// ============================================================================
// (B) SUB-AGENT-AS-TOOL  — model-decides, but parent stays IN CONTROL
// ============================================================================
//
// ADK's AgentTool / Claude's Agent tool: a child Provider is exposed to the
// parent as an ordinary Tool. The parent's run_tools loop calls it like any
// other tool; the child runs to completion and returns its output as a tool
// RESULT (not a swap). This is a pure adapter: Provider -> Tool, plus a dispatch
// hook that runs the child when its name is called.
//
// We piggyback on ExecutionContext.dispatch: the host already routes ToolCalls
// to handlers by name. `agent_as_tool` returns both the Tool descriptor AND a
// handler the app registers, so when the parent calls it the child agent runs.

class AgentTool {
  child: Agent
  output_type: type             // what the child returns (ADK's output_schema)

  // The descriptor the parent's model sees.
  function descriptor(self) -> Tool {
    Tool {
      name: self.child.name,
      description: self.child.description,
      parameters: reflect.type_of<SubAgentInput>(),
    }
  }

  // The handler the app registers with its ExecutionContext for this tool name.
  // Runs the child to completion with a FRESH context (isolated — the background's
  // "without polluting the main context"). Returns the child's output as the
  // tool's result payload.
  function handle(self, call: ToolCall, ctx: ExecutionContext) -> ToolResult throws baml.errors.LlmClient {
    let prompt = baml.agent.prompt_from_input<SubAgentInput>(self.child.provider, call.args);
    // child gets its OWN nested execution context (own tool dispatch table).
    let child_ctx = ctx.child(self.child.name);
    let out: unknown = self.child.run<unknown>(prompt, child_ctx);
    ToolResult { id: call.id, output: out }          // id correlation preserved (opaque to loop)
  }
}

class SubAgentInput { request: string }

// ============================================================================
// (C) DETERMINISTIC ORCHESTRATION  — code-decides; just functions over providers
// ============================================================================
//
// ADK's SequentialAgent / ParallelAgent / LoopAgent. The proposal already says
// deterministic orchestration is "plain BAML functions". So these are NOT new
// capabilities — they're combinators (Provider in, Provider out) whose `call`
// runs children in code. We model the shared-state ("output_key") plumbing with
// the `Shared` carry, since that's how ADK threads sequential pipelines.

// Sequential: run children in order; each reads prior outputs from shared state.
class Sequential {
  steps: Agent[]
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      let state: map<string, unknown> = {};
      let last: unknown = null;
      for (let a in self.steps) {
        let p = baml.llm.inject_vars(prompt, state);     // editor reads {draft} from state
        let ctx = baml.agent.root_ctx();
        last = a.run<unknown>(p, ctx);
        state = baml.collections.set(state, a.name, last);   // output_key := agent name
      }
      baml.agent.coerce<T>(last)
    }
  }
}

// Parallel: fan children out concurrently; collect all outputs.
class Parallel {
  branches: Agent[]
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      // host runs the closures concurrently and joins.
      let results = baml.async.join_all(baml.collections.map(self.branches, a => {
        () => a.run<unknown>(prompt, baml.agent.root_ctx())
      }));
      baml.agent.coerce<T>(results)            // T is typically a record/array of branch outputs
    }
  }
}

// Loop: repeat children until a condition or max_iterations (LoopAgent).
class Loop {
  body: Agent[]
  max_iterations: int
  until: (unknown) -> bool          // code-decided stop, NOT a model
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      let last: unknown = null;
      for (let i in baml.collections.range(0, self.max_iterations)) {
        for (let a in self.body) {
          last = a.run<unknown>(prompt, baml.agent.root_ctx());
        }
        if (self.until(last)) { return baml.agent.coerce<T>(last); }
      }
      baml.agent.coerce<T>(last)
    }
  }
}

// ----------------------------------------------------------------------------
// Host boundaries (genuine $rust_io_function / $rust_type seams).
// These are the agent-graph plumbing the BAML surface can't express in pure BAML.
// ----------------------------------------------------------------------------
// baml.agent.*
//   pending_transfer(inner)            -> TransferCall?     // peek the delegate transcript's last calls
//   partition_transfers(calls)         -> Partition         // split transfer_to_* from real tool calls
//   set_pending(state, transfers)      -> HandoffState      // mark a swap for `submit`
//   extend_carry(carry, inner)         -> $rust_type        // accumulate parent run for threading
//   prompt_from_input<I>(p, args)      -> baml.llm.PromptAst
//   coerce<T>(value)                   -> T                 // existential narrowing at a deterministic seam
//   root_ctx()                         -> ExecutionContext  // fresh dispatch table
// ExecutionContext gains (per proposal §7 "needs a dispatch and an event-emit surface"):
//   ctx.dispatch(calls)                -> ToolResult[]      // (already proposed)
//   ctx.child(name)                    -> ExecutionContext  // NET-NEW: nested, isolated dispatch table
// baml.llm.*  concat_prompts / truncate_prompt / inject_vars / empty_prompt
// baml.async.join_all  / baml.collections.* / baml.strings.*
