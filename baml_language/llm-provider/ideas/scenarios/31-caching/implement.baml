// =============================================================================
// Caching — three shapes behind one idea  (library / std-lib author code)
// =============================================================================
//
// There are FOUR distinct things called "caching" here, and the whole teaching
// point of this scenario is that the proposal does NOT let us unify them under
// one flag:
//
//   (A) Client-side response cache — the spine's `Cache` combinator. A KV store
//       in front of `.call`: same prompt -> skip the model entirely. This is the
//       only one that is provider-agnostic, because it never touches the wire.
//
//   (B) Anthropic inline annotation (`cache_control`) — a per-content-block
//       breakpoint. NOT a combinator. It is a property of how *this provider*
//       renders its request body, so it lives inside `build_request`. Content-
//       keyed, no handle, no lifecycle.
//
//   (C) OpenAI automatic prefix cache + routing hint (`prompt_cache_key`) — the
//       provider caches stable prefixes for you. The only knob is an optional
//       string. Also a `build_request` concern: it's just one more field on the
//       provider that gets emitted into the body.
//
//   (D) Gemini explicit resource (`cachedContents`) — a server-authoritative
//       object with create / get / update-TTL / delete and a STORAGE BILL. This
//       one does not fit the request-codec model at all: someone has to own the
//       handle's lifecycle across calls. This is the hard case, and it forces a
//       net-new capability interface + a host-backed resource store.
//
// (A) reuses the spine verbatim. (B) and (C) are pure provider-field / body
// concerns and need nothing new. (D) is where the proposal has to grow.

// -----------------------------------------------------------------------------
// (A) The client-side response cache — reused from the spine, shown for context.
// -----------------------------------------------------------------------------
// `Provider.cached(store)` (a default method on Provider) returns this. It is a
// plain non-generic combinator, exactly like Fallback/Retry. It caches the typed
// RESULT keyed on the rendered prompt — it is NOT prompt caching; it skips the
// model. Useful for deterministic/idempotent calls (evals, fixtures, replay).

interface KV {
  function get(self, key: string) -> string? throws baml.errors.Io
  function put(self, key: string, value: string, ttl_secs: int) -> null throws baml.errors.Io
}

class Cache {
  inner: Provider
  store: KV
  ttl_secs: int

  // Cache is a request/response COMBINATOR: it wraps an HttpProvider and serves
  // call/call_with from a KV store. Under the marker model those interactions
  // live on `HttpProvider`, so Cache implements HttpProvider (not the bare marker).
  // The combinator factories (with_retry/fallback_to/cached) are inherited via
  // `requires Provider`.
  implements HttpProvider {
    type Body = unknown

    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Key on the rendered prompt bytes. `prompt_fingerprint` is a host hash of
      // the opaque PromptAst — the same byte-stability requirement that prefix
      // caching has, surfaced here as the cache key.
      let key: string = baml.llm.prompt_fingerprint(prompt);

      let hit: string? = self.store.get(key)
        catch (e) { _ => { null } };          // a broken cache must never fail the call
      match (hit) {
        let raw: string => { return baml.sap.parse<T>(raw); },
        _ => {},                              // miss: fall through to the model
      }

      // Miss: route to the wrapped member's call. It IS an HttpProvider (the only
      // thing a response cache can sensibly front), so match for it.
      let value: T = match (self.inner) {
        let h: HttpProvider => h.call<T>(prompt),
        _ => throw baml.errors.Unsupported { message: "cache inner is not request/response" },
      };
      // re-serialize for storage; `to_string<T>` is the typed JSON encoder
      // (inverse of the SAP parse on the read side).
      self.store.put(key, baml.json.to_string<T>(value), self.ttl_secs)
        catch (e) { _ => { null } };          // best-effort write
      value
    }

    // call_with FORWARDS the projection to the wrapped member so the usage/cost
    // sidecar (cache_read_input_tokens, cached_tokens, etc.) is NOT dropped on a
    // cache MISS. On a cache HIT there is no live ResponseMeta — the model was
    // skipped — so the projection sees a synthetic "served-from-cache" meta.
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let key: string = baml.llm.prompt_fingerprint(prompt);

