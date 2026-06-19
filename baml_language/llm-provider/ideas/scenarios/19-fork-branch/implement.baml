// =============================================================================
// implement.baml — fork / branch a conversation. Library / std-lib code.
//
// Builds on the canonical primitives in ../_conventions.md (Provider, Tools-
// style provider-owned opaque associated types). The teaching points:
//
//   * A conversation is a `Session` capability: an opaque, provider-OWNED handle
//     plus turn/history ops. This mirrors `Tools.Transcript` — the wire
//     discipline (message array, parent ids, server snapshot ids) stays inside
//     the provider, behind an associated `type Session`.
//   * Branching is a SEPARATE capability `requires Session`: `fork(s, at?)`.
//     Not every Session can branch (OpenAI Agents `Session`, Vercel = "copy a
//     DB row" with no fork API), so it is its own interface, gated by `match`.
//   * The three encodings from background §3 are made REAL inside the provider
//     bodies, all behind the same `fork` seam:
//        - copy-the-prefix   (Claude Agent SDK: new id, transcript = copy)
//        - point-at-parent   (Pi DAG: child node, parent id, prefix SHARED)
//        - emergent-from-ids (OpenAI Responses: fork falls out of reusing a
//                             prior previous_response_id; server owns the bytes)
//   * Fork-at-tail vs fork-anywhere is one parameter: `at: Cursor?`. A provider
//     that can only branch from the head REJECTS a non-tail cursor at runtime —
//     the capability is present but the *position* is the runtime promise.
//   * Server-authoritative state (Responses) is the place the model leaks: you
//     hold ids, not bytes, so `history`/`copy_out` cannot be honest. We flag it.
// =============================================================================


// -----------------------------------------------------------------------------
// NET-NEW host + value surface this scenario needs on top of _conventions.md.
// -----------------------------------------------------------------------------

// A position INSIDE a conversation that you can fork from. It is OPAQUE to the
// app: you obtain one from `Session.cursor_at` / `Session.head`, you never
// construct its innards. Different providers encode it differently —
//   * copy-prefix  -> an index into the prefix array
//   * pointer/DAG  -> a node id in the tree
//   * emergent-ids -> a server response/snapshot id
// ...so the encoding lives in `$rust_type`, exactly like PromptAst / Transcript.
// (net-new type: there is no conversation-position primitive today.)
class Cursor { _data: $rust_type }

// A read-only projection of a conversation so the app can render / inspect it
// WITHOUT owning the wire format. Companion to Session, same move as the
// `History` view in scenario 10. For server-authoritative providers this can be
// PARTIAL or empty (you don't hold the bytes) — that's the leak, made explicit
// by the `complete` flag rather than pretended away.
// (net-new type: a read-only transcript view.)
class Transcript {
  turns: Turn[]
  complete: bool             // false => provider holds bytes we cannot see (server-stored)
}
class Turn {
  role: string               // "user" | "assistant" | "system" | "tool"
  content: string
  id: string?                // provider's per-turn id, when it exposes one (fork-anywhere needs it)
}

// Branching can fail in scenario-specific ways the base LlmClient error doesn't
// name. We refine the typed-throws channel (proposal §1: `type Error`) so the
// `match`/`catch` at the call site can distinguish "can't branch here" from
// "the model call failed".
// (net-new error variants, in the same family as baml.errors.Unsupported.)
class CannotForkHere {
  message: string            // e.g. "this provider only forks from the tail"
  cursor: Cursor
}
class ForkNotOwned {
  message: string            // server-authoritative: branch exists but you can't copy it out
  branch_id: string
}


// -----------------------------------------------------------------------------
// The Session capability: a conversation as provider-owned opaque state.
//
// This is the base every fork-capable provider needs. It is NOT in
// _conventions.md — multi-turn sessions are their own scenario — so it is
// net-new surface, defined here. It deliberately mirrors `Tools`:
//   open  ~ begin       (start a conversation)
//   turn  ~ step+submit (one round-trip, mutating the opaque Session)
//   the Session associated `type` is provider-owned & opaque, like Transcript.
// -----------------------------------------------------------------------------

