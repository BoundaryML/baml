// 05 — multimodal arguments survive the same Request<T> seam.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s05_DescribeImage(img: image) -> s05_Caption {
//   client: BepV2FixtureGenerate {
//     name: "vision-default",
//     reply: `{"text":"a red square","tags":["red","square"]}`,
//     attributes: {},
//   }
//   prompt: `Describe this image: ${img}. ${ctx.output_format}`
// }

// This is the desugared code for s05_DescribeImage --------------------------
function s05_DescribeImage_request(
  img: image,
  provider: root.ai.Provider = BepV2FixtureGenerate {
    name: "vision-default",
    reply: `{"text":"a red square","tags":["red","square"]}`,
    attributes: {},
  },
) -> root.ai.Request<s05_Caption> {
  root.ai.task_request_messages<s05_Caption>(
    provider,
    "s05_DescribeImage",
    "ns_ai_scenarios",
    { "img": img },
    [],
    root.ai.RequestOptions {},
    {},
    [root.ai.ChatMessage {
      role: "user",
      parts: [
        root.ai.MessagePart.from_text("Describe this image."),
        root.ai.MessagePart { image: img },
      ],
    }],
    s05_image_prompt_template("Describe this image:", img),
  )
}

function s05_DescribeImage_manual(
  img: image,
  provider: root.ai.Provider = BepV2FixtureGenerate {
    name: "vision-default",
    reply: `{"text":"a red square","tags":["red","square"]}`,
    attributes: {},
  },
) -> s05_Caption throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<s05_Caption>(s05_DescribeImage_request(img, provider = provider))
}

test "05: an image can be captured by the manual task request" {
  let red = baml.media.Image.from_base64(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Y9ZsmAAAAAASUVORK5CYII=",
    "image/png",
  );
  let caption = s05_DescribeImage_manual(red);
  assert.equal(caption.tags, ["red", "square"])
}

testset "integ-test-05-multimodal" {
  test "05 live: an inline image runs through the manual request" {
    let red = baml.media.Image.from_base64(
      "iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAIAAAAlC+aJAAAAb0lEQVR4nO3PAQkAAAyEwO9feoshgnABdLep8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3IPanc8OLDQitxAAAAAElFTkSuQmCC",
      "image/png",
    );
    let r = s05_DescribeImage_manual(red, provider = root.ai.OpenAi {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null,
    });
    assert.is_true(r.text.to_lower_case().includes("red") || r.tags.length() > 0)
  }
}
