// scenario 31 — caching: ../../../../../llm-provider/ideas/scenarios/31-caching/implement.baml
//   implementation.baml — the STD-LIB / library-author half.
//
// The design's implement.baml split "caching" into FOUR mechanisms wearing one
// word and showed the proposal absorbing each:
//   (A) a vendor-neutral client-side response cache — the spine's `Cache`
//       combinator + a `.cached(store)` factory on the Provider marker;
//   (B) Anthropic inline `cache_control` — a per-content-block annotation inside
//       `build_request`;
//   (C) OpenAI automatic prefix cache + `prompt_cache_key` — one optional field
//       spliced into the body inside `build_request`;
//   (D) Gemini explicit `cachedContents` — a SERVER-AUTHORITATIVE, billed-while-idle
//       resource with a create / use / delete lifecycle. THIS is the hard case,
//       and the one the design says forces net-new surface.
//
// On the REAL surface the scenario aligns to the one primitive that actually
// SHIPPED for it: `baml.ai.ManagedCache` (ns_ai/capabilities/cache.baml) — the
// Family-A1 capability. It is deliberately smaller than the design's version:
//   REAL   `interface ManagedCache requires Provider {`
//          `  create_cache(self, prefix: string, ttl_secs: int) -> CacheHandle`
//          `  delete_cache(self, handle: CacheHandle) -> null }`
//   DESIGN also had `refresh_cache`, a generic `call_with_cache<T>(handle, prompt)`,
//          an associated `type Handle`, a bespoke `baml.errors.CacheError` channel,
//          and a `prefix: PromptAst` (not `string`).
// So the alignment models the design's create/use/delete lifecycle, handle-as-data,
// the billed-while-idle hazard, and capability negotiation on the REAL two-method
// interface — spelling the missing "use a handle" step as a scenario-local method
// (the divergence is called out in the alignment report and in `usage.baml`).
//
// (A)/(B)/(C) are NOT reachable on the real surface as *caching*: there is no
// `Cache` combinator / `.cached()` factory (design (A) is net-new and absent),
// and Anthropic `cache_control` is a per-content-block wire annotation with no
// seam on the shipped provider (`extra_body` merges at the TOP level only, so it
// cannot reach a message content block). Those are documented as BLOCKED live
// paths in `usage.baml`. This file builds the offline ManagedCache surface.


// ---------------------------------------------------------------------------
// Usage sidecar — the observable that makes "cost drops on a cache hit" real.
// A cold call reports the full prefix token cost; a cache-hit call reports only
// the NEW-turn tokens (the prefix was billed once, at create). Reading
// `usage().input_tokens` off the two is how a test proves the discount.
// ---------------------------------------------------------------------------
class s31_UsageMeta {
    inp: int,
    out: int,

    implements baml.ai.ResponseMeta {
        function finish_reason(self) -> string throws never { "stop" }
        function usage(self) -> baml.ai.Usage throws never {
            baml.ai.Usage { input_tokens: self.inp, output_tokens: self.out }
        }
        function reasoning(self) -> string | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "n/a" }
        }
        function logprobs(self) -> baml.ai.Logprob[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "n/a" }
        }
        function citations(self) -> baml.ai.Citation[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "n/a" }
        }
    }
}


// ---------------------------------------------------------------------------
// The dangling/expired-handle hazard, as a TYPED shape (design §D:
// `GeminiExpiredCacheError`). A handle whose server-side resource was deleted
// (or expired past its TTL) is a distinct, RECOVERABLE failure the caller
// branches on — recreate the cache rather than treat it as a generic blip.
// The design put this on a net-new `baml.errors.CacheError` channel; that
// channel is NOT on the real surface, so it is a plain concrete error carrying
// its own `is_expired_handle()` triage classifier.
// ---------------------------------------------------------------------------
class s31_ExpiredCacheError {
    handle_name: string,

    function is_expired_handle(self) -> bool throws never { true }
}


// ---------------------------------------------------------------------------
// One record in the fake's server-side cache store. `alive` is the billed-idle
// switch: while true the resource costs `prefix_tokens` of STORAGE every moment
// it exists — even when no call is made — which is exactly the hazard the design
// warns about ("costs money while idle"). `delete_cache` flips it false; that is
// the only thing that stops the bill.
// ---------------------------------------------------------------------------
class s31_CacheRecord {
    name: string,
    prefix_tokens: int,   // storage cost while alive (the billed-idle unit)
    alive: bool,          // false once deleted — bill stops, handle goes dangling
}


