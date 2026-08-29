// A `prompt` accepts either string-literal form, and both lower to the same
// prompt-tag closure. A quoted prompt is the degenerate template: regular
// strings do not interpolate, so `${...}` stays literal text and earns a
// warning, while a backtick prompt interpolates and can escape with `\${`.
//
// The quoted form previously synthesized no `@spec` and no diagnostic, leaving
// the function with an empty body that the VM entered and read past the end of.
// These snapshots pin that both forms emit a real body.

client QuotedPromptClient = openai.ResponsesClient.new(
    model = "gpt-4.1",
    api_key = "test-key",
);

function QuotedPrompt(topic: string) -> string {
  client: QuotedPromptClient
  prompt: "Write one sentence. Escapes decode:\n\tdone. Interpolation stays literal: ${topic}"
}

function BacktickPrompt(topic: string) -> string {
  client: QuotedPromptClient
  prompt: `
    Write one sentence about ${topic}.
    An escaped marker stays literal and does not warn: \${topic}

    ${ctx.output_format()}
  `
}