      let hit: string? = self.store.get(key)
        catch (e) { _ => { null } };
      match (hit) {
        let raw: string => {
          // Cache hit: no wire response. Project a synthetic meta that reports the
          // call was served from cache (zero live usage) so the sidecar stays honest.
          return (baml.sap.parse<T>(raw), project(CacheHitMeta { }));
        },
        _ => {},                              // miss: fall through to the member
      }

      let (value, side): (T, U) = match (self.inner) {
        let h: HttpProvider => h.call_with<T, U>(prompt, project),
        _ => throw baml.errors.Unsupported { message: "cache inner is not request/response" },
      };
      self.store.put(key, baml.json.to_string<T>(value), self.ttl_secs)
        catch (e) { _ => { null } };
      (value, side)
    }
  }
}

// A synthetic ResponseMeta for a cache HIT: the model was skipped, so there is no
// live usage. Reports zero usage and a "cached" finish reason; everything else is
// Unsupported (the wire response that would carry it never happened).
class CacheHitMeta {
  implements ResponseMeta {
    function usage(self)         -> Usage                 { Usage { input: 0, output: 0 } }
    function finish_reason(self) -> string                { "cached" }
    function logprobs(self)      -> Supported<Logprob[]>  { Unsupported { reason: "served from cache" } }
    function citations(self)     -> Supported<Citation[]> { Unsupported { reason: "served from cache" } }
    function reasoning(self)     -> Supported<string>     { Unsupported { reason: "served from cache" } }
  }
}

// Note: Cache does NOT forward Streaming/Realtime/Tools. A response cache for a
// duplex session or a tool loop is meaningless, so we deliberately implement
// only HttpProvider here. `.stream` on a cached client therefore degrades to the
// Streaming companion's `_` arm (buffer-from-call) — which is correct: a cached
// answer is a single buffered value, not a live stream.

// -----------------------------------------------------------------------------
// Typed response envelopes — the shapes `parse` reads off the wire.
// -----------------------------------------------------------------------------
// `baml.http.Response` has NO `.json_path`; the typed read is
// `baml.json.from_json<RespClass>(baml.json.parse(resp.text()))` into one of
// these classes, then field access. Each class names only the fields we touch.

class AnthropicBlock     { text: string }
class AnthropicEnvelope  { content: AnthropicBlock[] }

class OpenAiContent      { text: string }
class OpenAiOutputItem   { content: OpenAiContent[] }
class OpenAiEnvelope     { output: OpenAiOutputItem[] }

class GeminiPart         { text: string }
class GeminiContent      { parts: GeminiPart[] }
class GeminiCandidate    { content: GeminiContent }
class GeminiEnvelope     { candidates: GeminiCandidate[] }
class GeminiCachedContents { name: string }   // POST /cachedContents response

// Pull `candidates[0].content.parts[0].text` out of a generateContent body by
// decoding into the typed envelope and walking the (optional) firsts. Used by
// both the plain `.call` parse and `call_with_cache`.
function gemini_first_text(raw: string) -> string
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  let env: GeminiEnvelope = baml.json.from_json<GeminiEnvelope>(baml.json.parse(raw));
  match (env.candidates.at(0)) {
    let cand: GeminiCandidate => match (cand.content.parts.at(0)) {
      let part: GeminiPart => part.text,
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>("empty parts", "gemini parse failed"),
    },
    _ => throw baml.UnknownError.with_message<baml.errors.CallError>("empty candidates", "gemini parse failed"),
  }
}

// -----------------------------------------------------------------------------
// (B) Anthropic inline annotation — `cache_control`, expressed in build_request.
// -----------------------------------------------------------------------------
// No new interface. `cache_control` is not orchestration and not a combinator —
// it is part of how Anthropic encodes its body. So it is just a provider field
// that build_request reads. The breakpoint placement (front-of-prompt, on the
// big stable block) is the provider's responsibility.

class Anthropic {
  model: string
  api_key: string?
  max_tokens: int

  // Which prompt segments to mark as cache breakpoints, by the segment role the
  // renderer tags. Empty => no caching (free, no premium). Anthropic caches
  // everything up to and INCLUDING each marked segment.
  cache_breakpoints: string[]   // e.g. ["system", "tools"]
  cache_ttl: string             // "5m" (ephemeral) | "1h" (extended)

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // PromptAst -> Anthropic's segmented message structure. The host renderer
      // gives us the segments tagged by role; we walk them and stamp
      // cache_control onto the ones the app asked to break on.
      let segments = baml.llm.anthropic.segments(prompt, self.model);

