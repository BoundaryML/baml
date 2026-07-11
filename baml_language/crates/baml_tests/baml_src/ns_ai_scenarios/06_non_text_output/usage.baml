// 06 — non-text output is an ordinary typed task result.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s06_DrawFox(subject: string) -> image {
//   client: s06_FixtureImage {
//     base64_data: "aW1hZ2U=",
//     mime: "image/png",
//   }
//   prompt: `Draw ${subject}.`
// }

// Manual expansion reference for s06_DrawFox.
// The helper names below are hand-written reference/test scaffolding, not
// literal compiler-generated companion names. `provider` stands in for the
// task's synthetic trailing `client =` argument.
// - s06_DrawFox_request(...) models s06_DrawFox.request(...): it renders and
//   packages Request<image> without performing I/O.
// - s06_DrawFox_manual(...) models the plain s06_DrawFox(...) lowering:
//   root.ai.run(request), returning the image value.
// - s06_DrawFox_with_meta_manual(...) models s06_DrawFox.with_meta(...):
//   root.ai.run_with_meta(request), preserving media metadata from the same call.
function s06_DrawFox_request(
  subject: string,
  provider: root.ai.Provider = s06_FixtureImage {
    base64_data: "aW1hZ2U=",
    mime: "image/png",
  },
) -> root.ai.Request<image> {
  root.ai.task_request<image>(
    provider,
    "s06_DrawFox",
    "ns_ai_scenarios",
    { "subject": subject },
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template(`Draw ${subject}.`, false),
  )
}

function s06_DrawFox_manual(
  subject: string,
  provider: root.ai.Provider = s06_FixtureImage {
    base64_data: "aW1hZ2U=",
    mime: "image/png",
  },
) -> image throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<image>(s06_DrawFox_request(subject, provider = provider))
}

function s06_DrawFox_with_meta_manual(
  subject: string,
  provider: root.ai.Provider,
) -> root.ai.Response<image> throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run_with_meta<image>(s06_DrawFox_request(subject, provider = provider))
}

test "06: an image result uses the builtin media type" {
  let img = s06_DrawFox_manual("a fox");
  assert.equal(img.mime_type(), "image/png")
  assert.equal(img.base64(), "aW1hZ2U=")
}

test "06: media-specific facts ride response metadata" {
  let response = s06_DrawFox_with_meta_manual(
    "a fox",
    s06_FixtureImage { base64_data: "aW1hZ2U=", mime: "image/png" },
  );
  assert.not_null(response.meta.attributes.get("generation_kind"))
}

test "06: image-only providers use Generate plus graded modality support" {
  let provider: root.ai.Provider = root.ai.AiGatewayImages {
    model: "bfl/flux-2-pro",
    api_key: "unused",
    base_url: null,
    extra_headers: null,
    extra_body: null,
  };
  assert.is_true(provider is root.ai.Generate)
  match (provider) {
    let capabilities: root.ai.Capabilities => {
      assert.equal(capabilities.image_output(), root.ai.Support.Yes)
      assert.equal(capabilities.image_input(), root.ai.Support.No)
      assert.equal(capabilities.pdf_input(), root.ai.Support.No)
    },
    _ => assert.is_true(false),
  }
}

testset "integ-test-06-non-text-output" {
  test "06 live: OpenAI Responses returns a usable builtin image" {
    let img = s06_DrawFox_manual(
      "a small red fox in a simple watercolor style",
      provider = root.ai.OpenAiResponses {
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      },
    );
    assert.is_true((img.mime_type() ?? "").starts_with("image/"))
    assert.is_true(img.base64().length() > 0)
  }
}
