test "guide 05.01: messages expose image, PDF, and audio parts uniformly" {
  let parts: root.ai.MessagePart[] = [
    root.ai.ImagePart {
      value: image.from_url("https://example.com/receipt.png", "image/png"), metadata: {},
    },
    root.ai.PdfPart {
      value: pdf.from_url("https://example.com/invoice.pdf", null), metadata: {},
    },
    root.ai.AudioPart {
      value: audio.from_url("https://example.com/note.wav", "audio/wav"), metadata: {},
    },
  ];
  assert.equal(parts[0].kind(), root.ai.MessagePartKind.Image)
  assert.equal(parts[1].kind(), root.ai.MessagePartKind.Pdf)
  assert.equal(parts[2].kind(), root.ai.MessagePartKind.Audio)
}

test "guide 05.01: a PDF stays structural through the task prompt" {
  let task = InspectPdf_task(
    pdf.from_url("https://example.com/invoice.pdf", "application/pdf"),
    root.ai.fake_output_provider(`"unused"`),
  );
  let rendered = task.prompt.text();
  assert.contains(rendered, "pdf::url")
  assert.contains(rendered, "https://example.com/invoice.pdf")
}

testset "integ-test-guide-05-image-input" {
  test "guide 05.01 live OpenAI: image input uses the same task abstraction" {
    let task = InspectImage_task(
      image.from_url("https://upload.wikimedia.org/wikipedia/commons/3/3f/Fronalpstock_big.jpg", "image/jpeg"),
      root.openai.Chat {
        ...root.openai.chat(),
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      },
    );
    let value = task.run(runner = root.ai.run.Completion<string>.new());
    assert.is_true(value.length() > 0)
  }

  test "guide 05.01 live Anthropic: PDF input reaches the same task abstraction" {
    let task = InspectPdf_task(
      pdf.from_url(
        "https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf",
        "application/pdf",
      ),
      live_anthropic(),
    );
    let value = task.run(runner = root.ai.run.Completion<string>.new());
    log.info(baml.json.stringify({
      "guide": "05.01",
      "media": "pdf",
      "summary_length": value.length(),
    }));
    assert.contains(value.to_lower_case(), "dummy")
  }
}
