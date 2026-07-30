// Tests for the images_pdfs_and_audio scenario.
test "guide 05.01: messages expose image, PDF, and audio parts uniformly" {
  let parts: ai.MessagePart[] = [
    ai.messages.ImagePart {
      value: image.from_url("https://example.com/receipt.png", "image/png"), metadata: {},
    },
    ai.messages.PdfPart {
      value: pdf.from_url("https://example.com/invoice.pdf", null), metadata: {},
    },
    ai.messages.AudioPart {
      value: audio.from_url("https://example.com/note.wav", "audio/wav"), metadata: {},
    },
  ];
  assert.equal(parts[0].kind(), ai.MessagePartKind.Image)
  assert.equal(parts[1].kind(), ai.MessagePartKind.Pdf)
  assert.equal(parts[2].kind(), ai.MessagePartKind.Audio)
}

test "guide 05.01: a PDF stays structural through the task prompt" {
  let task = InspectPdf@task(pdf.from_url("https://example.com/invoice.pdf", "application/pdf")).with_provider(ai.testing.fake_output_provider(`"unused"`));
  let rendered = task.prompt.text();
  assert.contains(rendered, "pdf::url")
  assert.contains(rendered, "https://example.com/invoice.pdf")
}

testset "integ-test-guide-05-image-input" {
  test "guide 05.01 live OpenAI: image input uses the same task abstraction" {
    let task = InspectImage@task(image.from_url("https://upload.wikimedia.org/wikipedia/commons/3/3f/Fronalpstock_big.jpg", "image/jpeg")).with_provider(root.openai.OpenAIProvider {
        ...root.openai.responses(),
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      });
    let value = ai.internal
      ._require_done(task.run(runner = ai.run.Agent<string>.new()))
      .value;
    assert.is_true(value.length() > 0)
  }

  test "guide 05.01 live Anthropic: PDF input reaches the same task abstraction" {
    let task = InspectPdf@task(pdf.from_url(
        "https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf",
        "application/pdf",
      )).with_provider(live_anthropic());
    let value = ai.internal
      ._require_done(task.run(runner = ai.run.Agent<string>.new()))
      .value;
    log.info(baml.json.stringify({
      "guide": "05.01",
      "media": "pdf",
      "summary_length": value.length(),
    }));
    assert.contains(value.to_lower_case(), "dummy")
  }
}
