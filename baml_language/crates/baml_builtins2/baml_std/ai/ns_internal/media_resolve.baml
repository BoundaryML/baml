// Internal helpers behind `ai.wire.resolve_media`: MIME-type inference and the
// url/file/base64 resolution itself. Nothing here is part of the public ai
// surface.

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
) -> root.wire.ResolvedMedia {
    if let path: string = file {
        // `open` and `bytes` both throw. The handle must close on the failure
        // path too — media resolution runs on every prompt carrying a local
        // file, so a repeated failure would leak a handle per request — and the
        // error is worth naming the path in.
        let handle = baml.fs.open(path, "r") catch_all (e) {
            _ => {
                throw baml.errors.InvalidArgument {
                    message: `media file ${path} could not be opened: ${e.to_string()}`,
                }
            },
        };
        let data = handle.bytes().to_base64() catch_all (e) {
            _ => {
                handle.close();
                throw baml.errors.InvalidArgument {
                    message: `media file ${path} could not be read: ${e.to_string()}`,
                }
            },
        };
        handle.close();
        return root.wire.ResolvedMedia { url: null, base64: data, mime_type: mime };
    }
    if (inline != "") {
        return root.wire.ResolvedMedia { url: null, base64: inline, mime_type: mime };
    }
    if let media_url: string = url {
        if (media_url.starts_with("data:")) {
            if let separator: int = media_url.index_of(";base64,") {
                return root.wire.ResolvedMedia {
                    url: null,
                    base64: media_url.slice(separator + 8, media_url.length()),
                    mime_type: media_url.slice(5, separator),
                };
            }
            // A percent-encoded `data:` URL has no base64 payload to lift out.
            // Falling through would either hand a provider a URL it cannot
            // fetch or hand `baml.http.fetch` a scheme it does not speak, both
            // of which surface as something unrelated to the real problem.
            // (The Vercel images client rejects the same shape in
            // `_gw_images_value`.)
            let preview = if (media_url.length() > 48) {
                `${media_url.slice(0, 48)}...`
            } else {
                media_url
            };
            throw baml.errors.InvalidArgument {
                message: `media carries a data: URL with no ;base64, marker, which has no payload to send: ${preview}`,
            }
        }
        if (!fetch_url || media_url.starts_with("gs://")) {
            return root.wire.ResolvedMedia { url: media_url, base64: null, mime_type: mime };
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
        return root.wire.ResolvedMedia {
            url: null,
            base64: response.bytes().to_base64(),
            mime_type: response_mime.split(";").at(0) ?? mime,
        };
    }
    throw baml.errors.InvalidArgument { message: "media has no URL, file, or base64 content" }
}
