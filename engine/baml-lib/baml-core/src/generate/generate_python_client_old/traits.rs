use super::file::{File, FileCollector};

pub(super) trait WithWritePythonString {
    fn write_py_file<'a>(&'a self, fc: &'a mut FileCollector);

    fn file_name(&self) -> String;
}

pub(crate) trait WithToCode {
    fn to_py_string(&self, f: &mut File) -> String;
}

// A trait that allows us to specify a "partial" type that is optional
// used for reading streaming data, where not all data may be completed.
pub(crate) trait WithPartial {
    fn to_partial_py_string(&self, f: &mut File) -> String;
}

pub(super) trait JsonHelper {
    fn json(&self, f: &mut File) -> serde_json::Value;
}
