//! OpenAI-format prompt body builders.
//!
//! Supports both the Chat Completions API and the Responses API.

use baml_builtins2::MediaContent;

pub(crate) mod chat_completions;
pub(crate) mod images;
pub(crate) mod responses;

fn content_to_url_or_data_url(
    content: &MediaContent,
    mime: &str,
) -> Result<String, crate::build_request::BuildRequestError> {
    if let Some(url) = content.url() {
        return Ok(url.to_string());
    }
    if let Some(b64) = content.base64_data() {
        return Ok(format!("data:{mime};base64,{b64}"));
    }
    Err(crate::build_request::BuildRequestError::FileNotResolved(
        content.file_path().unwrap_or("<unknown>").to_string(),
    ))
}

fn content_to_base64(
    content: &MediaContent,
) -> Result<String, crate::build_request::BuildRequestError> {
    if let Some(b64) = content.base64_data() {
        return Ok(b64.to_string());
    }
    if let Some(url) = content.url() {
        return Err(crate::build_request::BuildRequestError::UnsupportedMedia(
            format!("audio URL not pre-fetched: {url}"),
        ));
    }
    Err(crate::build_request::BuildRequestError::FileNotResolved(
        content.file_path().unwrap_or("<unknown>").to_string(),
    ))
}
