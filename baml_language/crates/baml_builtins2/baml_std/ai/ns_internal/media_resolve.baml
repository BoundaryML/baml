// Internal helpers behind `ai.wire.resolve_media`: the url/base64 resolution
// itself. MIME-type inference is `baml.media.<Kind>.infer_mime_type`. Nothing
// here is part of the public ai surface.

function _resolve_media_content(
    url: string?,
    inline: string,
    mime: string,
    fetch_url: bool,
) -> root.wire.ResolvedMedia {
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
        // A remote fetch gets a bounded deadline (the default is unbounded).
        // Preserve Timeout for the invocation boundary; other transport errors
        // become a typed, classified NetworkFailure.
        let response = baml.http.fetch(media_url, timeout = baml.time.Duration.from_seconds(30)) catch_all (e) {
            let timeout: baml.errors.Timeout => throw timeout,
            _ => {
                throw root.errors.NetworkFailure { status_code: null,
                    provider: "media",
                    detail: `media URL ${media_url} could not be fetched: ${e.to_string()}`,
                    raw_body: null,
                }
            },
        };
        if (!response.ok()) {
            throw root.errors.NetworkFailure { status_code: null,
                provider: "media",
                detail: `media URL ${media_url} returned HTTP ${response.status_code}`,
                raw_body: null,
            }
        }
        let response_mime = response.headers.get("content-type") ?? mime;
        let body = response.bytes() catch_all (e) {
            _ => {
                throw root.errors.NetworkFailure { status_code: null,
                    provider: "media",
                    detail: `media URL ${media_url} body could not be read: ${e.to_string()}`,
                    raw_body: null,
                }
            },
        };
        return root.wire.ResolvedMedia {
            url: null,
            base64: body.to_base64(),
            // HTTP allows whitespace around the `;` parameter separator, so the
            // subtype must be trimmed or providers reject the declared type.
            mime_type: (response_mime.split(";").at(0) ?? mime).trim(),
        };
    }
    throw baml.errors.InvalidArgument { message: "media has no URL or base64 content" }
}

/// (internal) Preview-only counterpart to `_resolve_media_content`. Ordinary URLs
/// remain references rather than being passed to `baml.http.fetch`.
function _resolve_media_preview_content(
    url: string?,
    inline: string,
    mime: string,
) -> root.wire.ResolvedMedia {
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
            throw ai.errors.PreviewUnsupported {
                provider: "media-preview",
                detail: "request preview cannot represent a non-base64 data URL",
            }
        }
        return root.wire.ResolvedMedia { url: media_url, base64: null, mime_type: mime };
    }
    throw ai.errors.PreviewUnsupported {
        provider: "media-preview",
        detail: "request preview received media without inline data or a URL",
    }
}