      let marked = for (let seg in segments) {
        // Stamp cache_control onto a segment if its role is a chosen breakpoint.
        if (self.cache_breakpoints.includes(seg.role)) {
          baml.llm.anthropic.with_cache_control(seg, self.cache_ttl)
        } else {
          seg
        }
      };

      let body: string = baml.llm.anthropic.encode_body(marked, self.model, self.max_tokens);
      baml.http.Request {
        method: "POST",
        url: "https://api.anthropic.com/v1/messages",
        headers: {
          "x-api-key": self.api_key ?? env.ANTHROPIC_API_KEY,
          "anthropic-version": "2023-06-01",
          "content-type": "application/json",
        },
        body: body,
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic send failed");
    }

    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // cache_creation_input_tokens / cache_read_input_tokens ride in usage; we
      // surface them through the normal usage channel, not the typed result.
      // Decode the wire envelope into a typed class, then SAP-parse the text it
      // carries. (Response has no `.json_path` — `from_json<T>` is the typed read.)
      let env: AnthropicEnvelope = baml.json.from_json<AnthropicEnvelope>(baml.json.parse(from.text()));
      let block: AnthropicBlock? = env.content.at(0);
      match (block) {
        let b: AnthropicBlock => baml.sap.parse<T>(b.text),
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>("empty content", "anthropic parse failed"),
      }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic parse failed");
    }
  }
}

// -----------------------------------------------------------------------------
// (C) OpenAI automatic prefix cache + routing hint — `prompt_cache_key`.
// -----------------------------------------------------------------------------
// Nothing to create, nothing to annotate. The provider caches stable prefixes
// automatically; the ONLY developer lever is an optional routing string. So it
// is a single optional provider field, emitted into the body. No interface, no
// combinator, no lifecycle. The byte-stable-prefix burden is on the renderer,
// not on us.

class OpenAI {
  model: string
  api_key: string?
  prompt_cache_key: string?     // optional routing hint; null => pure automatic

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // base body, then optionally splice in the routing hint. The hint changes
      // ONLY routing, never the model input — so it is appended after the prompt
      // is rendered and cannot bust the prefix.
      let base: string = baml.llm.openai.encode_responses_body(prompt, self.model);
      let body: string = match (self.prompt_cache_key) {
        let k: string => baml.llm.openai.with_field(base, "prompt_cache_key", k),
        _ => base,
      };
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/responses",
        headers: {
          "authorization": "Bearer " + (self.api_key ?? env.OPENAI_API_KEY),
          "content-type": "application/json",
        },
        body: body,
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai send failed");
    }

    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // usage.input_tokens_details.cached_tokens > 0 means a hit; observed via usage.
      // Typed read of the Responses envelope, then SAP-parse the inner text.
      let env: OpenAiEnvelope = baml.json.from_json<OpenAiEnvelope>(baml.json.parse(from.text()));
      let text: string? = match (env.output.at(0)) {
        let item: OpenAiOutputItem => match (item.content.at(0)) {
          let c: OpenAiContent => c.text,
          _ => null,
        },
        _ => null,
      };
      match (text) {
        let t: string => baml.sap.parse<T>(t),
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>("empty output", "openai parse failed"),
      }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai parse failed");
    }
  }
}

// -----------------------------------------------------------------------------
// (D) Gemini explicit resource — the hard case. NET-NEW surface.
// -----------------------------------------------------------------------------
// A `cachedContents/abc123` handle is server-authoritative state with a storage
// bill. It cannot be a `build_request` detail because it must EXIST before the
// request is built and must OUTLIVE a single call. The proposal's model has no
// place for "a provider that holds a long-lived server resource" — every
// capability so far is per-call. So we add one.
//
//   net-new interface: ManagedCache requires Provider
//
// It is a capability: a provider that supports explicit server-side caches.
// `CacheHandle` is an associated type (provider-owned + opaque, exactly like
// Tools.Transcript) because only Gemini knows what a handle is.

class CacheHandle { name: string, _data: $rust_type }   // opaque to callers

