// Canonical content: the blocks a turn carries, normalized away from any one
// provider's envelope. `Block` is what ANY turn may carry (a user turn, a tool
// result, a model turn); `root.content.ModelBlock` adds what only a model turn produces.
// Clients build these; the journal records them inside root.events.*.
//
// Mirrors _planv2/pages/04_reference/01_api.md (the `ai.content` namespace).

/// A model turn is structured blocks, never a bare string and never an HTTP
/// envelope.
class Text {
    text: string,
}

class Reasoning {
    summary: string,
}

class ToolUse {
    id: string,
    name: string,
    args: map<string, unknown>,
}

/// A media payload: a screenshot a runner attaches to a user turn, a file a
/// tool returned, a generated image a model produced. The value is a real BAML
/// media value, so `-> image` returns something callers can save or pass
/// straight back into another prompt.
///
/// `provider_id` and `revised_prompt` are provenance of GENERATED output and
/// are null on input.
class Media {
    media: root.MediaPart,
    // The provider's own handle for this output, when it names one
    // (Anthropic file ids, for example).
    provider_id: string?,
    // The prompt the provider actually rendered, when it rewrites the ask
    // (OpenAI image generation reports this).
    revised_prompt: string?,

    /// The payload's media kind: "image", "audio", "video", or "pdf".
    function kind(self) -> string {
        match (self.media) {
            let image: baml.media.Image => "image",
            let audio: baml.media.Audio => "audio",
            let video: baml.media.Video => "video",
            let pdf: baml.media.Pdf => "pdf",
        }
    }

    /// Wrap a media value a client just built from a provider payload.
    /// `baml.media.Image.from_base64` and its siblings return the `image` /
    /// `audio` / `video` / `pdf` primitives, and `baml.media.Image` is the
    /// declaration that carries `image`'s members rather than a second
    /// nominal type, so the value already is a `MediaPart` member.
    function new(
        value: image | audio | video | pdf,
        provider_id: string? = null,
        revised_prompt: string? = null,
    ) -> Media {
        Media { media: value, provider_id: provider_id, revised_prompt: revised_prompt }
    }
}

/// What any turn may carry.
type Block = Text | Media;

/// What a model turn produces: any `Block` plus the model-only reasoning and
/// tool-use blocks. Every other turn carries `Block[]`.
type ModelBlock = Block | Reasoning | ToolUse;

/// Content blocks from ordinary values: a bare `string` becomes `Text`, a bare
/// `image` / `audio` / `video` / `pdf` becomes `Media` with null provenance, and
/// a block already built passes through. The one constructor behind
/// `root.events.UserMessage.of` and `root.events.ToolCompleted.of`.
function blocks(items: (string | image | audio | video | pdf | Text | Media)[]) -> Block[] {
    let out: Block[] = [];
    for (let item in items) {
        match (item) {
            let text: string => {
                out.push(Text { text: text });
                null
            },
            let block: Block => {
                out.push(block);
                null
            },
            let media: image | audio | video | pdf => {
                out.push(Media.new(media));
                null
            },
        };
    }
    out
}

/// The readable projection of a block list: `Text` blocks as written, each
/// `Media` block as a `[image]`-style placeholder, joined in order.
function text(content: Block[]) -> string {
    let out = "";
    for (let block in content) {
        match (block) {
            let t: Text => {
                out = `${out}${t.text}`;
                null
            },
            let m: Media => {
                out = `${out}[${m.kind()}]`;
                null
            },
        };
    }
    out
}

/// Why the model stopped. `ToolUse` means the turn ended with tool calls the
/// runner must execute before the next turn.
enum StopReason {
    Complete,
    ToolUse,
    MaxTokens,
    Refused,
}