interface Session requires Provider {
  type Session                                   // provider-owned, opaque (like Transcript)

  // Start a fresh conversation seeded by `prompt` (usually the system turn).
  function open<T>(self, prompt: baml.llm.PromptAst) -> Session throws Error

  // One round-trip: append the user `prompt`, get the model answer, return the
  // NEW session value (sessions are immutable values here; the provider decides
  // whether that means "append to an array" or "advance a server id").
  function turn<T>(self, s: Session, prompt: baml.llm.PromptAst) -> Session throws Error

  // The typed answer the latest turn produced.
  function latest<T>(self, s: Session) -> T throws Error

  // Positions you can fork from. `head` is always available; `cursor_at` lets
  // you address a prior turn by its id (fork-anywhere). A tail-only provider
  // still implements `cursor_at` but its `fork` will reject a non-head cursor.
  function head(self, s: Session) -> Cursor throws Error
  function cursor_at(self, s: Session, turn_id: string) -> Cursor throws Error

  // The read-only view. `complete` tells the truth about server-stored bytes.
  function transcript(self, s: Session) -> Transcript throws Error
}


// -----------------------------------------------------------------------------
// The Branching capability: a SEPARATE interface `requires Session`.
//
// Kept separate on purpose — background §3: "in the OpenAI Agents `Session`
// there is no built-in fork; Vercel = copy a row in your DB." Those are
// `Session` providers that are NOT `Branching`. So branchability is its own
// capability, negotiated by `match`, never assumed.
// -----------------------------------------------------------------------------

interface Branching requires Session {
  // Fork at `at` (or at the tail when `at` is null). Returns a NEW session that
  // shares the prefix up to `at` and diverges after it. HOW it shares — copy,
  // pointer, or reused id — is entirely the provider's business.
  function fork<T>(self, s: Session, at: Cursor?) -> Session throws Error

  // Copy the branch OUT of the provider (export the bytes). Honest only for
  // providers that actually hold the bytes; server-authoritative providers
  // throw ForkNotOwned. Exposed so the app can ask, and get a real error.
  function copy_out<T>(self, s: Session) -> Transcript throws Error
}


// =============================================================================
// Provider #1 — Claude Agent SDK: COPY-THE-PREFIX.
//
// Divergence (background §3): `forkSession` creates a NEW id whose transcript
// begins as a COPY of the parent prefix up to the fork point, then diverges.
// Duplicates bytes. Forks anywhere (`resumeSessionAt`). Bytes are LOCAL — so
// `copy_out` is honest and `transcript.complete` is true.
// =============================================================================

class ClaudeSdkSession {
  model: string
  api_key: string

