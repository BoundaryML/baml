// LLM functions whose return types contain `image` compile end-to-end:
// plain image, image[], and (image | string)[] outputs.

client OpenAIImageResponses = openai.OpenAiClient.new(
    model = "gpt-4.1",
    api_key = "test-key",
);

function GenerateImage(prompt: string) -> image {
  client: OpenAIImageResponses
  prompt: `
    Generate an image for this prompt:
    ${prompt}

    ${ctx.output_format}
  `
}

function GenerateImageSet(prompt: string) -> image[] {
  client: OpenAIImageResponses
  prompt: `
    Generate multiple distinct image outputs for this prompt:
    ${prompt}

    ${ctx.output_format}
  `
}

function GenerateMixedImageNarrative(prompt: string) -> (image | string)[] {
  client: OpenAIImageResponses
  prompt: `
    Generate a short narrative with text captions and image outputs for:
    ${prompt}

    ${ctx.output_format}
  `
}
