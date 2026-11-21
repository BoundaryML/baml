//! `baml.path`: paths as the host writes them. Nothing here reads a file.

use std::path::{Path, PathBuf};

use super::{Arr, NativeBamlPath, Natives, Root, Scope, Str, StrRef, Thrown};

impl NativeBamlPath for Natives {
    /// The parts as one path: a part that is absolute replaces what came
    /// before it.
    fn join<'s>(scope: &mut Scope<'s>, parts: Root<'s, Arr<Str>>) -> Root<'s, Str> {
        let joined: PathBuf = parts.get(scope).iter().map(StrRef::as_str).collect();
        // Made of strings, so it is one.
        let joined = joined.to_string_lossy().into_owned();
        scope.alloc_str(&joined)
    }

    fn parent<'s>(scope: &mut Scope<'s>, path: Root<'s, Str>) -> Option<Root<'s, Str>> {
        let parent = Path::new(path.get(scope).as_str())
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())?;
        Some(scope.alloc_str(&parent))
    }

    fn is_absolute(path: StrRef<'_>) -> bool {
        Path::new(path.as_str()).is_absolute()
    }

    /// `path` from the root: under `base`, or the directory the program
    /// runs in, with a leading `~` read as the user's home. A `..` is kept:
    /// taking it out would change what a path through a symbolic link
    /// names.
    fn _absolute<'s>(
        scope: &mut Scope<'s>,
        path: Root<'s, Str>,
        base: Option<Root<'s, Str>>,
    ) -> Result<Root<'s, Str>, Thrown<'s>> {
        let base = match base {
            Some(base) => Ok(PathBuf::from(base.get(scope).as_str())),
            None => std::env::current_dir(),
        };
        let mut path = PathBuf::from(path.get(scope).as_str());
        if (path == Path::new("~") || path.starts_with("~/"))
            && let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
        {
            path = PathBuf::from(home).join(path.strip_prefix("~").unwrap_or(Path::new("")));
        }
        let absolute = base.and_then(|base| std::path::absolute(base.join(path)));
        match absolute.map(|absolute| absolute.to_str().map(str::to_owned)) {
            Ok(Some(absolute)) => Ok(scope.alloc_str(&absolute)),
            Ok(None) => Err(scope.io_error("Path is not valid UTF-8")),
            Err(error) => Err(scope.io_error(&error.to_string())),
        }
    }
}