  implements Provider {
    // single-turn convenience: open a throwaway session, take one turn.
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      $rust_io_function                          // spawn `claude`, one query, no persisted id
    }
  }

  implements Session {
    type Session = ClaudeForkState

    function open<T>(self, prompt: baml.llm.PromptAst) -> ClaudeForkState throws baml.errors.LlmClient {
      // the SDK mints a session id; transcript starts with the system prompt.
      let id = $rust_io_function;                // claude_sdk.new_session() -> string
      ClaudeForkState {
        session_id: id,
        // we keep a LOCAL copy of the transcript: copy-based fork needs the
        // prefix bytes to hand to the new id.
        turns: baml.llm.turns_from(prompt),
        parent_id: null,
      }
    }

    function turn<T>(self, s: ClaudeForkState, prompt: baml.llm.PromptAst) -> ClaudeForkState throws baml.errors.LlmClient {
      let user = baml.llm.turns_from(prompt);
      // resume the existing id, send only the new turn; the SDK appends.
      let answer: string = $rust_io_function;    // claude_sdk.query(resume=s.session_id, prompt)
      ClaudeForkState {
        session_id: s.session_id,
        turns: baml.list.concat(s.turns, baml.list.concat(user, [Turn { role: "assistant", content: answer, id: $rust_io_function }])),
        parent_id: s.parent_id,
      }
    }

    function latest<T>(self, s: ClaudeForkState) -> T throws baml.errors.LlmClient {
      let last = s.turns[-1];                    // negative indexing, BEP
      baml.sap.parse<T>(last.content)
    }

    function head(self, s: ClaudeForkState) -> Cursor throws baml.errors.LlmClient {
      // copy-based: a cursor is an INDEX into the prefix array.
      Cursor { _data: $rust_io_function }        // encode(index = len(turns) - 1)
    }
    function cursor_at(self, s: ClaudeForkState, turn_id: string) -> Cursor throws baml.errors.LlmClient {
      // find the index of `turn_id`; copy-fork can resume at ANY message.
      for (let i in baml.list.indices(s.turns)) {
        if (s.turns[i].id == turn_id) { return Cursor { _data: $rust_io_function }; }  // encode(index=i)
      }
      throw baml.errors.LlmClient { message: "no such turn" }
    }

    function transcript(self, s: ClaudeForkState) -> Transcript throws baml.errors.LlmClient {
      Transcript { turns: s.turns, complete: true }   // bytes are LOCAL
    }
  }

  implements Branching {
    function fork<T>(self, s: ClaudeForkState, at: Cursor?) -> ClaudeForkState throws baml.errors.LlmClient {
      // resolve the fork index: tail when `at` is null, else the cursor's index.
      let idx: int = match (at) {
        let c: Cursor => $rust_io_function,      // decode_index(c) — claude forks ANYWHERE
        _             => baml.list.len(s.turns) - 1,
      };
      // COPY the prefix [0..=idx], mint a new id seeded with that copy.
      let prefix = baml.list.take(s.turns, idx + 1);
      let new_id = $rust_io_function;            // claude_sdk.fork_session(s.session_id, atIndex=idx)
      ClaudeForkState {
        session_id: new_id,                      // NEW id
        turns: prefix,                           // a COPY — duplicated bytes
        parent_id: s.session_id,                 // we remember lineage for our own bookkeeping
      }
    }

    function copy_out<T>(self, s: ClaudeForkState) -> Transcript throws baml.errors.LlmClient {
      Transcript { turns: s.turns, complete: true }   // honest: we own the bytes
    }
  }
}

class ClaudeForkState {
  session_id: string
  turns: Turn[]              // the LOCAL copy — copy-based fork lives or dies on this
  parent_id: string?
}


// =============================================================================
// Provider #2 — Pi (earendil-works): POINT-AT-PARENT (DAG).
//
// Divergence (background §3): sessions are nodes in a DAG with parent ids; a
// child SHARES its prefix with the parent WITHOUT copying. Forking at message 47
// creates a new leaf pointing at node 47. A `SessionManager` owns the tree.
// Strictly cheaper than copy; needs a real tree + GC of dead leaves. Forks
// anywhere. Bytes are local (in the store) -> copy_out is honest but must WALK
// ancestry to materialize the prefix.
// =============================================================================

