use internal_baml_parser_database::walkers::{EnumWalker, FunctionWalker, VariantWalker};
use internal_baml_schema_ast::ast::{FunctionId, TopId, WithName};

use serde_json::json;

use crate::generate::generate_python_client::{file::clean_file_name, traits::SerializerHelper};

use super::{
    file::File,
    template::render_template,
    traits::{JsonHelper, WithWritePythonString},
    FileCollector,
};

impl<'db> JsonHelper for VariantWalker<'db> {
    fn json(&self, f: &mut File) -> serde_json::Value {
        let func = self.walk_function().unwrap();
        let client = self.client().unwrap();
        f.add_import(
            &format!(".{}", func.file_name()),
            &format!("BAML{}", func.name()),
        );
        f.add_import(&format!("..clients.{}", client.file_name()), client.name());

        let mut prompt = self.properties().prompt.0.clone();
        self.properties().replacers.0.iter().for_each(|(k, val)| {
            prompt = prompt.replace(&k.key(), &format!("{{{}}}", val));
        });

        json!({
            "name": self.identifier().name(),
            "function": func.json(f),
            "prompt": prompt,
            "client": client.name(),
            "output_replacers": (self.properties().replacers.1.iter().map(|x| json!({
                "key": x.0.key(),
                "value": match x.1 {
                    TopId::Function(id) => {
                        let func: FunctionWalker<'db> = self.db.walk(*id);
                        func.walk_output_args().into_iter().map(|x| x.serialize(f)).next().unwrap()
                    },
                    TopId::Enum(id) => {
                        let enm = self.db.walk(*id);
                        enm.serialize(f)
                    },
                    TopId::Class(id) => {
                        let cls = self.db.walk(*id);
                        cls.serialize(f)
                    }
                    _ => unreachable!("Only functions are allowed in output replacers"),
                },
            })).collect::<Vec<_>>()),
        })
    }
}

impl WithWritePythonString for VariantWalker<'_> {
    fn file_name(&self) -> String {
        format!(
            "fx_{}_impl_{}",
            clean_file_name(self.function_identifier().name()),
            clean_file_name(self.identifier().name())
        )
    }

    fn write_py_file(&self, fc: &mut FileCollector) {
        fc.start_py_file("functions", self.file_name());
        let json = self.json(fc.last_file());
        render_template(super::template::HSTemplate::Variant, fc.last_file(), json);
        fc.complete_file();
    }
}
