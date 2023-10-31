use internal_baml_parser_database::ParserDatabase;
use internal_baml_schema_ast::ast::WithName;
use log::info;
use serde_json::json;

use crate::configuration::Generator;

use self::{file::FileCollector, traits::WithWritePythonString};

mod r#class;
mod client;
mod r#enum;
mod field;
mod r#file;
mod function;
mod template;
mod traits;
mod types;
mod variants;

fn generate_py_file<'a>(obj: &impl WithWritePythonString, fc: &'a mut FileCollector) {
    obj.write_py_file(fc);
}

pub(crate) fn generate_py(db: &ParserDatabase, gen: &Generator) -> std::io::Result<()> {
    let mut fc = Default::default();
    db.walk_enums().for_each(|e| generate_py_file(&e, &mut fc));
    db.walk_classes()
        .for_each(|c| generate_py_file(&c, &mut fc));
    db.walk_functions().for_each(|f| {
        generate_py_file(&f, &mut fc);
        f.walk_variants()
            .for_each(|v| generate_py_file(&v, &mut fc));
    });
    db.walk_clients()
        .for_each(|f| generate_py_file(&f, &mut fc));
    generate_py_file(db, &mut fc);
    info!(
        "Writing files to {}",
        &gen.output.as_ref().unwrap().to_string()
    );
    fc.write(&gen.output)
}

impl WithWritePythonString for ParserDatabase {
    fn write_py_file<'a>(&'a self, fc: &'a mut FileCollector) {
        fc.start_py_file(".", "__init__");
        fc.last_file().add_import(".generated_baml_client", "baml");
        fc.last_file().add_import(".", "baml_types");
        fc.complete_file();

        fc.start_export_file(".", "baml_types");
        self.walk_functions().for_each(|f| {
            fc.last_file().add_import(
                &format!(".functions.{}", f.file_name()),
                &format!("I{}", f.name()),
            )
        });

        self.walk_enums().for_each(|e| {
            fc.last_file()
                .add_import(&format!(".types.enums.{}", e.file_name()), e.name())
        });

        self.walk_classes().for_each(|c| {
            fc.last_file()
                .add_import(&format!(".types.classes.{}", c.file_name()), c.name())
        });

        fc.complete_file();

        fc.start_py_file(".", "generated_baml_client");
        let fxs = self
            .walk_functions()
            .map(|f| {
                fc.last_file().add_import(
                    &format!(".functions.{}", f.file_name()),
                    &format!("BAML{}", f.name()),
                );
                f.name()
            })
            .collect::<Vec<_>>();
        template::render_template(
            template::HSTemplate::BAMLClient,
            fc.last_file(),
            json!({ "functions": fxs }),
        );
        fc.complete_file();
    }

    fn file_name(&self) -> String {
        "generated_baml_client".to_string()
    }
}