// NET-NEW capability-error interface. Managing a server-authoritative cache
// resource is NOT call/stream/realtime/tools, so it gets its own channel. A
// dangling/expired handle is the cache-specific failure callers want to triage
// (so they can recreate); the shared transport classifiers carry over.
interface baml.errors.CacheError {
  function is_network_error(self) -> bool
  function is_rate_limit(self) -> bool
  function is_expired_handle(self) -> bool
}

interface ManagedCache requires Provider {
  type Handle

  // Create the resource (a separate API call). Returns the addressable handle.
  function create_cache(self, prefix: baml.llm.PromptAst, ttl_secs: int) -> Handle
      throws baml.ExtendUnknownError<baml.errors.CacheError>

  // Extend TTL on an existing handle.
  function refresh_cache(self, handle: Handle, ttl_secs: int) -> Handle
      throws baml.ExtendUnknownError<baml.errors.CacheError>

  // Delete the resource — stops the storage bill. Caller MUST eventually call
  // this or leak money. The proposal has no destructor/RAII, so this is manual.
  function delete_cache(self, handle: Handle) -> null
      throws baml.ExtendUnknownError<baml.errors.CacheError>

  // Make a call that REFERENCES an existing handle as its cached prefix.
  function call_with_cache<T>(self, handle: Handle, prompt: baml.llm.PromptAst) -> T
      throws baml.ExtendUnknownError<baml.errors.CacheError>
}

// A dangling/expired `cachedContents` handle: Gemini returns an error referencing
// a cache that no longer exists. A distinct typed failure on the CacheError
// channel so the caller can recreate the resource rather than treating it as a
// generic transport blip.
class GeminiExpiredCacheError {
  handle_name: string
  implements baml.errors.CacheError {
    function is_network_error(self)   -> bool { return false; }
    function is_rate_limit(self)      -> bool { return false; }
    function is_expired_handle(self)  -> bool { return true; }
  }
}

class Gemini {
  model: string
  api_key: string?

  implements ChatModelOptions { model as model, api_key as api_key }

  // The ordinary, no-cache path. Gemini also has implicit caching on newer
  // models (zero-config, observed in usage), so plain `.call` may still get a
  // discount with no annotation — same as OpenAI's automatic mode.
  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.Request {
        method: "POST",
        url: "https://generativelanguage.googleapis.com/v1beta/models/" + self.model + ":generateContent",
        headers: { "x-goog-api-key": self.api_key ?? env.GEMINI_API_KEY, "content-type": "application/json" },
        body: baml.llm.gemini.encode_body(prompt, self.model),
      }
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "gemini send failed");
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(gemini_first_text(from.text()))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "gemini parse failed");
    }
  }

  // The explicit-resource path.
  implements ManagedCache {
    type Handle = CacheHandle

    function create_cache(self, prefix: baml.llm.PromptAst, ttl_secs: int) -> CacheHandle
        throws baml.ExtendUnknownError<baml.errors.CacheError> {
      // POST /cachedContents — a SEPARATE round trip from generateContent.
      let req = baml.http.Request {
        method: "POST",
        url: "https://generativelanguage.googleapis.com/v1beta/cachedContents",
        headers: { "x-goog-api-key": self.api_key ?? env.GEMINI_API_KEY, "content-type": "application/json" },
        body: baml.llm.gemini.encode_cache_create(prefix, self.model, ttl_secs),
      };
      let resp = baml.http.send(req);
      let created: GeminiCachedContents =
        baml.json.from_json<GeminiCachedContents>(baml.json.parse(resp.text()));
      CacheHandle {
        name: created.name,                   // "cachedContents/abc123"
        _data: baml.llm.gemini.cache_state(resp),
      }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CacheError>(e, "gemini cache create failed");
    }

    function refresh_cache(self, handle: CacheHandle, ttl_secs: int) -> CacheHandle
        throws baml.ExtendUnknownError<baml.errors.CacheError> {
      let req = baml.http.Request {
        method: "PATCH",
        url: "https://generativelanguage.googleapis.com/v1beta/" + handle.name + "?updateMask=ttl",
        headers: { "x-goog-api-key": self.api_key ?? env.GEMINI_API_KEY, "content-type": "application/json" },
        body: baml.llm.gemini.encode_ttl(ttl_secs),
      };
      baml.http.send(req);
      handle
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CacheError>(e, "gemini cache refresh failed");
    }

    function delete_cache(self, handle: CacheHandle) -> null
        throws baml.ExtendUnknownError<baml.errors.CacheError> {
      let req = baml.http.Request {
        method: "DELETE",
        url: "https://generativelanguage.googleapis.com/v1beta/" + handle.name,
        headers: { "x-goog-api-key": self.api_key ?? env.GEMINI_API_KEY },
        body: "",
      };
      baml.http.send(req);
      null
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CacheError>(e, "gemini cache delete failed");
    }

    function call_with_cache<T>(self, handle: CacheHandle, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CacheError> {
      // generateContent with `cached_content: <handle.name>`. The handle supplies
      // the prefix; `prompt` is just the new turn.
      let body: string = baml.llm.gemini.encode_body_with_cache(prompt, self.model, handle.name);
      let req = baml.http.Request {
        method: "POST",
        url: "https://generativelanguage.googleapis.com/v1beta/models/" + self.model + ":generateContent",
        headers: { "x-goog-api-key": self.api_key ?? env.GEMINI_API_KEY, "content-type": "application/json" },
        body: body,
      };
      let resp = baml.http.send(req);
      // A dangling/expired handle is a distinct, recoverable failure: surface it as
      // the concrete GeminiExpiredCacheError on the CacheError channel so the caller
      // can recreate, rather than letting it look like a generic transport error.
      if (baml.llm.gemini.is_expired_cache(resp)) {
        throw GeminiExpiredCacheError { handle_name: handle.name };
      }
      baml.sap.parse<T>(gemini_first_text(resp.text()))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CacheError>(e, "gemini cached generate failed");
    }
  }
}

