// =============================================================================
// implement.baml — Server-stored chains + the three ownership models
//
// Library / std-lib author code. Builds on the canonical primitives from
// _conventions.md (Provider, HttpProvider, Streaming, Tools). The teaching
// point of this scenario is OWNERSHIP: who holds the conversation, and how that
// changes the way you formulate every request.
//
// Three ownership models exist (background §5):
//   - client-held       : the client resends the full array every turn
//   - server-session    : a live socket holds the list (covered by Realtime)
//   - server-stored-by-id: the provider holds the chain; you pass a HANDLE
//
// This file makes "server-stored-by-id" expressible WITHOUT breaking the
// stateless `call<T>(prompt) -> T` shape of the base Provider, and shows that
// ownership is a property of the PROVIDER + TRANSCRIPT, not of the prompt.
// =============================================================================


// -----------------------------------------------------------------------------
// 0. Net-new: a Chain capability. A provider that can hold history server-side,
//    addressable by an opaque handle. This is the irreducible new surface this
//    scenario forces — the base Provider is single-shot and stateless, so the
//    "keep going from where we left off" verb has nowhere to live otherwise.
//
//    The handle is OPAQUE to the application exactly like Tools.Transcript: the
//    app threads it turn to turn and never interprets it. Ownership lives here.
// -----------------------------------------------------------------------------

// The shape of the handle the application holds. It is NOT the bytes of history;
// it is a pointer to provider-owned state, plus enough metadata to know what
// kind of state it is (so we can refuse to migrate it — see §5 below).
class ChainHandle {
  // The provider-owned id. For OpenAI Responses this is `previous_response_id`
  // (an implicit turn-to-turn chain) OR a `conversations` resource id (an
  // explicit named thread). The `kind` discriminates which.
  id: string
  kind: ChainKind
  // Identity of the provider class that owns this state, e.g. "OpenAIResponses".
  // A handle minted by one provider is meaningless to another — we check this on
  // continue() and refuse cross-provider resumption ("state you cannot migrate").
  owner: string
}

enum ChainKind {
  // `previous_response_id`: each id names a snapshot; two follow-ups from the
  // same id branch the thread server-side.
  ResponseId
  // a `conversations` resource: a named object many responses attach to.
  Conversation
}

interface Chain requires Provider {
  // Start a server-stored thread. Returns the first answer AND the handle that
  // names the (now server-held) context after this turn.
  function open<T>(self, prompt: baml.llm.PromptAst) -> ChainTurn<T> throws Error

  // Continue from a handle. The application sends ONLY the new turn; the provider
  // supplies all prior context from its stored chain. This is the whole point:
  // pass a handle, not the bytes.
  function continue<T>(self, prior: ChainHandle, prompt: baml.llm.PromptAst)
      -> ChainTurn<T> throws Error

  // Reasoning-continuity is the reason this capability exists: for reasoning
  // models the encrypted reasoning items carry forward across turns inside the
  // server-stored chain. A provider that stores the chain but DROPS reasoning is
  // a strictly weaker thing; we surface that as a static fact on the provider so
  // a caller can require it (see OpenAIResponses.preserves_reasoning).
  function preserves_reasoning(self) -> bool

  // A server-stored chain is provider-owned. You may explicitly tear it down,
  // but you cannot move it. Returns whether the provider actually deleted it
  // (store:false chains, or already-evicted ids, may have nothing to delete).
  function forget(self, handle: ChainHandle) -> bool throws Error
}

// The result of a chained turn: the typed answer plus the new handle.
class ChainTurn<T> {
  value: T
  next: ChainHandle
}


// -----------------------------------------------------------------------------
// 1. OpenAIResponses — server-stored-by-id. The flagship of this ownership model.
//
//    It is BOTH a plain HttpProvider (stateless `.call`, store:false) AND a
//    Chain provider (stateful, handle-threaded). The single `build_request`
//    encodes the divergence: presence/absence of a prior handle, and which of
//    the two handle shapes (previous_response_id vs conversation) is in play.
// -----------------------------------------------------------------------------

