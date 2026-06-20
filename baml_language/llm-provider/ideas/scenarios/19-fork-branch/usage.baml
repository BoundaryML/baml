// =============================================================================
// usage.baml — what an APPLICATION author writes to fork / branch. End-user code.
//
// Shows:
//   * how little the app writes to get a branchable conversation: a `client`
//     (config only) + the `.session` / `.fork` companions.
//   * capability negotiation as a runtime `match`: "is this client branchable?"
//     is answered by `match (p) { let b: Branching => ... }`, never the type
//     checker (proposal §5) — because `client` returns the existential Provider.
//   * TWO providers with the background's divergence (Claude copy-prefix vs
//     Responses emergent-ids) used through the SAME app code — proving (or
//     leaking) portability.
//   * a combinator interaction: forking a Fallback is awkward (see evaluation).
// =============================================================================


// --- clients: config only, exactly like every other scenario ---------------

client Brancher() {
  ClaudeSdkSession { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY }
}

client ServerChain() {
  ResponsesSession { model: "gpt-5", api_key: env.OPENAI_API_KEY }
}

client Tree() {
  // pointer/DAG provider; the store is a bound field, like `model`.
  PiSession {
    model: "claude-sonnet-4-6",
    api_key: env.ANTHROPIC_API_KEY,
    store: NodeStore { _handle: baml.kv.tree("conversations") },   // host-provided tree handle
  }
}


// --- companions: the app calls these, the framework generates the `match` ----
//
// These are the `.session`, `.turn`, `.fork` companions of a conversation
// function. Written out longhand here to show the negotiation the sugar hides.
// In the real surface the app writes `Chat.session(...)`, `s.fork(at)`, etc.

// Open a session. Every provider that is `Session` supports this; otherwise we
// have no honest fallback (a one-shot `call` is not a conversation), so error.
function OpenChat<T>(client_provider: Provider, system: string) -> SessionHandle
    throws baml.ExtendUnknownError<baml.errors.SessionError> {
  match (client_provider) {
    let sess: Session => SessionHandle { provider: sess, state: sess.open<T>(render_prompt(system)) },
    // Unsupported implements every capability interface, so it is legal on the
    // SessionError channel and the app catches it by its concrete type.
    _                 => throw baml.errors.Unsupported { message: "client is not a Session provider" },
  }
}

// Take one turn. Mutates the immutable session value -> returns a new handle.
function Say<T>(h: SessionHandle, message: string) -> SessionHandle
    throws baml.ExtendUnknownError<baml.errors.SessionError> {
  SessionHandle { provider: h.provider, state: h.provider.turn<T>(h.state, render_prompt(message)) }
}

// Fork. THE capability gate: branching is its own interface, so a Session that
// is not Branching (OpenAI Agents Session, Vercel) lands in `_` and errors —
// there is no honest way to fake a fork. `at` null = fork at tail.
function Fork<T>(h: SessionHandle, at: Cursor?) -> SessionHandle
    throws baml.ExtendUnknownError<baml.errors.BranchError> {
  match (h.provider) {
    let b: Branching => SessionHandle { provider: b, state: b.fork<T>(h.state, at) },
    _                => throw baml.errors.Unsupported {
      message: "this conversation cannot branch (no Branching capability)",
    },
  }
}

// A bag holding the provider + the opaque session value together, so the app
// can chain turn/fork without re-matching every time. (net-new app-side type.)
class SessionHandle {
  provider: Session          // the negotiated capability (Branching widens to this)
  state: $rust_type          // the provider's opaque Session value, carried verbatim
}


// --- the actual application flow --------------------------------------------
//
// "Draft a reply, then explore a more-formal alternative from the same point,
//  WITHOUT losing the original draft." This is the background's goal verbatim.

function DraftWithAlternative(client_provider: Provider, brief: string) -> TwoDrafts
    throws baml.errors.SessionError | baml.errors.BranchError | baml.UnknownError {
  // 1. open + two turns on the main line.
  let h0 = OpenChat<string>(client_provider, "You are a copywriter. Be concise.");
  let h1 = Say<string>(h0, brief);
  let casual: string = h1.provider.latest<string>(h1.state);

  // 2. fork at the TAIL and explore a formal continuation. The original `h1`
  //    is untouched — that is the whole point of branching.
  let forked = Fork<string>(h1, null);                     // null => tail
  let h2 = Say<string>(forked, "Rewrite that in a formal, executive tone.");
  let formal: string = h2.provider.latest<string>(h2.state);

  TwoDrafts { casual: casual, formal: formal }
}

