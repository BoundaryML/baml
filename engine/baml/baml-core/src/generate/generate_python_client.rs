use internal_baml_parser_database::ParserDatabase;

use crate::configuration::Generator;

use self::{
    file::{File, FileCollector},
    traits::WithWritePythonString,
};

mod r#class;
mod r#enum;
mod field;
mod r#file;
mod function;
mod template;
mod traits;
mod types;

fn generate_py_file<'a>(
    obj: impl WithWritePythonString,
    fc: &'a mut FileCollector,
    gen: &Generator,
) {
    obj.write_py_file(fc);
}

pub(crate) fn generate_py(db: &ParserDatabase, gen: &Generator) -> std::io::Result<()> {
    let mut fc = Default::default();
    db.walk_enums()
        .for_each(|e| generate_py_file(e, &mut fc, gen));
    db.walk_classes()
        .for_each(|c| generate_py_file(c, &mut fc, gen));
    db.walk_functions()
        .for_each(|f| generate_py_file(f, &mut fc, gen));
    fc.write(&gen.output)
}
