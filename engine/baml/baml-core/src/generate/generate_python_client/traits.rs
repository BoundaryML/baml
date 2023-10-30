use super::file::{File, FileCollector};

pub(super) trait WithWritePythonString {
    fn write_py_file<'a>(&'a self, fc: &'a mut FileCollector);

    fn file_name(&self) -> String;
}

pub(super) trait WithToCode {
    fn to_py_string(&self, f: &mut File) -> String;
}

pub(super) trait JsonHelper {
    fn json(&self, f: &mut File) -> serde_json::Value;
}

pub(super) trait SerializerHelper {
    fn serialize(&self, f: &mut File) -> serde_json::Value;
}
