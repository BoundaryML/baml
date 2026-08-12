// Shared helpers for client authors: transport with classification, the
// output-format text, and tool schema lowering.
// Mirrors _planv2/pages/02_guides/03_clients/03_writing_a_client.md.

// Send, classify non-2xx via root.errors.classify_http, decode the body as T.
// T = json-shaped envelope class; unknown fields are ignored by the decoder.
function send_as<T>(req: baml.http.Request, provider: string) -> T {
    let resp = baml.http.send(req) catch_all (e) {
        _ => {
            throw root.errors.NetworkFailure {
            provider: provider,
            detail: `transport failed before a response arrived: ${e.to_string()}`,
            raw_body: null,
        }
        },
    };
    let text = resp.text() catch_all (e) {
        _ => {
            throw root.errors.NetworkFailure {
            provider: provider,
            detail: "the response body could not be read completely",
            raw_body: null,
        }
        },
    };
    if (!resp.ok()) {
        throw root.errors.classify_http(provider, resp.status_code, text)
    }
    baml.json.from_string<T>(text) catch_all (e) {
        _ => throw root.errors.ParseFailed { provider: provider, raw_output: text },
    }
}

// The return type's schema as prompt text, in the standard dialect —
// exactly what ${ctx.output_format} renders in an LLM function block.
function render_output_format(t: type) -> string {
    baml.prompt.render_output_format(t)
}

/// Provider-neutral media payload after resolving local files and, when
/// requested, ordinary HTTP(S) URLs. Google Cloud `gs://` URLs remain URLs.
class ResolvedMedia {
    url: string?,
    base64: string?,
    mime_type: string,
}

function _media_mime_type(explicit: string?, source_value: string, kind: string) -> string {
    if let supplied: string = explicit {
        return supplied;
    }
    let source = source_value.to_lower_case();
    if (source.includes(".jpg") || source.includes(".jpeg")) {
        "image/jpeg"
    } else if (source.includes(".png")) {
        "image/png"
    } else if (source.includes(".webp")) {
        "image/webp"
    } else if (source.includes(".gif")) {
        "image/gif"
    } else if (source.includes(".svg")) {
        "image/svg+xml"
    } else if (source.includes(".wav")) {
        "audio/wav"
    } else if (source.includes(".mp3")) {
        "audio/mpeg"
    } else if (source.includes(".flac")) {
        "audio/flac"
    } else if (source.includes(".ogg")) {
        "audio/ogg"
    } else if (source.includes(".webm")) {
        if (kind == "audio") {
            "audio/webm"
        } else {
            "video/webm"
        }
    } else if (source.includes(".mov")) {
        "video/quicktime"
    } else if (source.includes(".mp4")) {
        "video/mp4"
    } else if (source.includes(".avi")) {
        "video/x-msvideo"
    } else if (source.includes(".mkv")) {
        "video/x-matroska"
    } else if (source.includes(".pdf")) {
        "application/pdf"
    } else if (kind == "audio") {
        "audio/mpeg"
    } else if (kind == "video") {
        "video/mp4"
    } else if (kind == "pdf") {
        "application/pdf"
    } else {
        "image/png"
    }
}

function _resolve_media_content(
    url: string?,
    file: string?,
    inline: string,
    mime: string,
    fetch_url: bool,
) -> ResolvedMedia {
    if let path: string = file {
        let handle = baml.fs.open(path, "r");
        let data = handle.bytes().to_base64();
        handle.close();
        return ResolvedMedia { url: null, base64: data, mime_type: mime };
    }
    if (inline != "") {
        return ResolvedMedia { url: null, base64: inline, mime_type: mime };
    }
    if let media_url: string = url {
        if (media_url.starts_with("data:")) {
            if let separator: int = media_url.index_of(";base64,") {
                return ResolvedMedia {
                    url: null,
                    base64: media_url.substring(separator + 8, media_url.length()),
                    mime_type: media_url.substring(5, separator),
                };
            }
        }
        if (!fetch_url || media_url.starts_with("gs://")) {
            return ResolvedMedia { url: media_url, base64: null, mime_type: mime };
        }
        let response = baml.http.fetch(media_url);
        if (!response.ok()) {
            throw root.errors.NetworkFailure {
                provider: "media",
                detail: `media URL ${media_url} returned HTTP ${response.status_code}`,
                raw_body: null,
            }
        }
        let response_mime = response.headers.get("content-type") ?? mime;
        return ResolvedMedia {
            url: null,
            base64: response.bytes().to_base64(),
            mime_type: response_mime.split(";").at(0) ?? mime,
        };
    }
    throw baml.errors.InvalidArgument { message: "media has no URL, file, or base64 content" }
}

/// Resolve BAML media for provider request lowering. This mirrors the media
/// resolution pass in `sys_llm`: local files become base64, existing base64 is
/// retained, and URL fetching is selected by the provider.
function resolve_media(media: ai.MediaPart, fetch_url: bool) -> ResolvedMedia {
    match (media) {
        let image: baml.media.Image => {
            let source = image.file() ?? image.url() ?? "";
            _resolve_media_content(
                image.url(),
                image.file(),
                image.base64(),
                _media_mime_type(image.mime_type(), source, "image"),
                fetch_url,
            )
        },
        let audio: baml.media.Audio => {
            let source = audio.file() ?? audio.url() ?? "";
            _resolve_media_content(
                audio.url(),
                audio.file(),
                audio.base64(),
                _media_mime_type(audio.mime_type(), source, "audio"),
                fetch_url,
            )
        },
        let video: baml.media.Video => {
            let source = video.file() ?? video.url() ?? "";
            _resolve_media_content(
                video.url(),
                video.file(),
                video.base64(),
                _media_mime_type(video.mime_type(), source, "video"),
                fetch_url,
            )
        },
        let pdf: baml.media.Pdf => {
            let source = pdf.file() ?? pdf.url() ?? "";
            _resolve_media_content(
                pdf.url(),
                pdf.file(),
                pdf.base64(),
                _media_mime_type(pdf.mime_type(), source, "pdf"),
                fetch_url,
            )
        },
    }
}
