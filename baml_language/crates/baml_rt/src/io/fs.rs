//! Path operations use owned strings on Tokio, with the native host's error
//! messages and process-working-directory semantics. No file handle escapes.
use super::{Failure, Output, Owned};
use std::path::Path;

pub fn exists(path: String) -> Output {
    Output::Async(Box::pin(async move {
        tokio::fs::try_exists(&path)
            .await
            .map(Owned::from)
            .map_err(|e| Failure::io(format!("Failed to check existence of '{path}': {e}")))
    }))
}
pub fn read(path: String) -> Output {
    Output::Async(Box::pin(async move {
        tokio::fs::read_to_string(&path)
            .await
            .map(Owned::from)
            .map_err(|e| Failure::io(format!("Failed to read file '{path}': {e}")))
    }))
}
pub fn write(path: String, content: String) -> Output {
    Output::Async(Box::pin(async move {
        if let Some(parent) = Path::new(&path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
        {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                Failure::io(format!(
                    "Failed to create parent directories for '{path}': {e}"
                ))
            })?;
        }
        tokio::fs::write(&path, content.as_bytes())
            .await
            .map_err(|e| Failure::io(format!("Failed to write file '{path}': {e}")))?;
        i64::try_from(content.len())
            .map(Owned::from)
            .map_err(|_| Failure::io(format!("Write size {} exceeds i64::MAX", content.len())))
    }))
}
pub fn size(path: String) -> Output {
    Output::Async(Box::pin(async move {
        let meta = tokio::fs::metadata(&path)
            .await
            .map_err(|e| Failure::io(format!("Failed to stat '{path}': {e}")))?;
        i64::try_from(meta.len())
            .map(Owned::from)
            .map_err(|_| Failure::io(format!("File '{path}' size exceeds i64::MAX")))
    }))
}
pub fn remove(path: String) -> Output {
    Output::Async(Box::pin(async move {
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(Owned::Null),
            Err(e) => {
                if tokio::fs::symlink_metadata(&path)
                    .await
                    .is_ok_and(|m| m.is_dir())
                {
                    return Err(Failure::io(format!(
                        "Failed to remove '{path}': it is a directory; use baml.fs.remove_dir or baml.fs.remove_dir_all to delete directories"
                    )));
                }
                Err(Failure::io(format!("Failed to remove file '{path}': {e}")))
            }
        }
    }))
}
pub fn remove_dir(path: String) -> Output {
    Output::Async(Box::pin(async move {
        tokio::fs::remove_dir(&path)
            .await
            .map(|()| Owned::Null)
            .map_err(|e| Failure::io(format!("Failed to remove directory '{path}': {e}")))
    }))
}
pub fn remove_dir_all(path: String) -> Output {
    Output::Async(Box::pin(async move {
        match tokio::fs::remove_dir_all(&path).await {
            Ok(()) => Ok(Owned::Null),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Owned::Null),
            Err(e) => Err(Failure::io(format!(
                "Failed to remove directory '{path}': {e}"
            ))),
        }
    }))
}
pub fn rename(source: String, destination: String) -> Output {
    Output::Async(Box::pin(async move {
        tokio::fs::rename(source, destination)
            .await
            .map(|()| Owned::Null)
            .map_err(|e| Failure::io(e.to_string()))
    }))
}
pub fn canonicalize(path: String) -> Output {
    Output::Async(Box::pin(async move {
        let path = tokio::fs::canonicalize(path)
            .await
            .map_err(|e| Failure::io(e.to_string()))?;
        super::path_string(path)
    }))
}
