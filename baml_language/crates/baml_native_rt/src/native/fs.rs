//! `baml.fs`.

use std::path::PathBuf;

use super::{NativeBamlFs, Natives, Root, Scope, Str, Thrown};

impl NativeBamlFs for Natives {
    async fn read<'s>(
        scope: &mut Scope<'s>,
        path: Root<'s, Str>,
    ) -> Result<Root<'s, Str>, Thrown<'s>> {
        let path = PathBuf::from(path.get(scope).as_str());
        let asked = path.clone();
        let read = scope
            .io(async move { tokio::fs::read_to_string(asked).await })
            .await;
        match read {
            Ok(text) => Ok(scope.alloc_str(&text)),
            Err(error) => Err(scope.io_error(&format!(
                "Failed to read file '{}': {error}",
                path.display()
            ))),
        }
    }
}
