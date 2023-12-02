use super::file::{File, FileCollector};

#[derive(Debug, Clone, Copy)]
pub(crate) enum TargetLanguage {
    Python,
    TypeScript,
}

impl TargetLanguage {
    pub(super) fn as_str(&self) -> &'static str {
        match self {
            TargetLanguage::Python => "python",
            TargetLanguage::TypeScript => "ts",
        }
    }
}

pub(super) trait WithToObject {
    fn to_object(&self, f: &mut File, lang: TargetLanguage) -> String {
        match lang {
            TargetLanguage::Python => self.to_py_object(f),
            TargetLanguage::TypeScript => self.to_ts_object(f),
        }
    }
    fn to_py_object(&self, f: &mut File) -> String;
    fn to_ts_object(&self, f: &mut File) -> String;
}

pub(super) trait JsonHelper {
    fn json(&self, f: &mut File, lang: TargetLanguage) -> serde_json::Value;
}

pub(crate) trait WithFileName {
    fn file_name(&self) -> String;

    fn to_file(&self, fc: &mut FileCollector, lang: TargetLanguage) {
        match lang {
            TargetLanguage::Python => self.to_py_file(fc),
            TargetLanguage::TypeScript => self.to_ts_file(fc),
        }
    }

    fn to_py_file(&self, fc: &mut FileCollector);
    fn to_ts_file(&self, fc: &mut FileCollector);
}

pub(crate) trait WithToCode {
    fn to_code(&self, f: &mut File, lang: TargetLanguage) -> String {
        match lang {
            TargetLanguage::Python => self.to_py_string(f),
            TargetLanguage::TypeScript => self.to_ts_string(f),
        }
    }

    fn to_py_string(&self, f: &mut File) -> String;
    fn to_ts_string(&self, f: &mut File) -> String;
}