// ---------------------------------------------------------------------------
// Offline fake — a Gemini-like provider that holds SERVER-AUTHORITATIVE cache
// resources. It implements THREE stdlib interfaces:
//   - `Provider`      (the marker; `is_effectful` = true because a cache resource
//                      is external, billed state a blind re-drive would duplicate),
//   - `HttpProvider`  (the ordinary no-cache call path — the COLD baseline),
//   - `ManagedCache`  (create/delete of the server-side resource).
// plus a scenario-local `use_cache<T>` (the design's `call_with_cache`, which the
// real two-method `ManagedCache` interface omits — DIVERGENCE, see report).
//
// `store` is the scripted "server": create_cache mints a record, delete_cache
// kills it, use_cache serves cheap iff the record is alive. The counters
// (`create_calls`/`delete_calls`/`hit_calls`) are the observability hooks a test
// reads to prove the lifecycle ran exactly as claimed.
// ---------------------------------------------------------------------------
class s31_ManagedFake {
    reply: string,
    cold_input_tokens: int,    // a full call pays the whole prefix
    cached_input_tokens: int,  // a cache-hit call pays only the new turn
    output_tokens: int,
    store: map<string, s31_CacheRecord>,
    mint_counter: int,
    create_calls: int,
    delete_calls: int,
    hit_calls: int,

    implements baml.ai.Provider {
        // A server-side cache is external, billed state: re-driving a create would
        // double-provision (and double-bill). Mark it effectful so combinators
        // refuse-then-error (CannotRetry) instead of silently re-driving. (design D2)
        function is_effectful(self) -> bool throws never { true }
    }

    // ── The COLD call path (no managed cache) ──────────────────────────────
    // This is what a normal `.call` costs: the full prefix every time. The
    // managed-cache path (use_cache) is the discount against THIS baseline.
    implements baml.ai.HttpProvider {
        type Body = string

        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request
            throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["managed-fake: no request"] }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["managed-fake: no send"] }
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["managed-fake parse failed"] },
            }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
            s31_UsageMeta { inp: self.cold_input_tokens, out: self.output_tokens }
        }

        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V>
            throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            let value: T = baml.sap.parse<T>(self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["managed-fake parse failed"] },
            };
            baml.ai.CallResult<T, V> {
                value: value,
                meta: project(s31_UsageMeta { inp: self.cold_input_tokens, out: self.output_tokens }),
            }
        }
    }

    // ── The managed-cache lifecycle (the real ManagedCache capability) ─────
    implements baml.ai.ManagedCache {
        // Create the server-side resource — a SEPARATE round trip that provisions
        // billed storage. Returns the addressable handle (handle-as-data: a plain
        // `CacheHandle { name }` the caller passes around). log at the create point.
        function create_cache(self, prefix: string, ttl_secs: int) -> baml.ai.CacheHandle throws never {
            let name: string = "cachedContents/s31-" + self.mint_counter.to_string();
            self.mint_counter = self.mint_counter + 1;
            self.create_calls = self.create_calls + 1;
            // prefix length stands in for the prefix's token cost (billed while idle).
            self.store.set(name, s31_CacheRecord { name: name, prefix_tokens: prefix.length(), alive: true });
            log.info("31 cache create: name=" + name + " ttl_secs=" + ttl_secs.to_string() + " prefix_tokens=" + prefix.length().to_string());
            baml.ai.CacheHandle { name: name }
        }

        // Delete the resource — the ONLY thing that stops the storage bill. The
        // caller MUST call this (or leak money); the model has no destructor/RAII.
        function delete_cache(self, handle: baml.ai.CacheHandle) -> null throws never {
            self.delete_calls = self.delete_calls + 1;
            match (self.store.get(handle.name)) {
                null => { null }
                let rec: s31_CacheRecord => {
                    rec.alive = false;                 // bill stops; handle now dangling
                    self.store.set(handle.name, rec);
                    log.info("31 cache delete: name=" + handle.name + " (storage bill stopped)");
                    null
                }
            }
        }
    }

    // ── Scenario-local: USE a handle (the design's `call_with_cache<T>`). ───
    // The real `ManagedCache` interface has no "use" method, so a call that
    // references a handle is spelled here as a fake-local method. A cache HIT
    // pays only `cached_input_tokens` (the new turn) — the prefix was billed at
    // create — which is how a test observes the cost drop. A dead/unknown handle
    // is the recoverable expired-handle failure. (DIVERGENCE: real interface omits this.)
    function use_cache<T>(self, handle: baml.ai.CacheHandle, prompt: string)
        -> baml.ai.CallResult<T, baml.ai.Usage>
        throws s31_ExpiredCacheError | baml.errors.UnknownError {
        match (self.store.get(handle.name)) {
            null => { throw s31_ExpiredCacheError { handle_name: handle.name }; }
            let rec: s31_CacheRecord => {
                if (!rec.alive) {
                    throw s31_ExpiredCacheError { handle_name: handle.name };
                }
                self.hit_calls = self.hit_calls + 1;
                let value: T = baml.sap.parse<T>(self.reply) catch (e) {
                    _ => throw baml.errors.UnknownError { data: e, message: ["managed-fake cached parse failed"] },
                };
                log.info("31 cache hit: name=" + handle.name + " cached_input_tokens=" + self.cached_input_tokens.to_string());
                baml.ai.CallResult<T, baml.ai.Usage> {
                    value: value,
                    meta: baml.ai.Usage { input_tokens: self.cached_input_tokens, output_tokens: self.output_tokens },
                }
            }
        }
    }

    // The billed-while-idle observable: total storage cost of every LIVE record,
    // summed regardless of whether any call was made. Non-zero after create even
    // with zero calls (the hazard); zero once every handle is deleted.
    function storage_bill_units(self) -> int throws never {
        let total: int = 0;
        for (let rec in self.store.values()) {
            if (rec.alive) { total = total + rec.prefix_tokens; }
        }
        total
    }
}


