// Claude Code CLI internals: how one turn's prompt reaches the process.

/// (internal) One CLI run's prompt transport: the argv entries that carry it,
/// and the complete stdin when it travels there instead.
class CliPrompt {
    args: string[],
    stdin: string?,
    /// The readable prompt, as `prompt.text()` renders it, plus the suffix.
    text: string,
}

/// (internal) A text-only prompt rides argv as one readable string, as it
/// always has. A prompt carrying media cannot: `text()` folds each media part
/// to a placeholder the model never sees, and inline base64 outgrows argv
/// (macOS caps the whole argv at 1 MiB). It goes instead as one stream-json
/// user message on stdin, whose content blocks keep the media. `suffix` is
/// the text the client appends after the prompt: the folded journal and the
/// tool protocol.
function cli_prompt(prompt: ai.Prompt, suffix: string) -> CliPrompt {
    let text = `${prompt.text()}${suffix}`;
    if (!_has_media(prompt)) {
        return CliPrompt { args: [text], stdin: null, text: text };
    }
    let message: json = { "type": "user", "message": { "role": "user", "content": content_blocks(prompt, suffix) } };
    CliPrompt {
        args: ["--input-format", "stream-json"],
        stdin: `${baml.json.stringify(message)}\n`,
        text: text,
    }
}

/// (internal) Whether any prompt message carries a media part.
function _has_media(prompt: ai.Prompt) -> bool {
    for (let m in prompt.messages()) {
        for (let part in m.parts) {
            match (part) {
                let media: ai.MediaPart => {
                    return true;
                },
                _ => null,
            };
        }
    }
    false
}

/// (internal) The prompt as Messages API content blocks, then `suffix`. The
/// text reads exactly as `prompt.text()` renders it (a `[role]` header per
/// message, a blank line between messages); each media part lowers in place
/// of its placeholder. Cache delimiters drop: the CLI manages its own cache.
function content_blocks(prompt: ai.Prompt, suffix: string) -> json[] {
    let blocks: json[] = [];
    let pending = "";
    let first = true;
    for (let m in prompt.messages()) {
        if (!first) {
            pending = pending + "\n\n";
        }
        first = false;
        if (m.role != "") {
            pending = `${pending}[${m.role}]\n`;
        }
        for (let part in m.parts) {
            match (part) {
                let text: string => {
                    pending = pending + text;
                    null
                },
                let media: ai.MediaPart => {
                    _push_text(blocks, pending);
                    pending = "";
                    blocks.push(_media_block(media));
                    null
                },
                let delimiter: ai.CacheDelimiter => null,
            };
        }
    }
    _push_text(blocks, pending + suffix);
    blocks
}

/// (internal) The API rejects empty and whitespace-only text blocks, and a
/// rendered prompt routinely produces them around media parts.
function _push_text(blocks: json[], text: string) -> void {
    if (text.trim() != "") {
        blocks.push({ "type": "text", "text": text });
    }
}

/// (internal) One media part as a content block, shaped as the Messages API
/// takes it: a URL source passes through, anything else is base64. The CLI
/// has no audio or video input, so those are a typed error, never a silent
/// placeholder.
function _media_block(media: ai.MediaPart) -> json {
    let block_type = match (media) {
        let image: baml.media.Image => "image",
        let pdf: baml.media.Pdf => "document",
        let audio: baml.media.Audio => {
            throw _reject("claude-code does not support audio prompt input")
        },
        let video: baml.media.Video => {
            throw _reject("claude-code does not support video prompt input")
        },
    };
    let resolved = ai.wire.resolve_media(media, false);
    if let url: string = resolved.url {
        return { "type": block_type, "source": { "type": "url", "url": url } };
    }
    if (block_type == "image" && !_supported_image_mime(resolved.mime_type)) {
        throw _reject(
            `claude-code does not accept ${resolved.mime_type} images: it supports image/jpeg, image/png, image/gif and image/webp`,
        )
    }
    {
        "type": block_type,
        "source": { "type": "base64", "media_type": resolved.mime_type, "data": resolved.base64 ?? "" },
    }
}

/// (internal) The four image media types the Messages API accepts on a
/// base64 source; anything else is a 400 once the CLI sends it.
function _supported_image_mime(mime: string) -> bool {
    let m = mime.to_lower_case();
    m == "image/jpeg" || m == "image/png" || m == "image/gif" || m == "image/webp"
}

function _reject(detail: string) -> ai.errors.InvalidRequest {
    ai.errors.InvalidRequest {
        provider: "claude-code",
        status_code: null,
        detail: detail,
        raw_body: null,
    }
}
