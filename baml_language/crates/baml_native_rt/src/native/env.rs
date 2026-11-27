//! `baml.env`.

use super::{NativeBamlEnv, Natives, Root, Scope, Str, Thrown};

impl NativeBamlEnv for Natives {
    async fn get<'s>(
        scope: &mut Scope<'s>,
        key: Root<'s, Str>,
    ) -> Result<Option<Root<'s, Str>>, Thrown<'s>> {
        let name = key.get(scope).as_str().to_owned();
        let asked = name.clone();
        let read = scope
            .io(async move { std::env::var_os(asked).map(std::ffi::OsString::into_string) })
            .await;
        match read {
            None => Ok(None),
            Some(Ok(value)) => Ok(Some(scope.alloc_str(&value))),
            Some(Err(_)) => {
                Err(scope.parse_error(&format!("Environment variable '{name}' is not valid UTF-8")))
            }
        }
    }
}
