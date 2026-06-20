// =============================================================================
// usage.baml — Server-stored chains, from the APPLICATION author's seat.
//
// The story this file tells: the SAME conversation, written three ways, one per
// ownership model. The app code looks structurally different in each because
// "who holds the conversation" changes how you formulate every request.
// =============================================================================


// -----------------------------------------------------------------------------
// Clients are config-only sugar for `function Name() -> Provider`. Ownership is
// a property of the provider you pick — nothing in the client block declares it.
// -----------------------------------------------------------------------------

// Server-stored-by-id. store:true => the server holds the chain; we hold ids.
client GptChained() {
  OpenAIResponses.default("gpt-5")          // store:true, reasoning_model:true
}

// The SAME provider, deliberately stateless: store:false. Now it is a plain chat
// model with no chain — `.open` will refuse. Same class, different ownership,
// just by flipping a field. Ownership genuinely is a provider property.
client GptStateless() {
  OpenAIResponses { model: "gpt-5", api_key: env.OPENAI_API_KEY, store: false, reasoning_model: true }
}

// Client-held. Anthropic has no server chain; the app owns every byte.
client ClaudeHeld() {
  Anthropic { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 4096 }
}


class Answer { text: string }


// =============================================================================
// MODEL A — server-stored-by-id (OpenAI Responses).
// The app passes a HANDLE, never the bytes. Note how little it writes: no array,
// no system-prompt placement, no trimming. The handle threads the turns.
// =============================================================================

function GeographyChat_ServerStored() -> Answer
    throws baml.ExtendUnknownError<baml.errors.ChainError> {
  client: GptChained()

  // We need the Chain capability, so we match it off the existential provider.
  // (.open / .continue have no plain-call fallback — a stateless call can't
  //  resume a server-stored chain — so the _ arm errors, like Realtime.)
  let p = GptChained();
  match (p) {
    let c: Chain => {
      // Reasoning-continuity is THE reason this layer exists. Assert it before
      // we lean on it; if the model were non-reasoning or store:false this is
      // where we'd find out, not three turns deep. A dropped-reasoning chain is
      // a handle/config domain error, so we raise a concrete ChainError.
      if (!c.preserves_reasoning()) {
        throw ChainNotStored { message: "expected a reasoning-preserving chain" };
      }

      // Turn 1: open the thread. We get the answer AND a handle.
      let t1: ChainTurn<Answer> = c.open<Answer>(render_prompt("What's the capital of France?"));

      // Turn 2: send ONLY the new turn; the server supplies turn 1's context
      // (including the encrypted reasoning items) from t1.next.
      let t2: ChainTurn<Answer> = c.continue<Answer>(t1.next, render_prompt("And its population?"));

      // Branching falls out: continuing AGAIN from t1.next (not t2.next) forks
      // the thread server-side — a sibling of t2, sharing turn 1.
      let t2b: ChainTurn<Answer> = c.continue<Answer>(t1.next, render_prompt("And its area?"));

      t2.value
    },
    // Capability negotiation: the client's provider is not a Chain. Unsupported
    // implements every capability-error interface, so it is legal on this
    // ChainError channel and the caller catches it by its concrete type.
    _ => { throw baml.errors.Unsupported { message: "client does not hold a server-stored chain" }; },
  }
}


// =============================================================================
// MODEL B — client-held (Anthropic). The app OWNS the transcript and resends it.
// Structurally different: there is a session value threaded through, and the
// bytes grow in our process. Durable only because/if we persist `s2.session`.
// =============================================================================

function GeographyChat_ClientHeld() -> Answer
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  client: ClaudeHeld()

  let p = ClaudeHeld();
  let s0 = ClientHeldSession.empty();

  // Each ask() appends to the local array and resends the WHOLE thing.
  let s1: SessionTurn<Answer> = s0.ask<Answer>(p, "What's the capital of France?");
  let s2: SessionTurn<Answer> = s1.session.ask<Answer>(p, "And its population?");

  // To make this durable we would $rust_io persist s2.session.turns ourselves —
  // the provider holds nothing for us. That is the client-held tax.
  s2.value
}


