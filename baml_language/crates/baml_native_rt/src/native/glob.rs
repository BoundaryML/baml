//! `baml.glob`: a `Glob` owns its compiled pattern as Rust state. The
//! pattern language is `sys_glob`'s, which the interpreter matches with
//! too. Scanning a directory is not here: it reads the file system.

use std::sync::Arc;

use sys_glob::GlobPattern;

use super::{
    NativeBamlGlob, NativeBamlGlobGlob, Natives, Obj, ObjRef, Opaque, Root, Scope, Str, StrRef,
    Thrown,
};
use crate::layout::NAMED_GLOB;

impl NativeBamlGlob for Natives {
    fn new<'s>(scope: &mut Scope<'s>, pattern: Root<'s, Str>) -> Result<Root<'s, Obj>, Thrown<'s>> {
        match GlobPattern::new(pattern.get(scope).as_str()) {
            Ok(compiled) => {
                let compiled = scope.alloc_owned(Arc::new(compiled));
                let class = scope.named_class(NAMED_GLOB);
                let mut glob = scope.build(class);
                glob.set(scope, 0, compiled);
                Ok(glob.finish())
            }
            Err(message) => Err(scope.parse_error(&message)),
        }
    }
}

impl NativeBamlGlobGlob for Natives {
    fn matches(glob: ObjRef<'_>, path: StrRef<'_>) -> bool {
        // SAFETY: a `Glob` has one field, its pattern.
        unsafe { glob.field::<Opaque>(0) }
            .owned::<GlobPattern>()
            .is_match(path.as_str())
    }
}
