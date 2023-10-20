use super::file::File;

pub(super) trait WithPythonString {
    fn python_string(&self, file: &mut File);
}

pub(super) trait WithFile {
    fn file(&self) -> File;
}
