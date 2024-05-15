use std::path::PathBuf;

use self::traits::WithWritePythonString;
use crate::{configuration::Generator, lockfile::LockFileWrapper};
use internal_baml_parser_database::ParserDatabase;
use internal_baml_schema_ast::ast::WithName;
use log::info;
use serde_json::json;
use std::io::{self, ErrorKind};
use std::thread::sleep;
use std::time::Duration;

mod r#class;
mod client;
mod configuration;
mod r#enum;
mod field;
mod r#file;
mod function;
mod template;
mod traits;
mod types;
mod types_partial;
mod value;
mod variants;
pub(super) use r#file::FileCollector;


fn generate_py_file<'a>(obj: &impl WithWritePythonString, fc: &'a mut FileCollector) {
    obj.write_py_file(fc);
}

pub(crate) fn generate_py(
    db: &ParserDatabase,
    gen: &Generator,
    lock: &LockFileWrapper,
) -> std::io::Result<()> {
    let mut fc = Default::default();
    db.walk_enums().for_each(|e| generate_py_file(&e, &mut fc));
    db.walk_classes()
        .for_each(|c| generate_py_file(&c, &mut fc));
    db.walk_old_functions()
        .chain(db.walk_new_functions())
        .for_each(|f| generate_py_file(&f, &mut fc));
    let mut variants = db.walk_variants().collect::<Vec<_>>();
    variants.sort_by(|a, b| {
        match a
            .function_identifier()
            .name()
            .cmp(b.function_identifier().name())
        {
            std::cmp::Ordering::Equal => a.name().cmp(b.name()),
            x => x,
        }
    });
    variants.iter().for_each(|f| generate_py_file(f, &mut fc));

    db.walk_clients()
        .for_each(|f| generate_py_file(&f, &mut fc));
    db.walk_retry_policies()
        .for_each(|f| generate_py_file(&f, &mut fc));
    let mut test_cases = db.walk_test_cases().collect::<Vec<_>>();
    test_cases.sort_by(|a, b| a.name().cmp(b.name()));
    test_cases.iter().for_each(|f| generate_py_file(f, &mut fc));

    generate_py_file(db, &mut fc);

    let output_path = &gen.output_path;
    info!("Writing files to {}", output_path.to_string_lossy());

    let temp_path = PathBuf::from(format!("{}.tmp", output_path.to_string_lossy().to_string()));

    // if the .tmp dir exists, delete it so we can get back to a working state without user intervention.
    let delete_attempts = 3; // Number of attempts to delete the directory
    let attempt_interval = Duration::from_millis(200); // Wait time between attempts

    for attempt in 1..=delete_attempts {
        if temp_path.exists() {
            match std::fs::remove_dir_all(&temp_path) {
                Ok(_) => {
                    println!("Temp directory successfully removed.");
                    break; // Exit loop after successful deletion
                }
                Err(e) if e.kind() == ErrorKind::Other && attempt < delete_attempts => {
                    info!(
                        "Attempt {}: Failed to delete temp directory: {}",
                        attempt, e
                    );
                    sleep(attempt_interval); // Wait before retrying
                }
                Err(e) => {
                    // For other errors or if it's the last attempt, fail with an error
                    return Err(io::Error::new(
                        e.kind(),
                        format!("Failed to delete temp directory: {}", e),
                    ));
                }
            }
        } else {
            break;
        }
    }

    if temp_path.exists() {
        // If the directory still exists after the loop, return an error
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to delete existing temp directory within the timeout",
        ));
    }

    match fc.write(&temp_path, gen, lock) {
        Ok(_) => {
            let _ = std::fs::remove_dir_all(output_path);
            std::fs::rename(&temp_path, output_path)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&temp_path);
            Err(e)
        }
    }
}

impl WithWritePythonString for ParserDatabase {
    fn write_py_file<'a>(&'a self, fc: &'a mut FileCollector) {
        fc.start_py_file(".", "__init__");
        fc.last_file().add_line("from . import impls");
        fc.complete_file();

        // Add final aliased imports so users just need to import from baml_client and not baml_core, baml_lib and baml_test
        fc.start_export_file("./testing", "__init__");
        fc.last_file()
            .add_import_and_reexport("pytest_baml", "baml_test");

        fc.complete_file();

        // tracing imports
        fc.start_export_file("./tracing", "__init__");
        fc.last_file()
            .add_import_and_reexport("baml_core.otel", "trace");
        fc.last_file()
            .add_import_and_reexport("baml_core.otel", "set_tags");
        fc.complete_file();

        fc.start_export_file("./baml_types", "__init__");

        self.walk_old_functions()
            .chain(self.walk_new_functions())
            .for_each(|f| {
                fc.last_file().add_import(
                    &format!("..__do_not_import.functions.{}", f.file_name()),
                    &format!("I{}", f.name()),
                );
                fc.last_file().add_import(
                    &format!("..__do_not_import.functions.{}", f.file_name()),
                    &format!("I{}Stream", f.name()),
                );
                fc.last_file().add_import(
                    &format!("..__do_not_import.functions.{}", f.file_name()),
                    &format!("I{}Output", f.name()),
                )
            });
        self.walk_enums().for_each(|e| {
            fc.last_file().add_import(
                &format!("..__do_not_import.types.enums.{}", e.file_name()),
                e.name(),
            )
        });

        self.walk_classes().for_each(|c| {
            fc.last_file().add_import(
                &format!("..__do_not_import.types.classes.{}", c.file_name()),
                c.name(),
            )
        });
        fc.complete_file();

        // Add the partial type exports for streaming and handling incomplete data.
        fc.start_export_file("./baml_types", "partial");
        self.walk_classes().for_each(|c| {
            fc.last_file().add_import_and_reexport(
                &format!("..__do_not_import.types.partial.classes.{}", c.file_name()),
                &format!("Partial{}", c.name()),
            )
        });
        fc.complete_file();

        // manually write each import line since we want imports to appear in a certain order to load dotenv vars (which happens when importing baml_init)
        fc.start_export_file(".", "__init__.py");
        fc.last_file().add_line("from baml_lib import baml_init");
        fc.last_file()
            .add_line("from .__do_not_import.generated_baml_client import baml");
        fc.last_file().add_line("__all__ = ['baml', 'baml_init']");
        fc.complete_file();

        fc.start_py_file(".", "generated_baml_client");
        let mut fxs = self
            .walk_old_functions()
            .chain(self.walk_new_functions())
            .map(|f| {
                fc.last_file().add_import(
                    &format!(".functions.{}", f.file_name()),
                    &format!("BAML{}", f.name()),
                );
                f.name()
            })
            .collect::<Vec<_>>();
        fxs.sort();

        let mut clients = self
            .walk_clients()
            .map(|f| {
                fc.last_file().add_import(
                    &format!(".clients.{}", f.file_name()),
                    &format!("{}", f.name()),
                );
                f.name().to_string()
            })
            .collect::<Vec<_>>();
        clients.sort();

        template::render_template(
            template::HSTemplate::BAMLClient,
            fc.last_file(),
            json!({ "functions": fxs, "clients": clients }),
        );
        fc.complete_file();
    }

    fn file_name(&self) -> String {
        "generated_baml_client".to_string()
    }
}