class PiSession {
  model: string
  api_key: string
  store: NodeStore           // the DAG — net-new host handle (see below)

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      $rust_io_function
    }
  }

  implements Session {
    type Session = PiNode

    function open<T>(self, prompt: baml.llm.PromptAst) -> PiNode throws baml.errors.LlmClient {
      // a root node: no parent. The node stores ONLY this turn's delta — the
      // prefix is reached by walking `parent`.
      let node_id = self.store.put_node($rust_io_function, null);   // put_node(turn, parent=null)
      PiNode { store: self.store, node_id: node_id }
    }

    function turn<T>(self, s: PiNode, prompt: baml.llm.PromptAst) -> PiNode throws baml.errors.LlmClient {
      // to call the model we must materialize the full prefix by walking ancestors.
      let prefix = self.store.walk_ancestry(s.node_id);            // [root .. s] — shared, not copied
      let answer: string = $rust_io_function;                      // model.call(prefix + new user turn)
      // append a NEW child node pointing at s; only the delta is stored.
      let delta = baml.list.concat(baml.llm.turns_from(prompt), [Turn { role: "assistant", content: answer, id: null }]);
      let child = self.store.put_node(delta, s.node_id);           // parent = s.node_id
      PiNode { store: self.store, node_id: child }
    }

    function latest<T>(self, s: PiNode) -> T throws baml.errors.LlmClient {
      let delta = self.store.node_delta(s.node_id);
      baml.sap.parse<T>(delta[-1].content)
    }

    function head(self, s: PiNode) -> Cursor throws baml.errors.LlmClient {
      // pointer-based: a cursor IS a node id. No copy involved.
      Cursor { _data: $rust_io_function }                          // encode(node_id = s.node_id)
    }
    function cursor_at(self, s: PiNode, turn_id: string) -> Cursor throws baml.errors.LlmClient {
      // walk ancestry to find the node whose delta contains turn_id.
      let node = self.store.find_ancestor_with_turn(s.node_id, turn_id);
      Cursor { _data: $rust_io_function }                          // encode(node)
    }

    function transcript(self, s: PiNode) -> Transcript throws baml.errors.LlmClient {
      // materialize by WALKING — the leak the DAG buys back: read is O(depth).
      Transcript { turns: self.store.walk_ancestry(s.node_id), complete: true }
    }
  }

  implements Branching {
    function fork<T>(self, s: PiNode, at: Cursor?) -> PiNode throws baml.errors.LlmClient {
      // resolve the fork node: tail = s itself, else the cursor's node.
      let parent_node: string = match (at) {
        let c: Cursor => $rust_io_function,                        // decode_node(c) — Pi forks ANYWHERE
        _             => s.node_id,
      };
      // POINT at the parent: a new EMPTY leaf whose parent is the fork node.
      // No bytes copied — the prefix is shared via the parent pointer.
      let leaf = self.store.put_node([], parent_node);
      PiNode { store: self.store, node_id: leaf }
    }

    function copy_out<T>(self, s: PiNode) -> Transcript throws baml.errors.LlmClient {
      // honest — bytes are in OUR store — but we must walk to materialize them.
      Transcript { turns: self.store.walk_ancestry(s.node_id), complete: true }
    }
  }
}

class PiNode {
  store: NodeStore
  node_id: string            // the leaf; prefix is reached via parent pointers
}

// The DAG store. Pointer-fork needs a real tree with ancestry walks and GC of
// dead leaves (background §3 "what's hard"). This is net-new host surface — KV
// (proposal §6 `cached(store: KV)`) is flat key->value and cannot express
// parent pointers / ancestry walks. So branching forces a richer store than
// caching ever needed.
class NodeStore {
  _handle: $rust_type
  // each method is a host boundary on the underlying tree:
  function put_node(self, delta: Turn[], parent: string?) -> string throws baml.errors.Io { $rust_io_function }
  function node_delta(self, node: string) -> Turn[] throws baml.errors.Io { $rust_io_function }
  function walk_ancestry(self, node: string) -> Turn[] throws baml.errors.Io { $rust_io_function }       // root..node
  function find_ancestor_with_turn(self, node: string, turn_id: string) -> string throws baml.errors.Io { $rust_io_function }
  function gc(self) -> int throws baml.errors.Io { $rust_io_function }                                    // collect dead leaves
}


