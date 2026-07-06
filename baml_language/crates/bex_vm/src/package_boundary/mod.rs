pub(crate) mod id;

use crate::package_baml::NativeFunction;

pub fn get_native_fn(path: &str) -> Option<NativeFunction> {
    match path.strip_prefix("boundary.")? {
        "id" => Some(id::new),
        "id.current" => Some(id::current),
        "LocalId.capture" => Some(id::capture),
        _ => None,
    }
}