// -----------------------------------------------------------------------------
// A lifecycle helper so apps don't hand-roll create/delete. Still NOT a
// destructor — the app must call this scope explicitly. This is the closest the
// proposal gets to owning the Gemini resource lifecycle.
// -----------------------------------------------------------------------------
class CacheScope {
  // Run `body` with a freshly-created cache handle, then guarantee delete.
  // `body` receives the provider and the handle; it makes call_with_cache calls.
  // This is the GC the background warns about — done manually, in BAML, because
  // there is no `defer`/RAII in the model.
  function with_cache<T, R>(
    provider: ManagedCache,
    prefix: baml.llm.PromptAst,
    ttl_secs: int,
    body: (ManagedCache, ManagedCache.Handle) -> R
  ) -> R throws baml.ExtendUnknownError<baml.errors.CacheError> {
    let handle = provider.create_cache(prefix, ttl_secs);
    let result: R = body(provider, handle)
      catch (e) {
        // clean up even on failure, then re-raise the ORIGINAL error unchanged
        // (a known CacheError keeps its identity; anything else stays wrapped).
        _ => {
          provider.delete_cache(handle) catch (e2) { _ => { null } };
          throw baml.UnknownError.with_message<baml.errors.CacheError>(e, "cached body failed; handle deleted");
        }
      };
    provider.delete_cache(handle) catch (e) { _ => { null } };          // stop the storage bill
    result
  }
}

// -----------------------------------------------------------------------------
// Host boundaries used above (all genuine $rust_io / codec seams):
//   baml.llm.prompt_fingerprint(prompt) -> string          // stable hash of PromptAst
//   baml.llm.anthropic.segments(prompt, model) -> Segment[]
//   baml.llm.anthropic.with_cache_control(seg, ttl) -> Segment
//   baml.llm.anthropic.encode_body(segs, model, max_tokens) -> string
//   baml.llm.openai.encode_responses_body(prompt, model) -> string
//   baml.llm.openai.with_field(body, key, value) -> string
//   baml.llm.gemini.encode_body(prompt, model) -> string
//   baml.llm.gemini.encode_cache_create(prefix, model, ttl) -> string
//   baml.llm.gemini.encode_ttl(ttl) -> string
//   baml.llm.gemini.encode_body_with_cache(prompt, model, name) -> string
//   baml.llm.gemini.cache_state(resp) -> $rust_type
//   baml.llm.gemini.is_expired_cache(resp) -> bool           // dangling cachedContents handle
// `Segment` is the host renderer's tagged content-block type: { role: string, ... }.
