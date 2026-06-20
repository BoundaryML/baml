// ============================================================================
// 29-reliability / usage.baml  —  APPLICATION-author code
//
// What an app author writes to get retry, fallback, and load-balance. The win:
// because `client` is sugar for `function -> Provider`, reliability is just a
// combinator chain on the provider value. No options blob, no `retry_policy:`
// stanza — `.with_retry(...)`, `.fallback_to(...)`, `RoundRobin.over([...])`.
// ============================================================================


// ----------------------------------------------------------------------------
// The two providers the background shows diverging: OpenAI (`service_tier:
// "flex"/"priority"`) and Anthropic (`service_tier: "auto"`, plus `max_tokens`).
// service_tier is a plain field on each — different values, different provider.
// ----------------------------------------------------------------------------

client Gpt4Flex() {
  OpenAIChat {
    model: "gpt-4.1",
    api_key: env.OPENAI_API_KEY,
    service_tier: "flex",            // cheaper, higher/variable latency
    previous_response_id: null,
  }
}

client ClaudePriority() {
  Anthropic {
    model: "claude-opus-4-8",
    api_key: env.ANTHROPIC_API_KEY,
    max_tokens: 4096,
    service_tier: "auto",            // opt into Priority Tier, fall back to standard
  }
}


// ----------------------------------------------------------------------------
// 1. Bare retry. The app author writes ONE combinator call. The typed throws
//    channel inside Retry does the transient-vs-permanent work invisibly.
// ----------------------------------------------------------------------------

client Reliable() {
  Gpt4Flex().with_retry(4)           // 4 transient retries; 400s surface immediately
}

function Extract(resume: string) -> Resume {
  client: Reliable()
  prompt #"Extract the structured resume from: {{ resume }}"#
}

// What the retry budget surfaces lands on the CallError channel
// (`baml.ExtendUnknownError<baml.errors.CallError>`). The app catches by runtime
// match, most-specific first: the concrete LlmError (read its rich `kind`) → the
// CallError interface (the three coarse classifiers, when the handle is opaque) →
// baml.UnknownError (the escape hatch, read its breadcrumb `.message`).
function ExtractOrFallbackText(resume: string) -> Resume {
  Extract(resume) catch (e) {
    let err: LlmError => {
      // The combinator already exhausted transient retries; branch on the taxonomy.
      match (err.kind) {
        FailureKind.ContextLength => { Resume.empty_truncated() }   // shed + re-ask elsewhere
        FailureKind.ContentFilter => { Resume.empty_blocked() }     // clean refusal, not junk
        _ => { Resume.empty_failed() }                              // permanent / exhausted
      }
    }
    let c: baml.errors.CallError => {
      // Only hold the interface here: triage via classifiers, no `kind` to read.
      if (c.is_rate_limit()) { Resume.empty_rate_limited() } else { Resume.empty_failed() }
    }
    let u: baml.UnknownError => { Resume.empty_unknown(u.message) }  // escape hatch
  }
}


// ----------------------------------------------------------------------------
// 2. Fallback that preserves semantics. GPT-4.1, then Claude if GPT is *down*.
//    Both target the SAME `-> Resume` schema, so SAP parsing is identical across
//    them — the fallback doesn't silently change the output contract.
//    A 400 (bad schema) on GPT is NOT retried on Claude (won't get better).
// ----------------------------------------------------------------------------

client Resilient() {
  Gpt4Flex().with_retry(2).fallback_to(ClaudePriority().with_retry(2))
}

function ExtractResilient(resume: string) -> Resume {
  client: Resilient()
  prompt #"Extract the structured resume from: {{ resume }}"#
}


// ----------------------------------------------------------------------------
// 3. Load-balance across two API keys of the same model, then fall back to a
//    different provider. This is the LiteLLM Router shape from the background:
//    round-robin across keys (KEY_A, KEY_B) with a fallback target.
// ----------------------------------------------------------------------------

client Gpt4KeyA() {
  OpenAIChat { model: "gpt-4.1", api_key: env.OPENAI_KEY_A, service_tier: "priority", previous_response_id: null }
}
client Gpt4KeyB() {
  OpenAIChat { model: "gpt-4.1", api_key: env.OPENAI_KEY_B, service_tier: "priority", previous_response_id: null }
}

client Balanced() {
  // RoundRobin.over spreads load + cools a key down after repeated 429s;
  // the whole pool falls back to Claude if both keys are saturated.
  RoundRobin.over([Gpt4KeyA(), Gpt4KeyB()]).fallback_to(ClaudePriority())
}

function Classify(ticket: string) -> Category {
  client: Balanced()
  prompt #"Classify this support ticket: {{ ticket }}"#
}


// ----------------------------------------------------------------------------
// 4. Streaming survives the chain — Retry/Fallback forward Streaming. The .stream
//    companion (conventions §5) matches the combinator's forwarded capability.
// ----------------------------------------------------------------------------

function Summarize(doc: string) -> Summary {
  client: Resilient()
  prompt #"Summarize: {{ doc }}"#
}
// Summarize.stream("...long doc...")  ->  Fallback.stream -> first streaming member.


// ----------------------------------------------------------------------------
// 5. Idempotency: a tool-running agent step has SIDE EFFECTS, so the app author
//    must opt OUT of blind retry. Here they construct Retry by hand to set
//    `idempotent: false` — a transient failure with no response surfaces rather
//    than risk double-executing a tool. (The `.with_retry(n)` sugar defaults to
//    idempotent:true, which is correct only for pure completions.)
// ----------------------------------------------------------------------------

client AgentClient() {
  Retry {
    inner: ClaudePriority(),
    max: 3,
    base_secs: 0.5,
    cap_secs: 30.0,
    idempotent: false,              // <- the load-bearing choice for side-effecting calls
  }
}

function RunAgent(task: string) -> Outcome {
  client: AgentClient()
  prompt #"Do the task, using tools as needed: {{ task }}"#
}
// RunAgent.run_tools("...", tools, ctx)  ->  Tools loop over a retry-wrapped Claude.


// ----------------------------------------------------------------------------
// 6. The trap the background names: a STATEFUL chain breaks under fallback.
//    `previous_response_id` is an OpenAI server-side handle. If you set it and
//    then fall over to Claude, the handle is meaningless. The model gives you no
//    compile-time protection — see evaluation.md (c). The app author must keep
//    server-stateful clients OUT of a fallback strategy, or accept that the
//    fallback turn loses the prior context.
// ----------------------------------------------------------------------------

client StatefulTurn(prev: string) {
  OpenAIChat {
    model: "gpt-4.1",
    api_key: env.OPENAI_API_KEY,
    service_tier: "default",
    previous_response_id: prev,     // server-side conversational state
  }
}
// DON'T: StatefulTurn(id).fallback_to(ClaudePriority())  -- the fallback turn
// cannot see `prev`. The model permits this expression; nothing flags it.
