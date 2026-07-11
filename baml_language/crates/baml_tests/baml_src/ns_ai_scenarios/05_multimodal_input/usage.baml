// 05 — multimodal arguments survive the same Request<T> seam.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s05_DescribeImage(img: image) -> s05_Caption {
//   client: root.ai.FixtureGenerate {
//     name: "vision-default",
//     reply: `{"text":"a red square","tags":["red","square"]}`,
//     attributes: {},
//   }
//   prompt: `Describe this image: ${img}. ${ctx.output_format}`
// }

// Manual expansion reference for s05_DescribeImage.
// The helper names below are hand-written reference/test scaffolding, not
// literal compiler-generated companion names. `provider` stands in for the
// task's synthetic trailing `client =` argument.
// - s05_DescribeImage_request(...) models s05_DescribeImage.request(...): it
//   packages Request<s05_Caption> while preserving the image as a message part.
// - s05_DescribeImage_manual(...) models the plain s05_DescribeImage(...)
//   lowering: root.ai.run(request).
function s05_DescribeImage_request(
  img: image,
  provider: root.ai.Provider = root.ai.FixtureGenerate {
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
  provider: root.ai.Provider = root.ai.FixtureGenerate {
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


  test "05 live: an inline PDF reaches OpenAI Responses as an input file" {
    let document = baml.media.Pdf.from_base64(
      "JVBERi0xLjQKJeLjz9MKMSAwIG9iago8PCAvVHlwZSAvQ2F0YWxvZyAvUGFnZXMgMiAwIFIgPj4KZW5kb2JqCjIgMCBvYmoKPDwgL1R5cGUgL1BhZ2VzIC9LaWRzIFszIDAgUl0gL0NvdW50IDEgPj4KZW5kb2JqCjMgMCBvYmoKPDwgL1R5cGUgL1BhZ2UgL1BhcmVudCAyIDAgUiAvTWVkaWFCb3ggWzAgMCA2MTIgNzkyXSAvUmVzb3VyY2VzIDw8IC9Gb250IDw8IC9GMSA1IDAgUiA+PiA+PiAvQ29udGVudHMgNCAwIFIgPj4KZW5kb2JqCjQgMCBvYmoKPDwgL0xlbmd0aCA1NyA+PgpzdHJlYW0KQlQgL0YxIDE4IFRmIDcyIDcyMCBUZCAoQkVQdjIgUERGIG1hcmtlcjogT1JCSVQtMjcpIFRqIEVUCmVuZHN0cmVhbQplbmRvYmoKNSAwIG9iago8PCAvVHlwZSAvRm9udCAvU3VidHlwZSAvVHlwZTEgL0Jhc2VGb250IC9IZWx2ZXRpY2EgPj4KZW5kb2JqCnhyZWYKMCA2CjAwMDAwMDAwMDAgNjU1MzUgZiAKMDAwMDAwMDAxNSAwMDAwMCBuIAowMDAwMDAwMDY0IDAwMDAwIG4gCjAwMDAwMDAxMjEgMDAwMDAgbiAKMDAwMDAwMDI0NyAwMDAwMCBuIAowMDAwMDAwMzU0IDAwMDAwIG4gCnRyYWlsZXIKPDwgL1NpemUgNiAvUm9vdCAxIDAgUiA+PgpzdGFydHhyZWYKNDI0CiUlRU9GCg==",
      "application/pdf",
    );
    let marker = s05_ReadPdf_manual(document, provider = root.ai.OpenAiResponses {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null,
    });
    assert.contains(marker.to_upper_case(), "ORBIT-27")
  }
}

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s05_ReadPdf(document: pdf) -> string {
//   client: root.ai.FixtureGenerate {
//     name: "pdf-default",
//     reply: `"ORBIT-27"`,
//     attributes: {},
//   }
//   prompt: `Read the marker printed in this PDF: ${document}`
// }

// Manual expansion reference for s05_ReadPdf.
// The helper names below are hand-written reference/test scaffolding, not
// literal compiler-generated companion names. `provider` stands in for the
// task's synthetic trailing `client =` argument.
// - s05_ReadPdf_request(...) models s05_ReadPdf.request(...): it packages
//   Request<string> while preserving the PDF as a message part.
// - s05_ReadPdf_manual(...) models the plain s05_ReadPdf(...) lowering:
//   root.ai.run(request).
function s05_ReadPdf_request(
  document: pdf,
  provider: root.ai.Provider = root.ai.FixtureGenerate {
    name: "pdf-default",
    reply: `"ORBIT-27"`,
    attributes: {},
  },
) -> root.ai.Request<string> {
  root.ai.task_request_messages<string>(
    provider,
    "s05_ReadPdf",
    "ns_ai_scenarios",
    { "document": document },
    [],
    root.ai.RequestOptions {},
    {},
    [root.ai.ChatMessage {
      role: "user",
      parts: [
        root.ai.MessagePart.from_text("Read the marker printed in this PDF. Return only the marker."),
        root.ai.MessagePart { pdf: document },
      ],
    }],
    s05_pdf_prompt_template(
      "Read the marker printed in this PDF. Return only the marker:",
      document,
    ),
  )
}

function s05_ReadPdf_manual(
  document: pdf,
  provider: root.ai.Provider = root.ai.FixtureGenerate {
    name: "pdf-default",
    reply: `"ORBIT-27"`,
    attributes: {},
  },
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s05_ReadPdf_request(document, provider = provider))
}

test "05: a PDF remains structural in Request.messages" {
  let document = baml.media.Pdf.from_base64("JVBERg==", "application/pdf");
  let request = s05_ReadPdf_request(document);
  assert.equal(request.messages()[0].parts.length(), 2)
  assert.not_null(request.messages()[0].parts[1].pdf)
  assert.equal(s05_ReadPdf_manual(document), "ORBIT-27")
}

test "05: OpenAI Responses advertises model-dependent PDF input support" {
  let provider: root.ai.Provider = root.ai.OpenAiResponses {
    model: "gpt-5.6-luna",
    api_key: "unused",
    base_url: null,
  };
  match (provider) {
    let capabilities: root.ai.Capabilities => {
      assert.equal(capabilities.pdf_input(), root.ai.Support.Conditional)
    },
    _ => assert.is_true(false),
  }
}
