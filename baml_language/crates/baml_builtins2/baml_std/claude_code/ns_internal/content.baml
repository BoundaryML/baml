// Claude Code CLI internals: how one turn's prompt reaches the process.

/// (internal) One CLI run's whole prompt as the single stream-json user
/// message the CLI reads on stdin (`--input-format stream-json`), newline
/// terminated. Every prompt takes this path, text-only or not: argv cannot
/// carry media (`text()` folds each media part to a placeholder the model
/// never sees), Linux caps one argument at 128 KiB and macOS the whole argv
/// at 1 MiB, and an argv prompt that starts with `-` parses as an option.
/// Exactly one message: the CLI runs one turn, and emits one result, per
/// stdin message. `suffix` is the text the client appends after the prompt:
/// the folded journal and the tool protocol.
function stdin_message(prompt: ai.Prompt, suffix: string) -> string {
    let message: json = { "type": "user", "message": { "role": "user", "content": content_blocks(prompt, suffix) } };
    `${baml.json.stringify(message)}\n`
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
    let media_type = if (block_type == "image") {
        // the check is case-insensitive; send the lowercase form the API documents
        let lower = resolved.mime_type.to_lower_case();
        if (!_supported_image_mime(lower)) {
            throw _reject(
                `claude-code does not accept ${resolved.mime_type} images: it supports image/jpeg, image/png, image/gif and image/webp`,
            )
        }
        lower
    } else {
        resolved.mime_type
    };
    {
        "type": block_type,
        "source": { "type": "base64", "media_type": media_type, "data": resolved.base64 ?? "" },
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
