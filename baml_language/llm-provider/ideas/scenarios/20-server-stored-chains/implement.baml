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

// Net-new capability-error interface. `Chain` is a capability beyond
// call/stream/realtime/tools (server-owned, handle-addressed conversation
// state), so per the error model it gets its own channel rather than borrowing
// CallError. Its classifiers triage the failures a chain turn actually has:
// transport drops, rate limits, and a chain-specific "the handle is gone /
// owned by someone else" condition the app must branch on.
interface baml.errors.ChainError {
  function is_network_error(self) -> bool
  function is_rate_limit(self) -> bool
  // True when the failure is about the HANDLE itself (foreign owner, evicted id,
  // store:false), not the transport or the model.
  function is_handle_error(self) -> bool
}

// NET-NEW REQUIREMENT on a spec type: because `Chain` invents a new capability
// channel, `baml.errors.Unsupported` (the capability-negotiation error a
// companion's `_` arm throws) must ALSO implement `ChainError` to be legal on
// this channel — its canonical definition in _conventions.md only covers
// Call/Stream/Tool/Realtime. usage.baml throws `baml.errors.Unsupported` on the
// ChainError channel (the `_` arms of the `match (p) { let c: Chain => ... }`),
// so the spec's `Unsupported` is extended here with the chain channel. Like the
// others, every classifier is false: an unsupported provider has no handle error.
extend baml.errors.Unsupported {
  implements baml.errors.ChainError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_handle_error(self)  -> bool { return false; }
  }
}

interface Chain requires Provider {
  // Start a server-stored thread. Returns the first answer AND the handle that
  // names the (now server-held) context after this turn.
  function open<T>(self, prompt: baml.llm.PromptAst) -> ChainTurn<T>
      throws baml.ExtendUnknownError<baml.errors.ChainError>

  // Continue from a handle. The application sends ONLY the new turn; the provider
  // supplies all prior context from its stored chain. This is the whole point:
  // pass a handle, not the bytes.
  function continue<T>(self, prior: ChainHandle, prompt: baml.llm.PromptAst)
      -> ChainTurn<T> throws baml.ExtendUnknownError<baml.errors.ChainError>

  // Reasoning-continuity is the reason this capability exists: for reasoning
  // models the encrypted reasoning items carry forward across turns inside the
  // server-stored chain. A provider that stores the chain but DROPS reasoning is
  // a strictly weaker thing; we surface that as a static fact on the provider so
  // a caller can require it (see OpenAIResponses.preserves_reasoning).
  function preserves_reasoning(self) -> bool

  // A server-stored chain is provider-owned. You may explicitly tear it down,
  // but you cannot move it. Returns whether the provider actually deleted it
  // (store:false chains, or already-evicted ids, may have nothing to delete).
  function forget(self, handle: ChainHandle) -> bool
      throws baml.ExtendUnknownError<baml.errors.ChainError>
}

// A handle that names a chain this provider cannot interpret — minted by another
// provider class. This is THE failure server-stored chains add: "state you cannot
// migrate." It is a domain error the app branches on, not a transport blip, so it
// is a concrete ChainError with is_handle_error -> true.
class ForeignChainHandle {
  owner: string
  expected: string
  implements baml.errors.ChainError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_handle_error(self)  -> bool { return true; }
  }
}

