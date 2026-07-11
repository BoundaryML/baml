// 07 — reasoning changes provider configuration, not the task contract.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s07_Diagnose(build_log: string) -> s07_Diagnosis {
//   client: s07_ReasoningFixture {
//     reply: `{"root_cause":"race","fix":"serialize writes","confidence":0.9}`,
//     effort: "high",
//   }
//   prompt: `Diagnose this build failure: ${build_log}. ${ctx.output_format}`
// }

// Manual expansion reference for s07_Diagnose.
// The helper names below are hand-written reference/test scaffolding, not
// literal compiler-generated companion names. `provider` stands in for the
// task's synthetic trailing `client =` argument.
// - s07_Diagnose_request(...) models s07_Diagnose.request(...): it renders and
//   packages Request<s07_Diagnosis> without performing I/O.
// - s07_Diagnose_manual(...) models the plain s07_Diagnose(...) lowering:
//   root.ai.run(request), returning only the diagnosis.
// - s07_Diagnose_with_meta_manual(...) models s07_Diagnose.with_meta(...):
//   root.ai.run_with_meta(request), preserving reasoning metadata.
function s07_Diagnose_request(
  build_log: string,
  provider: root.ai.Provider = s07_ReasoningFixture {
    reply: `{"root_cause":"race","fix":"serialize writes","confidence":0.9}`,
    effort: "high",
  },
) -> root.ai.Request<s07_Diagnosis> {
  root.ai.task_request<s07_Diagnosis>(
    provider,
    "s07_Diagnose",
    "ns_ai_scenarios",
    { "build_log": build_log },
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template(`Diagnose this build failure: ${build_log}.`, true),
  )
}

function s07_Diagnose_manual(
  build_log: string,
  provider: root.ai.Provider = s07_ReasoningFixture {
    reply: `{"root_cause":"race","fix":"serialize writes","confidence":0.9}`,
    effort: "high",
  },
) -> s07_Diagnosis throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<s07_Diagnosis>(s07_Diagnose_request(build_log, provider = provider))
}

function s07_Diagnose_with_meta_manual(
  build_log: string,
  provider: root.ai.Provider,
) -> root.ai.Response<s07_Diagnosis> throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run_with_meta<s07_Diagnosis>(s07_Diagnose_request(build_log, provider = provider))
}

test "07: reasoning configuration does not change the typed value" {
  assert.equal(s07_Diagnose_manual("parallel-only failure").root_cause, "race")
}

test "07: reasoning metadata returns beside the value" {
  let r = s07_Diagnose_with_meta_manual(
    "parallel-only failure",
    s07_ReasoningFixture {
      reply: `{"root_cause":"race","fix":"serialize writes","confidence":0.9}`,
      effort: "high",
    },
  );
  assert.not_null(r.meta.attributes.get("reasoning_effort"))
  assert.equal(r.meta.reasoning_blocks().at(0)?.summary, "fixture reasoning summary")
}

testset "integ-test-07-reasoning" {
  test "07 live: reasoning task and usage use one manual request" {
    let r = s07_Diagnose_with_meta_manual(
      "The test intermittently fails only under parallelism.",
      root.ai.OpenAi {
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      },
    );
    assert.is_true(r.value.root_cause.length() > 0)
    assert.is_true((r.meta.usage?.output_tokens ?? 0) > 0)
  }
}
