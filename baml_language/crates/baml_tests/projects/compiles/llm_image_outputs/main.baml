// LLM functions whose return types contain `image` compile end-to-end:
// plain image, image[], and (image | string)[] outputs. The `openai-images/`
// shorthand constructs openai.ImageClient, whose turns carry media blocks that
// the runner builds these output types from.

function GenerateImage(prompt: string) -> image {
  client: OpenAIImageResponses
  prompt: `
    Generate an image for this prompt:
    ${prompt}
  `
}

function GenerateImageSet(prompt: string) -> image[] {
  client: OpenAIImageResponses
  prompt: `
    Generate multiple distinct image outputs for this prompt:
    ${prompt}
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
