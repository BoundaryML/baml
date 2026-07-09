// scenario 31 — caching: ../../../../../llm-provider/ideas/scenarios/31-caching/usage.baml
//   usage.baml — APPLICATION-author code. End-user surface.
//
// The design's app story split along the four caching shapes. Only ONE of them —
// (D) Gemini's explicit `cachedContents` — has a shipped home on the real surface:
// `baml.ai.ManagedCache` (the Family-A1 capability). The others are documented as
// unreachable-as-caching (see the BLOCKED live markers at the bottom):
//   (A) client-side response `Cache` combinator / `.cached()`  — net-new, ABSENT;
//   (B) Anthropic inline `cache_control`                        — no per-block seam;
//   (C) OpenAI `prompt_cache_key` / automatic prefix cache      — invisible to the app.
//
// So the app surface here is the design's HARD case, faithfully: the managed
// cache CANNOT ride the `client` sugar (it erases to the existential `Provider`,
// which drops the `ManagedCache` capability — the design's documented leak), so
// the app drops to longhand code that owns the create / use / delete lifecycle
// explicitly. Every test carries a `log.info` at a create / hit / delete point.


// The dead default client (ns_ai_scenarios house rule): offline tests must pass
// an explicit deterministic provider via `client =`.
client<llm> DeadS31 {
  provider openai
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

// The plain single-turn function — the COLD baseline and the negotiation entry
// point. Every caching policy is just a different Provider handed in via `client =`.
function s31_Ask(question: string) -> string {
  client DeadS31
  prompt #"
    {{ _.role("user") }}
    {{ question }}
  "#
}

// A fresh managed-cache fake. cold=1000 (full prefix) vs cached=50 (new turn only)
// is the observable discount; output=20. `store` starts empty.
function s31_fresh_fake() -> s31_ManagedFake throws never {
  let f: s31_ManagedFake = s31_ManagedFake {
    reply: "answer",
    cold_input_tokens: 1000,
    cached_input_tokens: 50,
    output_tokens: 20,
    store: {},
    mint_counter: 0,
    create_calls: 0,
    delete_calls: 0,
    hit_calls: 0,
  };
  f
}


// ============================================================================
// 1. LIFECYCLE — create -> use -> delete, with the handle threaded as DATA.
//    One `create_cache` provisions the server resource and returns an opaque,
//    addressable `CacheHandle`; the app passes it (as a plain value) into
//    `use_cache`; one `delete_cache` reclaims it. The counters prove each step
//    ran exactly once.
// ============================================================================

test "31: managed cache lifecycle — create returns a handle, use serves it, delete reclaims" {
    let fake = s31_fresh_fake();

    let handle: baml.ai.CacheHandle = fake.create_cache("You are a contract analyst. <big contract>", 3600);
    log.info("31 lifecycle: created handle name=" + handle.name);
    // handle-as-data: it is a plain value with an addressable name the app holds.
    assert.equal(handle.name, "cachedContents/s31-0")

    let r = fake.use_cache<string>(handle, "termination clause?") catch (e) { _ => baml.ai.CallResult<string, baml.ai.Usage> { value: "use failed", meta: baml.ai.Usage { input_tokens: -1, output_tokens: -1 } } };
    assert.equal(r.value, "answer")

    fake.delete_cache(handle) catch (e) { _ => null };

    assert.equal(fake.create_calls, 1)
    assert.equal(fake.hit_calls, 1)
    assert.equal(fake.delete_calls, 1)
}


// ============================================================================
// 2. COST DROPS ON A HIT — the whole point of a prompt cache. A COLD call
//    (the plain LLM function over the same fake) pays the full prefix; a
//    cache-HIT call over a live handle pays only the new turn. Reading
//    `usage().input_tokens` off each shows the discount.
// ============================================================================

test "31: a cache hit costs far less than a cold call (input tokens drop)" {
    let fake = s31_fresh_fake();

    // COLD: the ordinary call path, projecting usage via the $with companion.
    let cold = s31_Ask$with(
        "termination clause?",
        (m: baml.ai.ResponseMeta) -> baml.ai.Usage { m.usage() },
        client = fake,
    ) catch (e) { _ => baml.ai.CallResult<string, baml.ai.Usage> { value: "cold failed", meta: baml.ai.Usage { input_tokens: -1, output_tokens: -1 } } };

    // HIT: create the shared cache once, then reference it.
    let handle: baml.ai.CacheHandle = fake.create_cache("<the big shared contract prefix>", 3600);
    let hit = fake.use_cache<string>(handle, "termination clause?") catch (e) { _ => baml.ai.CallResult<string, baml.ai.Usage> { value: "hit failed", meta: baml.ai.Usage { input_tokens: -1, output_tokens: -1 } } };

    log.info("31 cost: cold_input=" + cold.meta.input_tokens.to_string() + " cached_input=" + hit.meta.input_tokens.to_string());
    assert.equal(cold.meta.input_tokens, 1000)
    assert.equal(hit.meta.input_tokens, 50)
    assert.is_true(hit.meta.input_tokens < cold.meta.input_tokens)

    fake.delete_cache(handle) catch (e) { _ => null };
}


// ============================================================================
// 3. BILLED-WHILE-IDLE — the hazard the design flags: a server-side cache costs
//    STORAGE the whole time it exists, even with ZERO calls. `storage_bill_units`
//    is non-zero the moment the handle is created (no call made), and only
//    `delete_cache` drives it back to zero. Forget to delete -> leak money.
// ============================================================================

test "31: a live handle is billed while idle — only delete stops the storage bill" {
    let fake = s31_fresh_fake();
    assert.equal(fake.storage_bill_units(), 0)     // nothing provisioned yet

    let handle: baml.ai.CacheHandle = fake.create_cache("<32KB of contract text>", 3600);
    // NO call made — yet the bill is already accruing (the whole hazard).
    let idle_bill: int = fake.storage_bill_units();
    log.info("31 billed-idle: bill_after_create_no_calls=" + idle_bill.to_string());
    assert.is_true(idle_bill > 0)

    fake.delete_cache(handle) catch (e) { _ => null };
    assert.equal(fake.storage_bill_units(), 0)     // bill stops ONLY on delete
}


// ============================================================================
// 4. EXPIRED / DANGLING HANDLE — a typed, recoverable failure. Using a handle
//    whose resource was deleted (or expired past TTL) throws the concrete
//    `s31_ExpiredCacheError`; the app branches on it (recreate) rather than
//    treating it as a generic transport blip. (design §D `GeminiExpiredCacheError`.)
// ============================================================================

test "31: using a deleted handle throws the typed expired-handle error" {
    let fake = s31_fresh_fake();
    let handle: baml.ai.CacheHandle = fake.create_cache("<prefix>", 3600);
    fake.delete_cache(handle) catch (e) { _ => null };   // resource gone; handle now dangling

    let verdict = fake.use_cache<string>(handle, "any question") catch (e) {
        let ex: s31_ExpiredCacheError => "expired/recoverable=" + ex.is_expired_handle().to_string() + " name=" + ex.handle_name,
        let u: baml.errors.UnknownError => "unknown",
    };
    log.info("31 expired: " + verdict);
    assert.equal(verdict, "expired/recoverable=true name=cachedContents/s31-0")
}


// ============================================================================
// 5. THE LIFECYCLE SCOPE (manual RAII) — `s31_with_cache` guarantees delete even
//    when the body throws. It is the design's `CacheScope.with_cache`: create a
//    shared handle, run N cheap cached calls, ALWAYS delete. BEP-042 would write
//    the cleanup as `defer { delete_cache(handle) }`; `defer` is reserved, so the
//    scope hand-rolls it.
// ============================================================================

test "31: with_cache runs a whole session over one shared handle, then always deletes" {
    let fake = s31_fresh_fake();
    let questions: string[] = ["termination clause?", "indemnity cap?", "governing law?"];

    let answers: string[] = s31_with_cache<string[]>(
        fake,
        "You are a contract analyst. Document: <big contract>",
        3600,
        (prov: s31_ManagedFake, handle: baml.ai.CacheHandle) -> string[] throws s31_ExpiredCacheError | baml.errors.UnknownError {
            let acc: string[] = [];
            for (let q in questions) {
                let r = prov.use_cache<string>(handle, q);
                acc.push(r.value);
            }
            acc
        },
    ) catch (e) { _ => [] };

    log.info("31 scope: answers=" + answers.length().to_string() + " creates=" + fake.create_calls.to_string() + " deletes=" + fake.delete_calls.to_string() + " hits=" + fake.hit_calls.to_string());
    assert.equal(answers.length(), 3)          // ONE create, N cheap hits, ONE delete
    assert.equal(fake.create_calls, 1)
    assert.equal(fake.hit_calls, 3)
    assert.equal(fake.delete_calls, 1)
    assert.equal(fake.storage_bill_units(), 0) // the scope guaranteed cleanup
}

test "31: with_cache still deletes the handle when the session body throws" {
    let fake = s31_fresh_fake();

    let outcome = s31_with_cache<string>(
        fake,
        "<shared prefix>",
        3600,
        (prov: s31_ManagedFake, handle: baml.ai.CacheHandle) -> string throws s31_ExpiredCacheError | baml.errors.UnknownError {
            // simulate a mid-session failure AFTER the cache was created.
            throw baml.errors.UnknownError { data: null, message: ["question 2 blew up"] };
        },
    ) catch (e) {
        let u: baml.errors.UnknownError => "surfaced-and-cleaned",
        let ex: s31_ExpiredCacheError => "expired",
    };

    log.info("31 scope-throw: outcome=" + outcome + " deletes=" + fake.delete_calls.to_string() + " bill=" + fake.storage_bill_units().to_string());
    assert.equal(outcome, "surfaced-and-cleaned")
    assert.equal(fake.delete_calls, 1)         // cleanup ran despite the throw
    assert.equal(fake.storage_bill_units(), 0) // no leaked storage bill
}


// ============================================================================
// 6. CAPABILITY NEGOTIATION — "does this provider support managed caching?" is a
//    RUNTIME `match` on the existential marker (design: capability is a runtime
//    promise). A managed fake answers yes; a plain provider answers no and the
//    app degrades to the ordinary (implicitly-cached-at-best) call path.
// ============================================================================

test "31: capability negotiation — managed cache is present on the fake, absent on a plain provider" {
    let managed: baml.ai.Provider = s31_fresh_fake();
    let plain: baml.ai.Provider = s31_PlainEcho { reply: "plain answer" };

    assert.is_true(s31_supports_managed_cache(managed))
    assert.is_true(!s31_supports_managed_cache(plain))
}

test "31: an app negotiates ManagedCache and degrades to a plain call when it is absent" {
    let plain: baml.ai.Provider = s31_PlainEcho { reply: "degraded answer" };

    // Prefer the managed path; degrade to a plain single-turn call if unsupported.
    let result = match (plain) {
        let mc: baml.ai.ManagedCache => "used managed cache",
        _ => s31_Ask("question", client = plain) catch (e) { _ => "degrade failed" },
    };
    log.info("31 negotiation-degrade: " + result);
    assert.equal(result, "degraded answer")   // fell through to the plain call
}

test "31: an effectful managed provider refuses a blind retry re-drive (no double-provision)" {
    // A managed-cache provider is effectful (server-stored, billed state). Wrapping
    // it in Retry and forcing a failure must NOT silently re-drive it — Retry
    // consults is_effectful() and throws CannotRetry. (design D2; billed-effect safety.)
    let fake = s31_fresh_fake();
    let verdict = s31_Ask("q", client = fake.with_retry(3)) catch (e) {
        let cr: baml.errors.CannotRetry => "refused/effectful=" + cr.is_effectful().to_string(),
        _ => "other",
    };
    log.info("31 effectful-refusal: " + verdict);
    assert.equal(verdict, "refused/effectful=true")
}


// ============================================================================
// LIVE TIER — BLOCKED. No `testset "integ-test-31-caching"` ships, and here is
// the honest accounting of why each caching shape is not live-assertable today:
//
//  (D) ManagedCache (Gemini `cachedContents`) — BLOCKED: no host surface. The
//      shipped `baml.ai.ManagedCache` (ns_ai/capabilities/cache.baml) has NO
//      `$rust_io` backing — create_cache/delete_cache have no wire implementation
//      to hit a real endpoint. This is the Family-A1 lifecycle gap called out in
//      REALIZED.md ("server-side cache with a finalizer-needing handle; host
//      surface pending"). Offline-only until the host seam lands.
//
//  (B) Anthropic inline `cache_control` — BLOCKED: no per-content-block seam. The
//      annotation must be stamped INSIDE a message/system content block; the
//      shipped `Anthropic` provider only exposes `extra_body`, which merges at the
//      TOP level of the request, so it cannot reach a content block. There is no
//      honest way to place a `cache_control` breakpoint from app code today.
//
//  (C) OpenAI automatic prefix cache / `prompt_cache_key` — BLOCKED as a *caching*
//      assertion: the discount is invisible from BAML (no `cached_tokens` /
//      `cache_read_input_tokens` dimension on `ResponseMeta`), and a plain call
//      succeeds with OR without a cache hit, so "assert success" would prove
//      nothing about caching. Dishonest to label it a cache test; omitted.
//
//  (A) client-side response `Cache` combinator / `.cached(store)` — BLOCKED:
//      net-new surface absent from the shipped stdlib. There is no `Cache`
//      combinator and no `.cached()` factory on the `Provider` marker
//      (only `.with_retry` / `.fallback_to` / `.traced` shipped).
//
// Net: the ONE caching mechanism with a live-capable path (managed caches) lacks
// the host backing; the others lack either a placement seam or an observable
// discount. Live stays absent; the offline tier above is the proven surface.
// ============================================================================