class OpenAIResponses {
  model: string
  api_key: string
  // When true (the API default), the server stores each response so it can be
  // chained. When false, .call works but .open/.continue cannot chain.
  store: bool
  // Reasoning models (o-series, gpt-5) carry encrypted reasoning items forward
  // ONLY when the chain is server-stored. We record the model family so
  // preserves_reasoning() can answer truthfully.
  reasoning_model: bool

  function default(model: string) -> OpenAIResponses {
    OpenAIResponses {
      model: model,
      api_key: env.OPENAI_API_KEY,
      store: true,
      reasoning_model: baml.str.starts_with(model, "o") || baml.str.starts_with(model, "gpt-5"),
    }
  }

  // ---- the stateless face: a normal chat model over /v1/responses ----
  implements HttpProvider {
    type Body = baml.http.Response

    // ONE build_request, parameterized by an optional prior handle. This is
    // where the ownership divergence physically lives. `prior` is internal — the
    // public capability methods (call/open/continue) all funnel through here.
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      self.build_with_chain<T>(prompt, null)
    }

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "responses http failed" }; } }
    }

    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      // The Responses API nests the assistant text; SAP-parse the structured slot.
      baml.sap.parse<T>(from.json_path("output.0.content.0.text"))
    }
  }

  // Shared request builder. `prior` carries the ownership decision:
  //   prior == null                         -> a fresh, possibly-stored turn
  //   prior.kind == ResponseId              -> previous_response_id chaining
  //   prior.kind == Conversation            -> attach to a conversations resource
  // This is the "the three ownership models change how you formulate every
  // request" claim made concrete: the SAME logical turn produces three
  // different request bodies.
  function build_with_chain<T>(self, prompt: baml.llm.PromptAst, prior: ChainHandle?)
      -> baml.http.Request throws baml.errors.LlmClient {
    let schema = reflect.type_of<T>();
    // Only the NEW turn's input is rendered. We never serialize prior history —
    // that is exactly the byte-savings + the point of server-stored chains.
    let new_input = baml.llm.render_user_turn(prompt);

    let chain_fields = match (prior) {
      null => {
        // No prior handle. `store` decides whether THIS response becomes chainable.
        baml.json.object([("store", baml.json.bool(self.store))])
      },
      let h: ChainHandle => {
        // Defensive: a handle from another provider class is not interpretable.
        if (h.owner != "OpenAIResponses") {
          throw baml.errors.LlmClient {
            message: baml.str.concat("cannot continue a chain owned by ", h.owner, " with OpenAIResponses"),
          };
        }
        match (h.kind) {
          ChainKind.ResponseId => baml.json.object([
            ("previous_response_id", baml.json.string(h.id)),
            ("store", baml.json.bool(self.store)),
          ]),
          ChainKind.Conversation => baml.json.object([
            ("conversation", baml.json.string(h.id)),
            ("store", baml.json.bool(self.store)),
          ]),
        }
      },
    };

    let body = baml.json.merge(
      baml.json.object([
        ("model", baml.json.string(self.model)),
        ("input", new_input),
        ("response_format", baml.llm.response_schema_json(schema)),
      ]),
      chain_fields,
    );

    baml.http.Request {
      method: "POST",
      url: "https://api.openai.com/v1/responses",
      headers: baml.http.bearer(self.api_key),
      body: baml.json.encode(body),
    }
  }

  // ---- the stateful face: server-stored chains ----
  implements Chain {
    function open<T>(self, prompt: baml.llm.PromptAst) -> ChainTurn<T> throws baml.errors.LlmClient {
      if (!self.store) {
        throw baml.errors.LlmClient { message: "open() requires store:true; this provider is configured stateless" };
      }
      let req = self.build_with_chain<T>(prompt, null);
      let resp = baml.http.send(req) catch (e) { _ => { throw baml.errors.LlmClient { message: "responses open failed" }; } };
      self.lift_turn<T>(resp, ChainKind.ResponseId)
    }

    function continue<T>(self, prior: ChainHandle, prompt: baml.llm.PromptAst)
        -> ChainTurn<T> throws baml.errors.LlmClient {
      // build_with_chain already refuses foreign-owner handles and picks the
      // right wire field for ResponseId vs Conversation. We just resend the new
      // turn; the server reconstructs everything before it.
      let req = self.build_with_chain<T>(prompt, prior);
      let resp = baml.http.send(req) catch (e) { _ => { throw baml.errors.LlmClient { message: "responses continue failed" }; } };
      // The new handle's kind matches the prior's kind: continuing a conversation
      // yields a conversation handle, continuing a response-id yields a response-id.
      self.lift_turn<T>(resp, prior.kind)
    }

    function preserves_reasoning(self) -> bool {
      // True iff this is a reasoning model AND the chain is actually stored.
      // Drop either and the encrypted reasoning items are lost between turns.
      self.reasoning_model && self.store
    }

    function forget(self, handle: ChainHandle) -> bool throws baml.errors.LlmClient {
      if (handle.owner != "OpenAIResponses") {
        throw baml.errors.LlmClient { message: "cannot forget a chain this provider does not own" };
      }
      let url = match (handle.kind) {
        ChainKind.ResponseId   => baml.str.concat("https://api.openai.com/v1/responses/", handle.id),
        ChainKind.Conversation => baml.str.concat("https://api.openai.com/v1/conversations/", handle.id),
      };
      let resp = baml.http.send(baml.http.Request {
        method: "DELETE", url: url, headers: baml.http.bearer(self.api_key), body: "",
      }) catch (e) { _ => { throw baml.errors.LlmClient { message: "delete failed" }; } };
      resp.ok()
    }

    // Lift an HTTP response into (value, next-handle). The id the server returns
    // becomes the handle for the NEXT turn — this is how branching falls out: two
    // continue() calls passing the same prior handle fork the thread server-side.
    function lift_turn<T>(self, resp: baml.http.Response, kind: ChainKind) -> ChainTurn<T> throws baml.errors.LlmClient {
      let value: T = baml.sap.parse<T>(resp.json_path("output.0.content.0.text"));
      let new_id = match (kind) {
        ChainKind.ResponseId   => resp.json_path("id"),               // the response id
        ChainKind.Conversation => resp.json_path("conversation.id"),  // the resource id is stable
      };
      ChainTurn<T> {
        value: value,
        next: ChainHandle { id: new_id, kind: kind, owner: "OpenAIResponses" },
      }
    }
  }
}


