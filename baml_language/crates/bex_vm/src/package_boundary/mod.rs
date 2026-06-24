mod id;

use crate::package_baml::NativeFunction;

pub fn get_native_fn(path: &str) -> Option<NativeFunction> {
    match path.strip_prefix("boundary.")? {
        "id.current" => Some(id::current),
        _ => None,
    }
}
