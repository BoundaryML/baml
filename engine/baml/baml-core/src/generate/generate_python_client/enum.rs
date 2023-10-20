use internal_baml_parser_database::walkers::Walker;
use internal_baml_schema_ast::ast::{EnumId, EnumValueId};

use super::{
    file::File,
    traits::{WithFile, WithPythonString},
};

impl WithFile for Walker<'_, EnumId> {
    fn file(&self) -> File {
        File::new("types/enums", self.name())
    }
}

impl WithPythonString for Walker<'_, EnumId> {
    fn python_string(&self, file: &mut File) {
        file.add_import("enum", "Enum");
        file.add_line(format!("class {}(Enum, str):", self.name()));
        for val in self.values() {
            val.python_string(file);
        }
    }
}

impl WithPythonString for Walker<'_, (EnumId, EnumValueId)> {
    fn python_string(&self, file: &mut File) {
        file.add_indent_line(format!("{0} = '{0}'", self.name()), 1);
    }
}
