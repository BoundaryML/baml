// 06 — non-text output is still a typed task result.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s06_DrawFox(subject: string) -> s06_Media {
//   client: BepV2FixtureGenerate {
//     name: "image-default",
//     reply: `{"mime":"image/png","data":"aW1hZ2U=","url":null}`,
//     attributes: { "generation_kind": "image" },
//   }
//   prompt: `Draw ${subject}. ${ctx.output_format}`
// }

// This is the desugared code for s06_DrawFox --------------------------------
function s06_DrawFox_request(
  subject: string,
  provider: root.ai.Provider = BepV2FixtureGenerate {
    name: "image-default",
    reply: `{"mime":"image/png","data":"aW1hZ2U=","url":null}`,
    attributes: { "generation_kind": "image" },
  },
) -> root.ai.Request<s06_Media> {
  root.ai.task_request<s06_Media>(
    provider,
    "s06_DrawFox",
    "ns_ai_scenarios",
    { "subject": subject },
    [],
    root.ai.RequestOptions {},
    {},
    bepv2_prompt_template(`Draw ${subject}.`, true),
  )
}

function s06_DrawFox_manual(
  subject: string,
  provider: root.ai.Provider = BepV2FixtureGenerate {
    name: "image-default",
    reply: `{"mime":"image/png","data":"aW1hZ2U=","url":null}`,
    attributes: { "generation_kind": "image" },
  },
) -> s06_Media throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<s06_Media>(s06_DrawFox_request(subject, provider = provider))
}

function s06_DrawFox_with_meta_manual(
  subject: string,
  provider: root.ai.Provider,
) -> root.ai.Response<s06_Media> throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run_with_meta<s06_Media>(s06_DrawFox_request(subject, provider = provider))
}

test "06: a media result is typed data, not a special return channel" {
  let media = s06_DrawFox_manual("a fox");
  assert.equal(media.mime, "image/png")
  assert.not_null(media.data)
}

test "06: media-specific facts ride metadata" {
  let r = s06_DrawFox_with_meta_manual(
    "a fox",
    BepV2FixtureGenerate {
      name: "image-default",
      reply: `{"mime":"image/png","data":"aW1hZ2U=","url":null}`,
      attributes: { "generation_kind": "image" },
    },
  );
  assert.not_null(r.meta.attributes.get("generation_kind"))
}

testset "integ-test-06-non-text-output" {
  test "06 live: dedicated image generation returns a decoded Media" {
    let m = s06_DrawFox_manual("a red fox in watercolor", provider = s06_GptImage {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      size: "1024x1024",
    }) catch (e) {
      _ => s06_Media { mime: "live failed", data: null, url: null },
    };
    assert.is_true(m.mime.starts_with("image/"))
  }
}
