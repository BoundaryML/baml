// Canonical assistant content: what a model turn produced, normalized away
// from any one provider's envelope. Clients build these; the journal records
// them inside root.events.AssistantMessage.
//
// Mirrors _planv2/pages/04_reference/01_api.md (the `ai.content` namespace).

// A model turn is structured blocks, never a bare string and never an HTTP
// envelope.
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

// A media payload the turn produced: a generated image, a file a provider-side
// tool wrote. The value is a real BAML media value, so `-> image` returns
// something callers can save or pass straight back into another prompt.
class Media {
    media: root.MediaPart,
    // The provider's own handle for this output, when it names one
    // (Anthropic file ids, for example).
    provider_id: string?,
    // The prompt the provider actually rendered, when it rewrites the ask
    // (OpenAI image generation reports this).
    revised_prompt: string?,

    // The payload's media kind: "image", "audio", "video", or "pdf".
    function kind(self) -> string {
        match (self.media) {
            let image: baml.media.Image => "image",
            let audio: baml.media.Audio => "audio",
            let video: baml.media.Video => "video",
            let pdf: baml.media.Pdf => "pdf",
        }
    }

    // Wrap a media value a client just built from a provider payload.
    // `baml.media.Image.from_base64` and its siblings return the `image` /
    // `audio` / `video` / `pdf` primitives, which are distinct static types
    // from the `baml.media.*` wrapper classes `MediaPart` is a union of, so
    // the value bridges through its BEP-038 `{kind, source, value, mime}`
    // envelope. The decode discriminates on the envelope's `kind`, so the
    // union lands on the wrapper for the value's own media kind.
    function new(
        value: image | audio | video | pdf,
        provider_id: string? = null,
        revised_prompt: string? = null,
    ) -> Media {
        let envelope = baml.json.to_json(value) catch_all (e) {
            _ => {
                throw baml.errors.InvalidArgument { message: `media output could not be encoded: ${e.to_string()}` }
            },
        };
        let part: root.MediaPart = baml.json.from_json<root.MediaPart>(envelope) catch_all (e) {
            _ => {
                throw baml.errors.InvalidArgument { message: `media output could not be decoded: ${e.to_string()}` }
            },
        };
        Media { media: part, provider_id: provider_id, revised_prompt: revised_prompt }
    }
}

type Block = Text | Reasoning | ToolUse | Media;

// Why the model stopped. `ToolUse` means the turn ended with tool calls the
// runner must execute before the next turn.
enum StopReason {
    Complete,
    ToolUse,
    MaxTokens,
    Refused,
}