// ---------------------------------------------------------------------------
// A plain provider that does NOT support managed caching — implements only the
// marker + HttpProvider. It stands in for both "a provider with only implicit
// wire-level caching (invisible to the app)" AND the negotiation-negative: an
// app that `match`es for `ManagedCache` degrades here. `.call` still works.
// ---------------------------------------------------------------------------
class s31_PlainEcho {
    reply: string,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request
            throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["plain: no request"] }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["plain: no send"] }
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["plain parse failed"] },
            }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
            baml.ai.LegacyMeta {}
        }
        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V>
            throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            let value: T = baml.sap.parse<T>(self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["plain parse failed"] },
            };
            baml.ai.CallResult<T, V> { value: value, meta: project(baml.ai.LegacyMeta {}) }
        }
    }
}


// ---------------------------------------------------------------------------
// Runtime capability negotiation — "does this provider support managed caching?"
// A pure `match` on the existential marker (design: capability is a RUNTIME
// promise). Used both to gate the lifecycle and to degrade when it is absent.
// ---------------------------------------------------------------------------
function s31_supports_managed_cache(p: baml.ai.Provider) -> bool throws never {
    match (p) {
        let mc: baml.ai.ManagedCache => true,
        _ => false,
    }
}


// ---------------------------------------------------------------------------
// The manual lifecycle scope — the design's `CacheScope.with_cache`. It
// guarantees `delete_cache` runs (stopping the storage bill) EVEN IF the body
// throws. This is hand-rolled garbage collection: BEP-042's
//   `defer { provider.delete_cache(handle) }`
// would express it as one line, but `defer` is a RESERVED keyword (BEP-042, not
// yet usable), so we spell the create → body → cleanup-on-both-paths by hand.
// Typed on the concrete fake (not the `ManagedCache` interface) because `use_cache`
// — which the body needs — is a fake-local method, not on the real interface.
// ---------------------------------------------------------------------------
function s31_with_cache<R>(
    provider: s31_ManagedFake,
    prefix: string,
    ttl_secs: int,
    body: (s31_ManagedFake, baml.ai.CacheHandle) -> R throws s31_ExpiredCacheError | baml.errors.UnknownError,
) -> R throws s31_ExpiredCacheError | baml.errors.UnknownError {
    let handle: baml.ai.CacheHandle = provider.create_cache(prefix, ttl_secs);
    // `defer { provider.delete_cache(handle) }` — hand-rolled because `defer` is reserved:
    let result: R = body(provider, handle) catch (e) {
        let ex: s31_ExpiredCacheError => { provider.delete_cache(handle); throw ex; }
        let u: baml.errors.UnknownError => { provider.delete_cache(handle); throw u; }
    };
    provider.delete_cache(handle);   // happy-path cleanup: stop the storage bill
    result
}