class TwoDrafts { casual: string, formal: string }


// --- fork ANYWHERE (not just the tail) --------------------------------------
//
// Branch from turn #2 even after the conversation moved on to turn #5. Claude
// (copy) and Pi (pointer) both honor this; Responses honors it ONLY if it kept
// the id; a tail-only provider would throw CannotForkHere.

function ReviseEarlierTurn(client_provider: Provider, brief: string) -> string
    throws baml.errors.SessionError | baml.errors.BranchError | baml.UnknownError {
  let h0 = OpenChat<string>(client_provider, "You are a copywriter.");
  let h1 = Say<string>(h0, brief);
  let early = h1.provider.head(h1.state);                  // remember this position
  let h2 = Say<string>(h1, "Add a call to action.");
  let h3 = Say<string>(h2, "Make it punchier.");           // conversation moved on...

  // ...now go BACK to `early` and take a different path.
  let branch = Fork<string>(h3, early);                    // fork-anywhere
  let h4 = Say<string>(branch, "Actually, keep it understated.");
  h4.provider.latest<string>(h4.state)
}


// --- portability check: SAME app code, two divergent providers --------------
//
// Run DraftWithAlternative against copy-based Claude and emergent-id Responses.
// Both produce TwoDrafts. The divergence is INVISIBLE here — proving the seam —
// EXCEPT when the app tries to copy the branch out (next function).

function DraftViaClaude(brief: string)  -> TwoDrafts
    throws baml.errors.SessionError | baml.errors.BranchError | baml.UnknownError { DraftWithAlternative(Brancher(), brief) }
function DraftViaServer(brief: string)  -> TwoDrafts
    throws baml.errors.SessionError | baml.errors.BranchError | baml.UnknownError { DraftWithAlternative(ServerChain(), brief) }
function DraftViaTree(brief: string)    -> TwoDrafts
    throws baml.errors.SessionError | baml.errors.BranchError | baml.UnknownError { DraftWithAlternative(Tree(), brief) }


// --- where the leak surfaces: exporting the branch --------------------------
//
// The app wants to back up / migrate the branch point. Claude & Pi can; the
// server-authoritative Responses provider THROWS ForkNotOwned. The proposal
// cannot promise "you own your branch" at the type level — it surfaces as a
// runtime error the app must handle. This is the model being honest, not clean.

function BackupBranch<T>(h: SessionHandle) -> Transcript
    throws baml.ExtendUnknownError<baml.errors.BranchError> {
  match (h.provider) {
    let b: Branching => b.copy_out<T>(h.state) catch (e) {
      // most-specific first: the concrete provider error, then the interface,
      // then the universal escape hatch.
      let nf: ForkNotOwned => {
        // server-stored: we can NAME the branch but not export it. Degrade to
        // a placeholder transcript that tells the truth (complete=false).
        Transcript { turns: [], complete: false };
      },
      let be: baml.errors.BranchError => { throw be; },   // any other branch failure: re-raise
      let u: baml.UnknownError        => { throw u; },    // foreign/transport: re-raise
    },
    _ => throw baml.errors.Unsupported { message: "not branchable" },
  }
}


// --- combinator interaction (the awkward bit; see evaluation §c) ------------
//
// A Fallback is a Provider, so the `client` sugar lets you wrap it. But a
// Session is STATEFUL — its opaque state belongs to ONE member. Forking across a
// fallback is meaningless: you cannot move a Claude session value into a
// Responses chain. (And under the marker revision these providers expose ONLY
// the Session/Branching surface — there is no degenerate single-turn `call` left
// for a Fallback to forward statelessly anyway.) So we do NOT route
// Session/Branching through Fallback; we pin to a single member up front.

client ResilientChat() {
  // pin the conversation to ONE branchable provider; a Fallback over these
  // Session providers could only forward the STATEFUL session, which can't fail
  // over mid-conversation — so we never call a session companion on the Fallback.
  Brancher().fallback_to(ServerChain())
}

function PinnedSession(brief: string) -> TwoDrafts
    throws baml.errors.SessionError | baml.errors.BranchError | baml.UnknownError {
  // We deliberately pull the FIRST branchable member out of the chain rather
  // than calling .session on the Fallback, because a forked state cannot
  // fail over to a different provider mid-conversation.
  let pinned = Brancher();                                 // explicit pin
  DraftWithAlternative(pinned, brief)
}
