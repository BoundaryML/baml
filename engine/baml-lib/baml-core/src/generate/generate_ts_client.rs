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
        let alias_value_name_pairs = e
            .elem()
            .values
            .iter()
            .flat_map(|v| -> Vec<(String, &String)> {
                let alias = v.attributes.get("alias");
                let description = v.attributes.get("description");
                match (alias, description) {
                    (Some(Expression::String(alias)), Some(Expression::String(description))) => {
                        // "alias" and "alias: description"
                        vec![
                            (format!("{}", alias.to_string()), &v.elem.0),
                            (format!("{}: {}", alias, description), &v.elem.0),
                        ]
                    }
                    (Some(Expression::String(alias)), None) => {
                        // "alias"
                        vec![(format!("{}", alias.to_string()), &v.elem.0)]
                    }
                    (None, Some(Expression::String(description))) => {
                        // "description"
                        vec![
                            (format!("{}: {}", v.elem.0, description), &v.elem.0),
                            (format!("{}", description), &v.elem.0),
                        ]
                    }
                    _ => vec![],
                }
            })
            .map(|(alias, value_name)| {
                format!(
                    "  \"{}\": \"{}\"",
                    alias.replace("\n", "\\n").replace("\"", "\\\""),
                    value_name
                )
            })
            .collect::<Vec<_>>();
        file.append(format!(
            "registerEnumDeserializer(schema.definitions.{}, {{\n{}\n}});",
            e.elem().name,
            alias_value_name_pairs.join(",\n")
        ))
    });
    ir.walk_classes().for_each(|c| {
        file.append(format!(
            "registerObjectDeserializer(schema.definitions.{}, {{\n{}\n}});",
            c.elem().name,
            c.elem()
                .static_fields
                .iter()
                .flat_map(|v| {
                    let Some(alias) = v.attributes.get("alias") else {
                        return vec![];
                    };

                    let Some(description) = v.attributes.get("description") else {
                        return vec![(alias.to_ts(), &v.elem.name)];
                    };

                    if let Expression::String(alias_str) = alias {
                        if let Expression::String(description_str) = description {
                            return vec![
                                (alias.to_ts(), &v.elem.name),
                                (
                                    format!("\"{}: {}\"", alias_str, description_str),
                                    &v.elem.name,
                                ),
                            ];
                        }
                    }

                    vec![
                        (alias.to_ts(), &v.elem.name),
                        (
                            format!("[`${{{}}}: ${{{}}}`]", alias.to_ts(), description.to_ts()),
                            &v.elem.name,
                        ),
                    ]
                })
                .map(|(alias, value_name)| format!("  {}: \"{}\"", alias, value_name))
                .collect::<Vec<_>>()
                .join(",\n")
        ))
    });
    file.add_export("schema");
    collector.finish_file();
    ir.write(&mut collector);

    collector.commit(&gen.output_path)
}
