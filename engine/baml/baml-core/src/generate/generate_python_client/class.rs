use internal_baml_parser_database::walkers::Walker;
use internal_baml_schema_ast::ast::{ClassId, FieldId};

use super::{
    file::File,
    traits::{WithFile, WithPythonString},
};

impl WithFile for Walker<'_, ClassId> {
    fn file(&self) -> File {
        File::new("types", self.name())
    }
}

impl WithPythonString for Walker<'_, ClassId> {
    fn python_string(&self, file: &mut File) {
        file.add_import("pydantic", "BaseModel");

        file.add_line(format!("class {}(BaseModel):", self.name()));
        for field in self.static_fields() {
            field.python_string(file);
        }
    }
}