// =============================================================================
// Provider #3 — OpenAI Responses: EMERGENT-FROM-IDS (server-authoritative).
//
// Divergence (background §3/§4): each `previous_response_id` names a server
// snapshot; issuing two different follow-ups from the SAME prior id produces two
// branches that share the prefix SERVER-SIDE. You never hold the bytes — you
// hold ids. So:
//   * fork is FREE and does NOTHING but pick which id to chain from next.
//   * fork-anywhere works iff you kept the intermediate response ids.
//   * transcript.complete is FALSE — you cannot see the bytes.
//   * copy_out THROWS ForkNotOwned — the branch point lives at the provider.
// This is where the proposal's model is honest about what it CANNOT promise.
// =============================================================================

class ResponsesSession {
  model: string
  api_key: string

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      $rust_io_function                          // responses.create, no previous_response_id
    }
  }

  implements Session {
    type Session = ResponsesChain

    function open<T>(self, prompt: baml.llm.PromptAst) -> ResponsesChain throws baml.errors.LlmClient {
      // first response; the server stores it. We keep only the id.
      let resp_id: string = $rust_io_function;   // responses.create(input=prompt, store=true).id
      ResponsesChain { current_id: resp_id, known_ids: [resp_id] }
    }

    function turn<T>(self, s: ResponsesChain, prompt: baml.llm.PromptAst) -> ResponsesChain throws baml.errors.LlmClient {
      // send ONLY the new turn, chaining off current_id; server supplies prefix.
      let resp_id: string = $rust_io_function;   // responses.create(input=prompt, previous_response_id=s.current_id)
      ResponsesChain {
        current_id: resp_id,
        known_ids: baml.list.concat(s.known_ids, [resp_id]),   // remember ids -> enables fork-anywhere
      }
    }

    function latest<T>(self, s: ResponsesChain) -> T throws baml.errors.LlmClient {
      let text: string = $rust_io_function;      // responses.get(s.current_id).output_text
      baml.sap.parse<T>(text)
    }

    function head(self, s: ResponsesChain) -> Cursor throws baml.errors.LlmClient {
      Cursor { _data: $rust_io_function }        // encode(response_id = s.current_id)
    }
    function cursor_at(self, s: ResponsesChain, turn_id: string) -> Cursor throws baml.errors.LlmClient {
      // fork-anywhere ONLY if we recorded that id. The server won't list them.
      for (let id in s.known_ids) {
        if (id == turn_id) { return Cursor { _data: $rust_io_function }; }   // encode(id)
      }
      throw baml.errors.LlmClient { message: "unknown response id (server won't enumerate the chain)" }
    }

    function transcript(self, s: ResponsesChain) -> Transcript throws baml.errors.LlmClient {
      // THE LEAK: we hold ids, not bytes. We can name the chain but not show it.
      Transcript { turns: [], complete: false }  // complete=false => "ask the server, we can't"
    }
  }

  implements Branching {
    function fork<T>(self, s: ResponsesChain, at: Cursor?) -> ResponsesChain throws baml.errors.LlmClient {
      // EMERGENT: forking is just choosing which prior id to chain the next turn
      // from. We copy NOTHING and call NOTHING — branching falls out of the id.
      let from_id: string = match (at) {
        let c: Cursor => $rust_io_function,      // decode_id(c)
        _             => s.current_id,
      };
      // a "branch" is a chain whose head is `from_id`. The next `turn` will use
      // it as previous_response_id, diverging from siblings server-side.
      ResponsesChain { current_id: from_id, known_ids: s.known_ids }
    }

    function copy_out<T>(self, s: ResponsesChain) -> Transcript throws baml.errors.LlmClient {
      // we CANNOT honor this — the bytes are server-authoritative. Say so plainly
      // instead of returning an empty transcript that lies.
      throw ForkNotOwned {
        message: "Responses branch points live at the provider; bytes cannot be copied out",
        branch_id: s.current_id,
      }
    }
  }
}

class ResponsesChain {
  current_id: string         // the head we chain the next turn from
  known_ids: string[]        // ids WE happened to keep — the only fork-anywhere we get
}
