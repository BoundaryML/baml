mod class;
mod client;
mod r#enum;
mod expression;
mod field_type;
mod function;
mod r#impl;
mod intermediate_repr;
mod template;
mod ts_language_features;

use crate::configuration::Generator;

use super::{
    dir_writer::WithFileContent,
    ir::{IntermediateRepr, WithJsonSchema},
};
use ts_language_features::get_file_collector;

pub(crate) fn generate_ts(ir: &IntermediateRepr, gen: &Generator) -> std::io::Result<()> {
    let mut collector = get_file_collector();

    ir.walk_enums().for_each(|e| e.write(&mut collector));
    ir.walk_classes().for_each(|c| c.write(&mut collector));
    ir.walk_functions().for_each(|f| f.write(&mut collector));
    ir.walk_functions().for_each(|f| {
        f.walk_impls().for_each(|i| {
            i.write(&mut collector);
        })
    });
    ir.walk_clients().for_each(|c| c.write(&mut collector));

    let file = collector.start_file("./", "json_schema", false);
    file.add_import("json-schema", "JSONSchema7", None, false);
    file.add_import(
        "@boundaryml/baml-client/deserializer/deserializer",
        "registerEnumDeserializer",
        None,
        false,
    );
    file.add_import(
        "@boundaryml/baml-client/deserializer/deserializer",
        "registerObjectDeserializer",
        None,
        false,
    );
    file.append(format!(
        "const schema: JSONSchema7 = {};",
        ir.json_schema().to_string()
    ));
    ir.walk_enums().for_each(|e| {
        file.append(format!(
            "registerEnumDeserializer(schema.definitions.{}, {{ }});",
            e.elem().name,
        ))
    });
    ir.walk_classes().for_each(|c| {
        file.append(format!(
            "registerObjectDeserializer(schema.definitions.{}, {{ }});",
            c.elem().name,
        ))
    });
    file.add_export("schema");
    collector.finish_file();
    ir.write(&mut collector);

    collector.commit(&gen.output_path)
}