// =============================================================================
// MODEL C — the double-counting trap, made to fail loudly.
// An app author naively wraps a server-stored provider in a client-held session
// (the exact mistake the OpenAI Agents SDK warns about). The session guard
// refuses it: two layers cannot both own the past.
// =============================================================================

function GeographyChat_DoubleCounted() -> Answer
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  client: GptChained()

  let p = GptChained();
  let s0 = ClientHeldSession.empty();
  // ClientHeldSession.ask matches `Chain` off p and THROWS DoubleCountedChain: the
  // provider already holds the chain server-side; layering a client-held session
  // double-counts. (DoubleCountedChain implements CallError, so it rides this channel.)
  let s1: SessionTurn<Answer> = s0.ask<Answer>(p, "What's the capital of France?");
  s1.value
}


// =============================================================================
// PORTABILITY PROBE — the same function over two divergent providers.
// `.call` is portable (every Provider has it). Chaining is NOT: switching the
// client from GptChained() to ClaudeHeld() turns the Chain match's _ arm live,
// because Anthropic does not implement Chain. The leak is honest and visible.
// =============================================================================

function OneShot(q: string) -> Answer
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  // Portable across BOTH providers: just `.call`, no ownership involved.
  client: GptStateless()
  prompt #"{{ q }}"#
}

// Swap the client and OneShot still compiles and runs — proof `.call` ports.
function OneShotClaude(q: string) -> Answer
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  client: ClaudeHeld()
  prompt #"{{ q }}"#
}


// =============================================================================
// HANDLE-NOT-BYTES, ACROSS PROCESS RESTARTS.
// The durability win: the app persists only the handle id (a string), not the
// transcript. A later process resumes by reconstructing the handle and calling
// continue(). This is what "you hold ids, not bytes" buys.
// =============================================================================

function ResumeLater(saved_id: string, follow_up: string) -> Answer
    throws baml.ExtendUnknownError<baml.errors.ChainError> {
  client: GptChained()

  let p = GptChained();
  match (p) {
    let c: Chain => {
      // Reconstruct the opaque handle from a persisted id. We assert ownership so
      // a handle minted by a different provider class can't be smuggled in here
      // (state you cannot migrate — implement.baml refuses foreign owners too).
      let handle = ChainHandle { id: saved_id, kind: ChainKind.ResponseId, owner: "OpenAIResponses" };
      // Consumer triage, most-specific-first: a concrete handle failure (the chain
      // was minted elsewhere or evicted) → the ChainError interface (transport vs
      // rate-limit, classified) → the UnknownError escape hatch (read .message).
      let t: ChainTurn<Answer> = c.continue<Answer>(handle, render_prompt(follow_up)) catch (e) {
        let f: ForeignChainHandle      => return Answer { text: "[unmigratable chain, owned by " + f.owner + "]" };
        let ce: baml.errors.ChainError => {
          if (ce.is_network_error()) { return Answer { text: "[transient network error resuming chain]" }; }
          return Answer { text: "[chain resume failed]" };
        }
        let u: baml.UnknownError       => return Answer { text: "[unknown: " + u.message.join("; ") + "]" };
      };
      t.value
    },
    _ => { throw baml.errors.Unsupported { message: "cannot resume: client is not a Chain provider" }; },
  }
}


// =============================================================================
// CHAIN-AWARE RETRY combinator (from implement.baml). Generic Retry/Fallback do
// NOT thread handles, so we use ChainRetry, which retries continue() against the
// SAME prior id — each attempt branches server-side, so a retry is never a
// double-append. Safe only because the SERVER owns the chain.
// =============================================================================

function GeographyChat_Resilient(prior_id: string, q: string) -> Answer
    throws baml.ExtendUnknownError<baml.errors.ChainError> {
  client: GptChained()

  let p = GptChained();
  match (p) {
    let c: Chain => {
      let retry = ChainRetry { inner: c, max: 3 };
      let prior = ChainHandle { id: prior_id, kind: ChainKind.ResponseId, owner: "OpenAIResponses" };
      let t: ChainTurn<Answer> = retry.continue_retrying<Answer>(prior, render_prompt(q));
      t.value
    },
    _ => { throw baml.errors.Unsupported { message: "not a chain" }; },
  }
}
