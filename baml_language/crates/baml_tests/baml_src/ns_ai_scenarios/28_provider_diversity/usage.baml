// 28 — one typed task, several provider implementations and wire protocols.
class s28_Profile { name: string, title: string, years: int }

// Task sketch: s28_Extract(bio) -> s28_Profile.

// Manual expansion reference for task `s28_Extract`:
// - `s28_Extract_request(...)` models `s28_Extract.request(...)`.
// - `s28_Extract_manual(...)` models the plain `s28_Extract(...)` call,
//   lowered through `root.ai.run`.
// These helper names are hand-written reference/test scaffolding, not literal
// compiler-generated names.
function s28_Extract_request(
  bio: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "profile", reply: `{"name":"Ada","title":"Analyst","years":12}`, attributes: {} },
) -> root.ai.Request<s28_Profile> {
  root.ai.task_request<s28_Profile>(
    provider, "s28_Extract", "ns_ai_scenarios", { "bio": bio },
    [], root.ai.RequestOptions {}, {}, bepv2_prompt_template(`Extract a profile.\n${bio}`, true),
  )
}

function s28_Extract_manual(
  bio: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "profile", reply: `{"name":"Ada","title":"Analyst","years":12}`, attributes: {} },
) -> s28_Profile throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<s28_Profile>(s28_Extract_request(bio, provider = provider))
}

test "28: one task runs through a swapped provider" {
  let r = s28_Extract_manual(
    "Alan Turing was a cryptographer.",
    provider = BepV2FixtureGenerate { name: "anthropic-shaped", reply: `{"name":"Alan Turing","title":"Cryptographer","years":15}`, attributes: {} },
  );
  assert.equal(r.name, "Alan Turing")
  assert.equal(r.years, 15)
}

test "28: a string registry returns an existential provider" {
  let p = s28_resolve("openai/gpt-5.6-luna");
  assert.is_true(p is root.ai.Provider)
}

testset "integ-test-28-provider-diversity" {
  test "28 live: extract through a native OpenAI override" {
    let r = s28_Extract_manual("Ada Lovelace is an analyst with 12 years of experience.", provider = root.ai.OpenAi {
      model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
    });
    assert.is_true(r.name.length() > 0)
  }

  test "28 live: the SAME body through a native Anthropic override" {
    let r = s28_Extract_manual("Alan Turing is a cryptographer with 15 years of experience.", provider = root.ai.Anthropic {
      model: "claude-haiku-4-5-20251001", api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
      base_url: null, max_tokens: null, anthropic_version: null,
      extra_headers: null, extra_body: null, append_output_schema: null,
    });
    assert.is_true(r.name.length() > 0)
  }

  test "28 live: the SAME body through a native Gemini override" {
    let r = s28_Extract_manual("Grace Hopper was a compiler pioneer with 30 years of experience.", provider = root.ai.Gemini {
      model: "gemini-flash-latest", api_key: baml.env.get_or_panic("GOOGLE_API_KEY"),
      base_url: null, extra_headers: null, extra_body: null, append_output_schema: null,
    });
    assert.is_true(r.name.length() > 0)
  }

  test "28 live: switch backend by changing a STRING — the registry path" {
    let r = s28_Extract_manual("Ada Lovelace is an analyst with 12 years of experience.", provider = s28_resolve("openai/gpt-5.6-luna"));
    assert.is_true(r.name.length() > 0)
  }
}
