// 01 — one typed task, one Request<T>, one Generate negotiation.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s01_Summarize(work: string) -> string {
//   client: root.ai.FixtureGenerate {
//     name: "default-summary-model",
//     reply: `"fixture summary"`,
//     attributes: {},
//   }
//   prompt: `Summarize this in one sentence: ${work}`
// }

// Manual expansion reference for s01_Summarize.
// The helper names below are hand-written reference/test scaffolding, not
// literal compiler-generated companion names. `provider` stands in for the
// task's synthetic trailing `client =` argument.
// - s01_Summarize_request(...) models s01_Summarize.request(...): it renders
//   and packages Request<string> without performing I/O.
// - s01_Summarize_manual(...) models the plain s01_Summarize(...) lowering:
//   root.ai.run(request), returning only the typed value.
// - s01_Summarize_with_meta_manual(...) models s01_Summarize.with_meta(...):
//   root.ai.run_with_meta(request), preserving metadata from the same call.
function s01_Summarize_request(
  work: string,
  provider: root.ai.Provider = root.ai.FixtureGenerate {
    name: "default-summary-model",
    reply: `"fixture summary"`,
    attributes: {},
  },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s01_Summarize",
    "ns_ai_scenarios",
    { "work": work },
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template(`Summarize this in one sentence: ${work}`, false),
  )
}

function s01_Summarize_manual(
  work: string,
  provider: root.ai.Provider = root.ai.FixtureGenerate {
    name: "default-summary-model",
    reply: `"fixture summary"`,
    attributes: {},
  },
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s01_Summarize_request(work, provider = provider))
}

function s01_Summarize_with_meta_manual(
  work: string,
  provider: root.ai.Provider,
) -> root.ai.Response<string> throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run_with_meta<string>(s01_Summarize_request(work, provider = provider))
}

test "01: manual request runs through Generate" {
  assert.equal(s01_Summarize_manual("a long story"), "fixture summary")
}

test "01: with-meta uses the same request and does not make another call" {
  let r = s01_Summarize_with_meta_manual(
    "a long story",
    root.ai.FixtureGenerate {
      name: "measured-fixture",
      reply: `"short"`,
      attributes: { "route": "primary" },
    },
  );
  assert.equal(r.value, "short")
  assert.equal(r.meta.provider, "measured-fixture")
  assert.equal(r.meta.usage?.output_tokens, 2)
}

testset "integ-test-01-single-turn" {
  test "01 live: the same manual request runs through OpenAI" {
    let r = s01_Summarize_manual(
      "Reply with exactly the lowercase word pong.",
      provider = root.ai.OpenAi {
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      },
    ) catch (e) { _ => "live call failed" };
    assert.is_true(r.to_lower_case().includes("pong"))
  }

  test "01 live: the same manual request runs through Anthropic" {
    let r = s01_Summarize_manual(
      "Reply with exactly the lowercase word pong.",
      provider = root.ai.Anthropic {
        model: "claude-haiku-4-5-20251001",
        api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
        base_url: null,
        max_tokens: null,
        anthropic_version: null,
        extra_headers: null,
        extra_body: null,
        append_output_schema: null,
      },
    ) catch (e) { _ => "live call failed" };
    assert.is_true(r.length() > 0 && r != "live call failed")
  }

  test "01 live: the same manual request runs through Gemini" {
    let r = s01_Summarize_manual(
      "Reply with exactly the lowercase word pong.",
      provider = root.ai.Gemini {
        model: "gemini-flash-latest",
        api_key: baml.env.get_or_panic("GOOGLE_API_KEY"),
        base_url: null,
        extra_headers: null,
        extra_body: null,
        append_output_schema: null,
      }.with_retry(2),
    ) catch (e) { _ => "live call failed" };
    assert.is_true(r.to_lower_case().includes("pong"))
  }

  test "01 live: metadata comes back with the same invocation" {
    let r = s01_Summarize_with_meta_manual(
      "Summarize Hamlet in one line.",
      root.ai.OpenAi {
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      },
    );
    assert.is_true(r.value.length() > 0)
    assert.is_true((r.meta.usage?.output_tokens ?? 0) > 0)
  }
}
