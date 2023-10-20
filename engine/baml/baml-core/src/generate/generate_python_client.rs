use internal_baml_parser_database::ParserDatabase;

use crate::configuration::Generator;

use self::{
    file::{File, FileCollector},
    traits::{WithFile, WithPythonString},
};

mod r#class;
mod r#enum;
mod field;
mod r#file;
mod traits;

fn generate_py_file<'a>(
    obj: impl WithFile + WithPythonString + Copy,
    fc: &'a mut FileCollector,
    gen: &Generator,
) {
    let file = fc.add_file(obj);
    obj.python_string(file);
}

pub(crate) fn generate_py(db: &ParserDatabase, gen: &Generator) {
    let mut fc = Default::default();
    db.walk_enums()
        .for_each(|e| generate_py_file(e, &mut fc, gen));
    db.walk_classes()
        .for_each(|c| generate_py_file(c, &mut fc, gen));

    fc.write(&gen.output);
}
