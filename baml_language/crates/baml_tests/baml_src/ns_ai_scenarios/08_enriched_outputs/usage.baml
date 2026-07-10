// 08 — citations, scores, and grounding live in Response.meta.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s08_Research(question: string) -> s08_Answer {
//   client: s08_EnrichedFixture {
//     reply: `{"text":"Water boils at 100 C at sea level."}`,
//     source_url: "https://example.test/water",
//   }
//   prompt: `Answer ${question}. ${ctx.output_format}`
// }

// This is the desugared code for s08_Research -------------------------------
function s08_Research_request(
  question: string,
  provider: root.ai.Provider = s08_EnrichedFixture {
    reply: `{"text":"Water boils at 100 C at sea level."}`,
    source_url: "https://example.test/water",
  },
) -> root.ai.Request<s08_Answer> {
  root.ai.task_request<s08_Answer>(
    provider,
    "s08_Research",
    "ns_ai_scenarios",
    { "question": question },
    [],
    root.ai.RequestOptions {},
    {},
    bepv2_prompt_template(`Answer ${question}.`, true),
  )
}

function s08_Research_manual(
  question: string,
  provider: root.ai.Provider = s08_EnrichedFixture {
    reply: `{"text":"Water boils at 100 C at sea level."}`,
    source_url: "https://example.test/water",
  },
) -> s08_Answer throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<s08_Answer>(s08_Research_request(question, provider = provider))
}

function s08_Research_with_meta_manual(
  question: string,
  provider: root.ai.Provider,
) -> root.ai.Response<s08_Answer> throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run_with_meta<s08_Answer>(s08_Research_request(question, provider = provider))
}

test "08: enriched output keeps the answer strongly typed" {
  assert.is_true(s08_Research_manual("When does water boil?").text.includes("100"))
}

test "08: provider-specific enrichment stays in generic metadata" {
  let r = s08_Research_with_meta_manual(
    "When does water boil?",
    s08_EnrichedFixture {
      reply: `{"text":"Water boils at 100 C at sea level."}`,
      source_url: "https://example.test/water",
    },
  );
  assert.not_null(r.meta.attributes.get("source_url"))
}

testset "integ-test-08-enriched-outputs" {
  test "08 live: OpenAI enrichment still uses the manual with-meta path" {
    let r = s08_Research_with_meta_manual(
      "What is the capital of France?",
      root.ai.OpenAi {
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
        extra_body: null,
      },
    );
    assert.is_true(r.value.text.length() > 0)
    assert.is_true((r.meta.usage?.output_tokens ?? 0) > 0)
  }

  test "08 live: Anthropic exposes unavailable dimensions in metadata" {
    let r = s08_Research_with_meta_manual(
      "What is the capital of France?",
      root.ai.Anthropic {
        model: "claude-haiku-4-5-20251001",
        api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
        base_url: null,
        max_tokens: null,
        anthropic_version: null,
        extra_headers: null,
        extra_body: null,
        append_output_schema: null,
      },
    );
    assert.is_true(r.value.text.length() > 0)
    assert.is_true((r.meta.usage?.output_tokens ?? 0) > 0)
  }
}