// -----------------------------------------------------------------------------
// 2. Anthropic — client-held only. The control case. It has NO server-stored
//    equivalent: you always resend the array. It does NOT implement Chain. Any
//    attempt to .open/.continue against it is a compile-error if you took the
//    concrete escape hatch, or a runtime "not a Chain" if you went through sugar.
//
//    We still give it a small client-held session helper so the contrast between
//    the two ownership models is visible side by side in usage.baml.
// -----------------------------------------------------------------------------

class Anthropic {
  model: string
  api_key: string
  max_tokens: int

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // Anthropic resends the FULL message array every call. There is no handle.
      let schema = reflect.type_of<T>();
      baml.http.Request {
        method: "POST",
        url: "https://api.anthropic.com/v1/messages",
        headers: baml.http.anthropic_headers(self.api_key),
        body: baml.json.encode(baml.json.object([
          ("model", baml.json.string(self.model)),
          ("max_tokens", baml.json.int(self.max_tokens)),
          // The whole transcript is serialized here, every turn. This is the cost
          // server-stored chains eliminate — and the durability Anthropic gives
          // you in exchange (you own the bytes; nothing to migrate).
          ("messages", baml.llm.render_messages(prompt)),
          ("tools", baml.llm.forced_schema_tool(schema)),  // Anthropic encodes structured output as a forced tool
        ])),
      }
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "anthropic http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.json_path("content.0.input"))  // the forced-tool input is the structured value
    }
  }

  // NOTE: deliberately NO `implements Chain`. Anthropic is client-held. The
  // application owns history. There is nothing for `continue(handle, ...)` to do.
}


