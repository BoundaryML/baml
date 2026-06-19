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

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
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

      let value: T = self.inner.call<T>(prompt);
      // re-serialize for storage; SAP round-trips the typed value back to text.
      self.store.put(key, baml.sap.to_json<T>(value), self.ttl_secs)
        catch (e) { _ => { null } };          // best-effort write
      value
    }
  }
}

// Note: Cache does NOT forward Streaming/Realtime/Tools. A response cache for a
// duplex session or a tool loop is meaningless, so we deliberately implement
// only Provider here. `.stream` on a cached client therefore degrades to the
// Streaming companion's `_` arm (buffer-from-call) — which is correct: a cached
// answer is a single buffered value, not a live stream.

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

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // PromptAst -> Anthropic's segmented message structure. The host renderer
      // gives us the segments tagged by role; we walk them and stamp
      // cache_control onto the ones the app asked to break on.
      let segments = baml.llm.anthropic.segments(prompt, self.model);

      let marked = for (let seg in segments) {
        // Stamp cache_control onto a segment if its role is a chosen breakpoint.
        if (self.cache_breakpoints.contains(seg.role)) {
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

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }

    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      // cache_creation_input_tokens / cache_read_input_tokens ride in usage; we
      // surface them through the normal usage channel, not the typed result.
      baml.sap.parse<T>(from.json_path("content.0.text"))
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

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
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

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }

    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      // usage.input_tokens_details.cached_tokens > 0 means a hit; observed via usage.
      baml.sap.parse<T>(from.json_path("output.0.content.0.text"))
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

interface ManagedCache requires Provider {
  type Handle

  // Create the resource (a separate API call). Returns the addressable handle.
  function create_cache(self, prefix: baml.llm.PromptAst, ttl_secs: int) -> Handle throws Error

  // Extend TTL on an existing handle.
  function refresh_cache(self, handle: Handle, ttl_secs: int) -> Handle throws Error

  // Delete the resource — stops the storage bill. Caller MUST eventually call
  // this or leak money. The proposal has no destructor/RAII, so this is manual.
  function delete_cache(self, handle: Handle) -> null throws Error

  // Make a call that REFERENCES an existing handle as its cached prefix.
  function call_with_cache<T>(self, handle: Handle, prompt: baml.llm.PromptAst) -> T throws Error
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

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      baml.http.Request {
        method: "POST",
        url: "https://generativelanguage.googleapis.com/v1beta/models/" + self.model + ":generateContent",
        headers: { "x-goog-api-key": self.api_key ?? env.GEMINI_API_KEY, "content-type": "application/json" },
        body: baml.llm.gemini.encode_body(prompt, self.model),
      }
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.json_path("candidates.0.content.parts.0.text"))
    }
  }

  // The explicit-resource path.
  implements ManagedCache {
    type Handle = CacheHandle

    function create_cache(self, prefix: baml.llm.PromptAst, ttl_secs: int) -> CacheHandle throws baml.errors.LlmClient {
      // POST /cachedContents — a SEPARATE round trip from generateContent.
      let req = baml.http.Request {
        method: "POST",
        url: "https://generativelanguage.googleapis.com/v1beta/cachedContents",
        headers: { "x-goog-api-key": self.api_key ?? env.GEMINI_API_KEY, "content-type": "application/json" },
        body: baml.llm.gemini.encode_cache_create(prefix, self.model, ttl_secs),
      };
      let resp = baml.http.send(req)
        catch (e) { _ => { throw baml.errors.LlmClient { message: "cache create failed" }; } };
      CacheHandle {
        name: resp.json_path("name"),         // "cachedContents/abc123"
        _data: baml.llm.gemini.cache_state(resp),
      }
    }

    function refresh_cache(self, handle: CacheHandle, ttl_secs: int) -> CacheHandle throws baml.errors.LlmClient {
      let req = baml.http.Request {
        method: "PATCH",
        url: "https://generativelanguage.googleapis.com/v1beta/" + handle.name + "?updateMask=ttl",
        headers: { "x-goog-api-key": self.api_key ?? env.GEMINI_API_KEY, "content-type": "application/json" },
        body: baml.llm.gemini.encode_ttl(ttl_secs),
      };
      baml.http.send(req) catch (e) { _ => { throw baml.errors.LlmClient { message: "cache refresh failed" }; } };
      handle
    }

    function delete_cache(self, handle: CacheHandle) -> null throws baml.errors.LlmClient {
      let req = baml.http.Request {
        method: "DELETE",
        url: "https://generativelanguage.googleapis.com/v1beta/" + handle.name,
        headers: { "x-goog-api-key": self.api_key ?? env.GEMINI_API_KEY },
        body: "",
      };
      baml.http.send(req) catch (e) { _ => { throw baml.errors.LlmClient { message: "cache delete failed" }; } };
      null
    }

    function call_with_cache<T>(self, handle: CacheHandle, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      // generateContent with `cached_content: <handle.name>`. The handle supplies
      // the prefix; `prompt` is just the new turn.
      let body: string = baml.llm.gemini.encode_body_with_cache(prompt, self.model, handle.name);
      let req = baml.http.Request {
        method: "POST",
        url: "https://generativelanguage.googleapis.com/v1beta/models/" + self.model + ":generateContent",
        headers: { "x-goog-api-key": self.api_key ?? env.GEMINI_API_KEY, "content-type": "application/json" },
        body: body,
      };
      let resp = baml.http.send(req)
        catch (e) { _ => { throw baml.errors.LlmClient { message: "cached generate failed" }; } };
      // A dangling/expired handle returns an error from Gemini — surfaced here as
      // the typed LlmClient throw, so the caller can recreate.
      baml.sap.parse<T>(resp.json_path("candidates.0.content.parts.0.text"))
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
  ) -> R throws baml.errors.LlmClient {
    let handle = provider.create_cache(prefix, ttl_secs);
    let result: R = body(provider, handle)
      catch (e) {
        _ => {
          provider.delete_cache(handle) catch (e2) { _ => { null } };   // clean up even on failure
          throw baml.errors.LlmClient { message: "cached body failed; handle deleted" };
        }
      };
    provider.delete_cache(handle) catch (e) { _ => { null } };          // stop the storage bill
    result
  }
}

// -----------------------------------------------------------------------------
// Host boundaries used above (all genuine $rust_io / codec seams):
//   baml.llm.prompt_fingerprint(prompt) -> string          // stable hash of PromptAst
//   baml.sap.to_json<T>(value) -> string                   // inverse of sap.parse
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
// `Segment` is the host renderer's tagged content-block type: { role: string, ... }.
