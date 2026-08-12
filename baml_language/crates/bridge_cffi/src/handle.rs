//! Safe, target-neutral ordinary handle and media operations.

use std::sync::Arc;

use bex_project::{BexExternalAdt, MediaKind, MediaValue};
use bridge_ctypes::{CffiHandleTableEntry, HANDLE_TABLE, baml_bridge::cffi::BamlHandleType};

/// An owned handle-table key and its protocol type tag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HandleParts {
    pub key: u64,
    pub handle_type: i32,
}

/// Failure from a safe ordinary handle or media operation.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum HandleError {
    #[error("invalid handle")]
    InvalidHandle,
    #[error("handle type mismatch")]
    TypeMismatch,
    #[error("unsupported handle type")]
    UnsupportedHandleType,
    #[error("{0}")]
    InvalidInput(String),
}

fn insert_entry(entry: CffiHandleTableEntry) -> HandleParts {
    let handle_type = entry.handle_type() as i32;
    let key = HANDLE_TABLE.insert(entry);
    HandleParts { key, handle_type }
}

fn validate_input(value: &str, field: &str) -> Result<(), HandleError> {
    if value.contains('\0') {
        return Err(HandleError::InvalidInput(format!(
            "{field} contains an embedded NUL byte"
        )));
    }
    Ok(())
}

fn validate_media_input(source: &str, mime_type: Option<&str>) -> Result<(), HandleError> {
    validate_input(source, "media source")?;
    if let Some(mime_type) = mime_type {
        validate_input(mime_type, "media MIME type")?;
    }
    Ok(())
}

fn resolve_media(key: u64, handle_type: i32) -> Result<Arc<MediaValue>, HandleError> {
    let entry = HANDLE_TABLE
        .resolve(key)
        .ok_or(HandleError::InvalidHandle)?;

    if handle_type != BamlHandleType::HandleUnspecified as i32
        && handle_type != entry.handle_type() as i32
    {
        return Err(HandleError::TypeMismatch);
    }

    match &*entry {
        CffiHandleTableEntry::Adt(BexExternalAdt::Media(media)) => Ok(media.clone()),
        _ => Err(HandleError::UnsupportedHandleType),
    }
}

/// Clone one live ordinary handle-table row into a distinct owned key.
pub fn clone_handle(key: u64) -> Result<u64, HandleError> {
    HANDLE_TABLE
        .clone_handle(key)
        .ok_or(HandleError::InvalidHandle)
}

/// Release one owned ordinary handle-table key.
pub fn release_handle(key: u64) -> Result<(), HandleError> {
    if HANDLE_TABLE.release(key) {
        Ok(())
    } else {
        Err(HandleError::InvalidHandle)
    }
}

/// Return the number of currently owned ordinary handle-table keys.
///
/// This is exposed by target adapters only as focused test instrumentation;
/// it is not part of the generated SDK surface.
pub fn live_handle_count() -> usize {
    HANDLE_TABLE.len()
}

/// Seed a function-reference handle for focused bridge tests.
pub fn seed_function_ref_handle(global_index: u64) -> HandleParts {
    insert_entry(CffiHandleTableEntry::FunctionRef {
        global_index: global_index as usize,
    })
}

/// Seed a generic-media handle for focused bridge tests.
pub fn seed_generic_media_handle() -> HandleParts {
    insert_entry(CffiHandleTableEntry::Adt(BexExternalAdt::Media(
        MediaValue::from_url(MediaKind::Generic, "https://example.com/", None),
    )))
}

/// Construct an owned media handle from a URL descriptor.
pub fn media_from_url(
    kind: MediaKind,
    url: &str,
    mime_type: Option<&str>,
) -> Result<HandleParts, HandleError> {
    validate_media_input(url, mime_type)?;
    Ok(insert_entry(CffiHandleTableEntry::Adt(
        BexExternalAdt::Media(MediaValue::from_url(kind, url, mime_type)),
    )))
}

/// Construct an owned media handle from a file-path descriptor.
pub fn media_from_file(
    kind: MediaKind,
    file: &str,
    mime_type: Option<&str>,
) -> Result<HandleParts, HandleError> {
    validate_media_input(file, mime_type)?;
    Ok(insert_entry(CffiHandleTableEntry::Adt(
        BexExternalAdt::Media(MediaValue::from_file(kind, file, mime_type)),
    )))
}

/// Construct an owned media handle from base64 text.
pub fn media_from_base64(
    kind: MediaKind,
    base64: &str,
    mime_type: Option<&str>,
) -> Result<HandleParts, HandleError> {
    validate_media_input(base64, mime_type)?;
    Ok(insert_entry(CffiHandleTableEntry::Adt(
        BexExternalAdt::Media(MediaValue::from_base64(kind, base64, mime_type)),
    )))
}

pub fn media_url(key: u64, handle_type: i32) -> Result<Option<String>, HandleError> {
    Ok(resolve_media(key, handle_type)?.url())
}

pub fn media_file(key: u64, handle_type: i32) -> Result<Option<String>, HandleError> {
    Ok(resolve_media(key, handle_type)?.file())
}

pub fn media_base64(key: u64, handle_type: i32) -> Result<String, HandleError> {
    Ok(resolve_media(key, handle_type)?.base64())
}

pub fn media_mime_type(key: u64, handle_type: i32) -> Result<Option<String>, HandleError> {
    Ok(resolve_media(key, handle_type)?.mime_type())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_media_source_and_mime_type() {
        assert_eq!(validate_media_input("payload", Some("image/png")), Ok(()));
        assert_eq!(
            validate_media_input("bad\0payload", None),
            Err(HandleError::InvalidInput(
                "media source contains an embedded NUL byte".to_string()
            ))
        );
        assert_eq!(
            validate_media_input("payload", Some("bad\0mime")),
            Err(HandleError::InvalidInput(
                "media MIME type contains an embedded NUL byte".to_string()
            ))
        );
    }

    #[test]
    fn resolve_media_rejects_type_mismatch() {
        let parts = media_from_url(MediaKind::Image, "https://example.com/image.png", None)
            .expect("media handle should be created");

        assert_eq!(
            resolve_media(parts.key, BamlHandleType::AdtMediaAudio as i32),
            Err(HandleError::TypeMismatch)
        );
        release_handle(parts.key).expect("media handle should be released");
    }

    #[test]
    fn resolve_media_rejects_unsupported_handle_type() {
        let parts = seed_function_ref_handle(7);

        assert_eq!(
            resolve_media(parts.key, parts.handle_type),
            Err(HandleError::UnsupportedHandleType)
        );
        release_handle(parts.key).expect("function handle should be released");
    }

    #[test]
    fn clone_and_release_reject_invalid_handles() {
        assert_eq!(clone_handle(u64::MAX), Err(HandleError::InvalidHandle));
        assert_eq!(release_handle(u64::MAX), Err(HandleError::InvalidHandle));
    }
}