// -----------------------------------------------------------------------------
// 3. A client-held session helper (ownership = client). This is the OTHER half
//    of the ownership story. It is NOT a Chain — it accumulates bytes locally and
//    resends them. It works against ANY Provider (Anthropic, OpenAIResponses with
//    store:false, anything), precisely because it holds the history itself.
//
//    It is a plain value the app threads, not a provider. Putting both this and a
//    Chain in the same flow is the "double-counting" hazard from background §4 —
//    see the comment in usage.baml and the guard below.
// -----------------------------------------------------------------------------

class ClientHeldSession {
  // The locally-owned transcript. The app owns these bytes; durable iff the app
  // persists them. This is the entire history, in the client's process.
  turns: baml.llm.Message[]

  function empty() -> ClientHeldSession { ClientHeldSession { turns: [] } }

  // Append a user turn, ask the provider, append the assistant turn, return the
  // grown session plus the answer. The provider sees the WHOLE array every time.
  function ask<T>(self, p: Provider, user: string) -> SessionTurn<T> throws baml.errors.LlmClient {
    // GUARD against double-counting: a client-held session must not be layered on
    // a provider that ALSO stores the chain server-side, or context is counted
    // twice (background §4 — the OpenAI Agents SDK warns about exactly this).
    match (p) {
      let c: Chain => {
        // A Chain provider whose store is on would double-count. We can't read
        // arbitrary fields off the existential, but the capability itself is the
        // tell: if a provider is a Chain, the app should thread its handle, not
        // wrap it in a client-held session.
        throw baml.errors.LlmClient {
          message: "double-counting: this provider holds the chain server-side; use .open/.continue, not a ClientHeldSession",
        };
      },
      _ => {},
    }
    let with_user = baml.arr.push(self.turns, baml.llm.user_message(user));
    let answer: T = p.call<T>(baml.llm.from_messages(with_user));
    let grown = baml.arr.push(with_user, baml.llm.assistant_message_of(answer));
    SessionTurn<T> { value: answer, session: ClientHeldSession { turns: grown } }
  }
}

class SessionTurn<T> {
  value: T
  session: ClientHeldSession
}


// -----------------------------------------------------------------------------
// 4. Combinator interaction: a Chain provider still gets Retry/Fallback for free
//    on its `.call` face, but the chain-threaded methods are NOT forwarded by the
//    generic combinators (Fallback can't fall back across providers mid-chain —
//    the handle is owner-bound). We make that explicit with a Chain-aware retry
//    that re-threads the SAME handle (idempotent: continuing from the same prior
//    id is safe and, in fact, branches rather than corrupts).
// -----------------------------------------------------------------------------

class ChainRetry {
  inner: Chain
  max: int

  // Not a Provider combinator — a Chain combinator. It wraps a Chain and retries
  // continue() against the SAME prior handle. Because each continue from a fixed
  // prior id branches server-side, a retried turn is a fresh branch, never a
  // double-append. That is only safe BECAUSE the server owns the chain.
  function continue_retrying<T>(self, prior: ChainHandle, prompt: baml.llm.PromptAst)
      -> ChainTurn<T> throws baml.errors.LlmClient {
    let attempt = 0;
    while (attempt < self.max) {
      let r: ChainTurn<T> = self.inner.continue<T>(prior, prompt) catch (e) {
        _ => { attempt = attempt + 1; continue; }
      };
      return r;
    }
    throw baml.errors.LlmClient { message: "chain continue exhausted retries" }
  }
}


// -----------------------------------------------------------------------------
// Host boundaries used above (all already implied by _conventions.md, declared
// here for completeness — these are the genuine $rust seams, nothing more).
// -----------------------------------------------------------------------------
// baml.llm.render_user_turn(prompt) -> JsonValue        // just the new turn
// baml.llm.render_messages(prompt)  -> JsonValue        // the full array (client-held)
// baml.llm.render_user_turn / response_schema_json / forced_schema_tool : prompt codecs
// baml.llm.user_message / assistant_message_of / from_messages / Message : transcript values
// baml.json.{object,merge,encode,string,int,bool}      // JSON builder
// baml.http.{bearer,anthropic_headers}                  // header helpers
// baml.str.{starts_with,concat} ; baml.arr.push         // small std helpers