// The chain is not stored server-side (store:false), so there is nothing to open
// or continue. Also a handle-shaped failure: the app misconfigured ownership.
class ChainNotStored {
  message: string
  implements baml.errors.ChainError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_handle_error(self)  -> bool { return true; }
  }
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
      reasoning_model: model.starts_with("o") || model.starts_with("gpt-5"),
    }
  }

  // ---- the stateless face: a normal chat model over /v1/responses ----
  implements HttpProvider {
    type Body = baml.http.Response

    // ONE build_request, parameterized by an optional prior handle. This is
    // where the ownership divergence physically lives. `prior` is internal — the
    // public capability methods (call/open/continue) all funnel through here.
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // build_with_chain<T> can throw a ChainError (foreign-owner handle); on the
      // plain call path there is no prior handle, so only CallError-shaped failures
      // surface. Normalize anything foreign through UnknownError.
      self.build_with_chain<T>(prompt, null)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai responses build_request failed");
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai responses http failed"); }
    }

    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // The Responses API nests the assistant text; SAP-parse the structured slot.
      baml.sap.parse<T>(self.assistant_text(from))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai responses parse failed");
    }
  }

  // Navigate the Responses envelope to the assistant text slot
  // (`output[0].content[0].text`). Replaces the invented `Response.json_path`:
  // parse the body to a `json` value and walk the `map`/`json[]` union arms.
  function assistant_text(self, from: baml.http.Response) -> string
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    let root = baml.json.parse(from.text());
    match (root) {
      let obj: map<string, json> => match (obj.get("output")) {
        let output: json[] => match (output.at(0)) {
          let first: map<string, json> => match (first.get("content")) {
            let content: json[] => match (content.at(0)) {
              let part: map<string, json> => match (part.get("text")) {
                let text: string => text,
                _ => "",
              },
              _ => "",
            },
            _ => "",
          },
          _ => "",
        },
        _ => "",
      },
      _ => "",
    }
  } catch (e) {
    _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai responses assistant_text failed");
  }

  // Shared request builder. `prior` carries the ownership decision:
  //   prior == null                         -> a fresh, possibly-stored turn
  //   prior.kind == ResponseId              -> previous_response_id chaining
  //   prior.kind == Conversation            -> attach to a conversations resource
  // This is the "the three ownership models change how you formulate every
  // request" claim made concrete: the SAME logical turn produces three
  // different request bodies.
  // Channel: ChainError — its one deliberate domain throw is the foreign-owner
  // handle (a ChainError). On the plain `.call` path (prior == null) it cannot
  // reach that throw, and build_request normalizes its ChainError|UnknownError
  // result onto the CallError channel via with_message.
  function build_with_chain<T>(self, prompt: baml.llm.PromptAst, prior: ChainHandle?)
      -> baml.http.Request throws baml.ExtendUnknownError<baml.errors.ChainError> {
    let schema = reflect.type_of<T>();
    // Only the NEW turn's input is rendered. We never serialize prior history —
    // that is exactly the byte-savings + the point of server-stored chains.
    let new_input: json = baml.llm.render_user_turn(prompt);

    // Build the body as a native `map<string, json>`, starting with the fields
    // every turn shares, then adding the chain-ownership fields. `store` decides
    // whether THIS response becomes chainable.
    let body: map<string, json> = {
      "model": self.model,
      "input": new_input,
      "response_format": baml.llm.response_schema_json(schema),
      "store": self.store,
    };

    match (prior) {
      null => {},   // No prior handle — nothing more to add.
      let h: ChainHandle => {
        // Defensive: a handle from another provider class is not interpretable.
        if (h.owner != "OpenAIResponses") {
          throw ForeignChainHandle { owner: h.owner, expected: "OpenAIResponses" };
        }
        match (h.kind) {
          ChainKind.ResponseId   => { body.set("previous_response_id", h.id); },
          ChainKind.Conversation => { body.set("conversation", h.id); },
        }
      },
    };

    baml.http.Request {
      method: "POST",
      url: "https://api.openai.com/v1/responses",
      headers: { "Authorization": "Bearer " + self.api_key },
      body: baml.json.stringify(body),
    }
  }

  // ---- the stateful face: server-stored chains ----
  implements Chain {
    function open<T>(self, prompt: baml.llm.PromptAst) -> ChainTurn<T>
        throws baml.ExtendUnknownError<baml.errors.ChainError> {
      if (!self.store) {
        throw ChainNotStored { message: "open() requires store:true; this provider is configured stateless" };
      }
      let req = self.build_with_chain<T>(prompt, null);
      let resp = baml.http.send(req);
      self.lift_turn<T>(resp, ChainKind.ResponseId)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.ChainError>(e, "openai responses open failed");
    }

    function continue<T>(self, prior: ChainHandle, prompt: baml.llm.PromptAst)
        -> ChainTurn<T> throws baml.ExtendUnknownError<baml.errors.ChainError> {
      // build_with_chain already refuses foreign-owner handles and picks the
      // right wire field for ResponseId vs Conversation. We just resend the new
      // turn; the server reconstructs everything before it.
      let req = self.build_with_chain<T>(prompt, prior);
      let resp = baml.http.send(req);
      // The new handle's kind matches the prior's kind: continuing a conversation
      // yields a conversation handle, continuing a response-id yields a response-id.
      self.lift_turn<T>(resp, prior.kind)
    } catch (e) {
      // A ForeignChainHandle thrown by build_with_chain passes through unchanged
      // (it is a known ChainError); only foreign transport failures get wrapped.
      _ => throw baml.UnknownError.with_message<baml.errors.ChainError>(e, "openai responses continue failed");
    }

    function preserves_reasoning(self) -> bool {
      // True iff this is a reasoning model AND the chain is actually stored.
      // Drop either and the encrypted reasoning items are lost between turns.
      self.reasoning_model && self.store
    }

    function forget(self, handle: ChainHandle) -> bool
        throws baml.ExtendUnknownError<baml.errors.ChainError> {
      if (handle.owner != "OpenAIResponses") {
        throw ForeignChainHandle { owner: handle.owner, expected: "OpenAIResponses" };
      }
      let url = match (handle.kind) {
        ChainKind.ResponseId   => "https://api.openai.com/v1/responses/" + handle.id,
        ChainKind.Conversation => "https://api.openai.com/v1/conversations/" + handle.id,
      };
      let resp = baml.http.send(baml.http.Request {
        method: "DELETE", url: url, headers: { "Authorization": "Bearer " + self.api_key }, body: "",
      });
      resp.ok()
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.ChainError>(e, "openai responses forget failed");
    }

    // Lift an HTTP response into (value, next-handle). The id the server returns
    // becomes the handle for the NEXT turn — this is how branching falls out: two
    // continue() calls passing the same prior handle fork the thread server-side.
    function lift_turn<T>(self, resp: baml.http.Response, kind: ChainKind) -> ChainTurn<T>
        throws baml.ExtendUnknownError<baml.errors.ChainError> {
      let value: T = baml.sap.parse<T>(self.assistant_text(resp));
      let root = baml.json.parse(resp.text());
      let new_id = match (root) {
        let obj: map<string, json> => match (kind) {
          ChainKind.ResponseId   => match (obj.get("id")) {            // the response id
            let s: string => s,
            _ => "",
          },
          ChainKind.Conversation => match (obj.get("conversation")) {  // the resource id is stable
            let conv: map<string, json> => match (conv.get("id")) {
              let s: string => s,
              _ => "",
            },
            _ => "",
          },
        },
        _ => "",
      };
      ChainTurn<T> {
        value: value,
        next: ChainHandle { id: new_id, kind: kind, owner: "OpenAIResponses" },
      }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.ChainError>(e, "openai responses lift_turn failed");
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
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Anthropic resends the FULL message array every call. There is no handle.
      let schema = reflect.type_of<T>();
      let body: map<string, json> = {
        "model": self.model,
        "max_tokens": self.max_tokens,
        // The whole transcript is serialized here, every turn. This is the cost
        // server-stored chains eliminate — and the durability Anthropic gives
        // you in exchange (you own the bytes; nothing to migrate).
        "messages": baml.llm.render_messages(prompt),
        "tools": baml.llm.forced_schema_tool(schema),  // Anthropic encodes structured output as a forced tool
      };
      baml.http.Request {
        method: "POST",
        url: "https://api.anthropic.com/v1/messages",
        headers: {
          "x-api-key": self.api_key,
          "anthropic-version": "2023-06-01",
          "content-type": "application/json",
        },
        body: baml.json.stringify(body),
      }
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic http failed"); }
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // The forced-tool input (`content[0].input`) is the structured value. Walk
      // the `json` union to that slot, then SAP-parse its serialized form.
      let root = baml.json.parse(from.text());
      let input: json = match (root) {
        let obj: map<string, json> => match (obj.get("content")) {
          let content: json[] => match (content.at(0)) {
            let part: map<string, json> => match (part.get("input")) {
              let v: json => v,
              _ => null,
            },
            _ => null,
          },
          _ => null,
        },
        _ => null,
      };
      baml.sap.parse<T>(baml.json.stringify(input))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic parse failed");
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

// Layering a client-held session over a provider that ALSO stores the chain
// server-side counts context twice. It is a config-level domain error; it can
// surface from a plain `.call`-shaped helper (CallError) AND from chain-aware code
// (ChainError), so it implements both — caught concretely either way.
class DoubleCountedChain {
  message: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
  implements baml.errors.ChainError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_handle_error(self)  -> bool { return true; }
  }
}

class ClientHeldSession {
  // The locally-owned transcript. The app owns these bytes; durable iff the app
  // persists them. This is the entire history, in the client's process.
  turns: baml.llm.Message[]

  function empty() -> ClientHeldSession { ClientHeldSession { turns: [] } }

  // Append a user turn, ask the provider, append the assistant turn, return the
  // grown session plus the answer. The provider sees the WHOLE array every time.
  // Channel: CallError — the body's only foreign call is `p.call<T>` (CallError).
  // Its one deliberate domain throw, DoubleCountedChain, implements BOTH CallError
  // and ChainError so it is legal here and recognizable to a chain-aware caller.
  function ask<T>(self, p: Provider, user: string) -> SessionTurn<T>
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    // GUARD against double-counting: a client-held session must not be layered on
    // a provider that ALSO stores the chain server-side, or context is counted
    // twice (background §4 — the OpenAI Agents SDK warns about exactly this).
    match (p) {
      let c: Chain => {
        // A Chain provider whose store is on would double-count. We can't read
        // arbitrary fields off the existential, but the capability itself is the
        // tell: if a provider is a Chain, the app should thread its handle, not
        // wrap it in a client-held session.
        throw DoubleCountedChain {
          message: "double-counting: this provider holds the chain server-side; use .open/.continue, not a ClientHeldSession",
        };
      },
      _ => {},
    }
    let with_user = self.turns.concat([baml.llm.user_message(user)]);
    let answer: T = p.call<T>(baml.llm.from_messages(with_user));
    let grown = with_user.concat([baml.llm.assistant_message_of(answer)]);
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

// Default failure when every retried continue() failed for a reason we didn't
// keep (or there were zero attempts). A retry budget ran out — not transport,
// not a handle problem per se, so all classifiers are false.
class ChainExhausted {
  attempts: int
  implements baml.errors.ChainError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_handle_error(self)  -> bool { return false; }
  }
}

class ChainRetry {
  inner: Chain
  max: int

  // Not a Provider combinator — a Chain combinator. It wraps a Chain and retries
  // continue() against the SAME prior handle. Because each continue from a fixed
  // prior id branches server-side, a retried turn is a fresh branch, never a
  // double-append. That is only safe BECAUSE the server owns the chain.
  function continue_retrying<T>(self, prior: ChainHandle, prompt: baml.llm.PromptAst)
      -> ChainTurn<T> throws baml.ExtendUnknownError<baml.errors.ChainError> {
    let attempt = 0;
    let last: baml.errors.ChainError | baml.UnknownError = ChainExhausted { attempts: self.max };
    while (attempt < self.max) {
      let r: ChainTurn<T> = self.inner.continue<T>(prior, prompt) catch (e) {
        // Remember the last failure so an exhausted retry reports WHY, not a
        // generic message. continue<T>'s channel is exactly ChainError|UnknownError.
        _ => { last = e; attempt = attempt + 1; continue; }
      };
      return r;
    }
    throw last
  }
}


// -----------------------------------------------------------------------------
// Host boundaries used above (all already implied by _conventions.md, declared
// here for completeness — these are the genuine $rust seams, nothing more).
// -----------------------------------------------------------------------------
// baml.llm.render_user_turn(prompt) -> json             // just the new turn
// baml.llm.render_messages(prompt)  -> json             // the full array (client-held)
// baml.llm.render_user_turn / response_schema_json / forced_schema_tool : prompt codecs
// baml.llm.user_message / assistant_message_of / from_messages / Message : transcript values
//
// Everything else is the REAL stdlib: request bodies are built as native
// `map<string, json>` literals + `baml.json.stringify`; responses are read with
// `baml.json.parse(resp.text())` + `map`/`json[]` navigation; headers are inline
// `map<string, string>` literals; `string.starts_with` / `+` concat and
// `Array.concat` are methods.
