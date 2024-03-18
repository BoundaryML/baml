mod class;
mod client;
mod r#enum;
mod expression;
mod field_type;
mod function;
mod r#impl;
mod intermediate_repr;
mod retry_policy;
mod template;
mod test_case;
mod ts_language_features;

use crate::configuration::Generator;

use super::{
    dir_writer::WithFileContent,
    ir::{Expression, IntermediateRepr, WithJsonSchema},
};
use ts_language_features::{get_file_collector, ToTypeScript};

pub(crate) fn generate_ts(ir: &IntermediateRepr, gen: &Generator) -> std::io::Result<()> {
    let mut collector = get_file_collector();

    ir.walk_enums().for_each(|e| e.write(&mut collector));
    ir.walk_classes().for_each(|c| c.write(&mut collector));
    ir.walk_functions().for_each(|f| f.write(&mut collector));
    ir.walk_functions().for_each(|f| {
        f.walk_impls().for_each(|i| {
            i.write(&mut collector);
        });
        f.walk_tests().for_each(|t| {
            t.write(&mut collector);
        });
    });

    {
        let clients = collector.start_file(".", "client", false);
        clients.append(
            r#"
    import { loadEnvVars } from '@boundaryml/baml-core';
    loadEnvVars();
            "#
            .to_string(),
        );
        collector.finish_file();
    }

    ir.walk_clients().for_each(|c| c.write(&mut collector));
    ir.walk_retry_policies()
        .for_each(|r| r.write(&mut collector));

    let file = collector.start_file("./", "json_schema", false);
    file.add_import("json-schema", "JSONSchema7", None, false);
    file.add_import(
        "@boundaryml/baml-core/deserializer/deserializer",
        "registerEnumDeserializer",
        None,
        false,
    );
    file.add_import(
        "@boundaryml/baml-core/deserializer/deserializer",
        "registerObjectDeserializer",
        None,
        false,
    );
    file.append(format!(
        "const schema: JSONSchema7 = {};",
        serde_json::to_string_pretty(&ir.json_schema())?,
    ));
    ir.walk_enums().for_each(|e| {
        file.append(format!(
            "registerEnumDeserializer(schema.definitions.{}, {{\n{}\n}});",
            e.elem().name,
            e.elem()
                .values
                .iter()
                .flat_map(|v| {
                    let Some(alias) = v.attributes.get("alias") else {
                        return vec![];
                    };

                    let Some(description) = v.attributes.get("description") else {
                        return vec![(alias.to_ts(), &v.elem.0)];
                    };

                    if let Expression::String(alias_str) = alias {
                        if let Expression::String(description_str) = description {
                            return vec![
                                (alias.to_ts(), &v.elem.0),
                                (format!("\"{}: {}\"", alias_str, description_str), &v.elem.0),
                            ];
                        }
                    }

                    vec![
                        (alias.to_ts(), &v.elem.0),
                        (
                            format!("[`${{{}}}: ${{{}}}`]", alias.to_ts(), description.to_ts()),
                            &v.elem.0,
                        ),
                    ]
                })
                .map(|(alias, value_name)| format!("  {}: \"{}\"", alias, value_name))
                .collect::<Vec<_>>()
                .join(",\n")
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
