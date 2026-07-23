// Hand-written reference expansion of the compiler task surface for `s31_Ask`;
// these helper names are scaffolding, not literal generated names:
// - `s31_Ask_request` represents `s31_Ask.request(...)`.
// - `s31_Ask_manual` spells the plain `s31_Ask(...)` lowering through `root.ai.run`.
// - `s31_Ask_cached_manual` is a custom Cache consumer of the same request, not
//   a compiler-generated task modifier.
function s31_Ask_request(
  question: string,
  provider: root.ai.Provider = root.ai.FakeGenerateProvider { name: "s31-default", reply: `"unused"`, attributes: {} },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider, "s31_Ask", "ns_ai_scenarios", { "question": question },
    [], root.ai.RequestOptions {}, {}, root.ai.prompt_template(question, false),
  )
}

function s31_Ask_manual(
  question: string,
  provider: root.ai.Provider,
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s31_Ask_request(question, provider = provider))
}

function s31_Ask_cached_manual(
  question: string,
  cache: root.ai.Cache,
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  cache.run<string>(s31_Ask_request(question))
}

test "31: managed cache lifecycle creates, serves, and deletes" {
  let fake = s31_fresh_fake();
  let cache = root.ai.create_cache(
    fake, [root.ai.ChatMessage.system("You are a contract analyst.")],
    root.ai.CacheOptions { ttl: baml.time.Duration.from_hours(1) },
  );
  assert.equal(s31_Ask_cached_manual("termination clause?", cache), "cached answer")
  assert.contains(fake.ledger.last_prompt, "contract analyst")
  cache.delete();
  assert.equal(fake.ledger.active_caches, 0)
}

test "31: a managed cache hit uses fewer input tokens than a cold call" {
  let fake = s31_fresh_fake();
  let _ = s31_Ask_manual("cold", fake);
  let cache = root.ai.create_cache(
    fake, [root.ai.ChatMessage.system("large reference")],
    root.ai.CacheOptions { ttl: baml.time.Duration.from_hours(1) },
  );
  let _ = s31_Ask_cached_manual("warm", cache);
  assert.is_true(fake.ledger.cached_input_tokens < fake.ledger.cold_input_tokens)
  cache.cleanup();
}

test "31: a live cache remains billable until delete or cleanup" {
  let fake = s31_fresh_fake();
  let cache = root.ai.create_cache(
    fake, [root.ai.ChatMessage.system("reference")],
    root.ai.CacheOptions { ttl: baml.time.Duration.from_hours(1) },
  );
  assert.equal(fake.ledger.active_caches, 1)
  cache.delete();
  assert.equal(fake.ledger.active_caches, 0)
}

test "31: using a deleted cache throws the typed expired error" {
  let fake = s31_fresh_fake();
  let cache = root.ai.create_cache(
    fake, [root.ai.ChatMessage.system("reference")],
    root.ai.CacheOptions { ttl: baml.time.Duration.from_hours(1) },
  );
  cache.delete();
  let label = s31_Ask_cached_manual("q", cache) catch (e) {
    let expired: s31_ExpiredCache => "expired",
    _ => "other",
  };
  assert.equal(label, "expired")
}

test "31: with_cache reuses one resource and always cleans it up" {
  let fake = s31_fresh_fake();
  let answers = root.ai.with_cache(
    fake, [root.ai.ChatMessage.system("reference")],
    root.ai.CacheOptions { ttl: baml.time.Duration.from_hours(1) },
    (cache) -> string[] {
      [s31_Ask_cached_manual("one", cache), s31_Ask_cached_manual("two", cache)]
    },
  );
  assert.equal(answers.length(), 2)
  assert.equal(fake.ledger.active_caches, 0)
}

test "31: with_cache cleans the resource when the body throws" {
  let fake = s31_fresh_fake();
  let label = root.ai.with_cache(
    fake, [root.ai.ChatMessage.system("reference")],
    root.ai.CacheOptions { ttl: baml.time.Duration.from_hours(1) },
    (cache) -> string throws baml.errors.UnknownError {
      throw baml.errors.UnknownError { data: null, message: ["body failed"] }
    },
  ) catch (e) {
    let u: baml.errors.UnknownError => "failed",
  };
  assert.equal(label, "failed")
  assert.equal(fake.ledger.active_caches, 0)
}

test "31: ManagedCache is negotiated separately from Generate" {
  let managed: root.ai.Provider = s31_fresh_fake();
  let plain: root.ai.Provider = root.ai.FakeGenerateProvider { name: "plain", reply: `"plain"`, attributes: {} };
  assert.is_true(s31_supports_managed_cache(managed))
  assert.is_true(!s31_supports_managed_cache(plain))
}

test "31: an app can degrade to a plain run when ManagedCache is absent" {
  let plain: root.ai.Provider = root.ai.FakeGenerateProvider { name: "plain", reply: `"degraded answer"`, attributes: {} };
  let answer = match (plain) {
    let managed: root.ai.ManagedCache => "used managed cache",
    _ => s31_Ask_manual("q", plain),
  };
  assert.equal(answer, "degraded answer")
}

test "31: a Generate retry wrapper does not duplicate cache provisioning" {
  let fake: root.ai.Provider = s31_fresh_fake();
  let guarded: root.ai.Provider = fake.with_retry(3);
  assert.is_true(!s31_supports_managed_cache(guarded))
}

// No live testset: provider-managed cache APIs are provider-specific and none of
// the configured live providers in this reference layer advertises ManagedCache.
