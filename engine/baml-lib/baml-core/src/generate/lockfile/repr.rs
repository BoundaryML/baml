use internal_baml_parser_database::walkers::{
    ClassWalker, EnumWalker, FunctionWalker, VariantWalker,
};
use internal_baml_schema_ast::ast::{self, FieldType, Identifier, TypeValue, WithName};
use serde_json::{json, Value};

pub(crate) trait WithRepr {
    fn repr(&self) -> Value;
}

#[derive(serde::Serialize)]
enum Primitive {
    STRING,
}

#[derive(serde::Serialize)]
enum Type {
    PRIMITIVE(Primitive),
    ENUM(Enum),
    CLASS(Class),
}

#[derive(serde::Serialize)]
struct Enum {
    name: String,
    // DO NOT LAND - need to model attributes
    values: Vec<String>,
}

impl WithRepr for EnumWalker<'_> {
    fn repr(&self) -> Value {
        serde_json::to_value(Enum {
            name: self.name().to_string(),
            values: self.values().map(|v| v.name().to_string()).collect(),
        })
        .unwrap()
    }
}

#[derive(serde::Serialize)]
struct Field {
    name: String,
    r#type: Type,
}

#[derive(serde::Serialize)]
struct Class {
    name: String,
    properties: Vec<Field>,
}

impl WithRepr for ClassWalker<'_> {
    fn repr(&self) -> Value {
        serde_json::to_value(Class {
            name: self.name().to_string(),
            properties: self
                .static_fields()
                .map(|field| Field {
                    name: field.name().to_string(),
                    // DO NOT LAND- needs to recurse
                    r#type: Type::PRIMITIVE(Primitive::STRING),
                })
                .collect(),
        })
        .unwrap()
    }
}

#[derive(serde::Serialize)]
enum ImplementationType {
    LLM,
}

#[derive(serde::Serialize)]
struct Implementation {
    r#type: ImplementationType,
    name: String,

    prompt: String,
    input_replacers: Vec<String>,
    output_replacers: Vec<String>,
    client: String,
    //        //            "type": "llm",
    //        //            "name": StringSpan::new(i.ast_variant().name(), &i.identifier().span()),
    //        //            "prompt_key": {
    //        //                "start": props.prompt.key_span.start,
    //        //                "end": props.prompt.key_span.end,
    //        //                "source_file": props.prompt.key_span.file.path(),
    //        //            },
    //        //            "prompt": props.prompt.value,
    //        //            "input_replacers": props.replacers.0.iter().map(
    //        //                |r| json!({
    //        //                    "key": r.0.key(),
    //        //                    "value": r.1,
    //        //                })
    //        //            ).collect::<Vec<_>>(),
    //        //            "output_replacers": props.replacers.1.iter().map(
    //        //                |r| json!({
    //        //                    "key": r.0.key(),
    //        //                    "value": r.1,
    //        //                })
    //        //            ).collect::<Vec<_>>(),
    //        //            "client": schema.db.find_client(&props.client.value).map(|c| StringSpan::new(c.name(), &c.identifier().span())).unwrap_or_else(|| StringSpan::new(&props.client.value, &props.client.span)),
}
#[derive(serde::Serialize)]
struct Function {
    name: String,
    // DO NOT LAND - need clarification
    //  - >1 inputs => ast::FunctionArgs::Named / NamedFunctionArgList
    //  - =1 input -> ast::FuncitonArgs::Unnamed / FunctionArg
    named_inputs: Vec<String>,
    positional_inputs: Vec<String>,
    output: Type,
    impls: Vec<Implementation>,
}

impl WithRepr for FunctionWalker<'_> {
    fn repr(&self) -> Value {
        serde_json::to_value(Function {
            name: self.name().to_string(),
            named_inputs: if let ast::FunctionArgs::Named(arg_list) = self.ast_function().input() {
                vec![]
            } else {
                vec![]
            },
            positional_inputs: if let ast::FunctionArgs::Unnamed(arg) = self.ast_function().input()
            {
                vec![]
            } else {
                vec![]
            },
            output: match self.ast_function().output() {
                ast::FunctionArgs::Named(arg_list) => Type::PRIMITIVE(Primitive::STRING),
                ast::FunctionArgs::Unnamed(arg) => Type::PRIMITIVE(Primitive::STRING),
            },
            impls: self
                .walk_variants()
                .map(|e| Implementation {
                    r#type: ImplementationType::LLM,
                    name: e.name().to_string(),
                    prompt: e.properties().prompt.value.clone(),
                    input_replacers: vec![],
                    output_replacers: vec![],
                    client: e.properties().client.value.clone(),
                })
                .collect(),
        })
        .unwrap()
        //    json!({
        //        "name": self.name(),
        //        "input": match self.ast_function().input() {
        //            ast::FunctionArgs::Named(arg_list) => json!({
        //                "arg_type": "named",
        //                "values": arg_list.args.iter().map(
        //                    |(id, arg)| json!({
        //                        "name": id.name(),
        //                        "type": format!("{}", arg.field_type),
        //                        "jsonSchema": arg.field_type.json_schema()

        //                    })
        //                ).collect::<Vec<_>>(),
        //            }),
        //            ast::FunctionArgs::Unnamed(arg) => json!({
        //                "arg_type": "positional",
        //                "type": format!("{}", arg.field_type),
        //                "jsonSchema": arg.field_type.json_schema()
        //            }),
        //        },
        //        // "output": match func.ast_function().output() {
        //        //     ast::FunctionArgs::Named(arg_list) => json!({
        //        //         "arg_type": "named",
        //        //         "values": arg_list.args.iter().map(
        //        //             |(id, arg)| json!({
        //        //                 "name": StringSpan::new(id.name(), &id.span()),
        //        //                 "type": format!("{}", arg.field_type),
        //        //                 "jsonSchema": arg.field_type.json_schema()
        //        //             })
        //        //         ).collect::<Vec<_>>(),
        //        //     }),
        //        //     ast::FunctionArgs::Unnamed(arg) => json!({
        //        //         "arg_type": "positional",
        //        //         "type": format!("{}", arg.field_type),
        //        //         "jsonSchema": arg.field_type.json_schema()
        //        //     }),
        //        // },
        //        //"test_cases": func.walk_tests().map(
        //        //    |t| {
        //        //        let props = t.test_case();
        //        //        json!({
        //        //            "name": StringSpan::new(t.name(), &t.identifier().span()),
        //        //            "content": props.content.value(),
        //        //        })
        //        //    }
        //        //).collect::<Vec<_>>(),
        //        //"impls": func.walk_variants().map(
        //        //    |i| {
        //        //        let props = i.properties();
        //        //        json!({
        //        //            "type": "llm",
        //        //            "name": StringSpan::new(i.ast_variant().name(), &i.identifier().span()),
        //        //            "prompt_key": {
        //        //                "start": props.prompt.key_span.start,
        //        //                "end": props.prompt.key_span.end,
        //        //                "source_file": props.prompt.key_span.file.path(),
        //        //            },
        //        //            "prompt": props.prompt.value,
        //        //            "input_replacers": props.replacers.0.iter().map(
        //        //                |r| json!({
        //        //                    "key": r.0.key(),
        //        //                    "value": r.1,
        //        //                })
        //        //            ).collect::<Vec<_>>(),
        //        //            "output_replacers": props.replacers.1.iter().map(
        //        //                |r| json!({
        //        //                    "key": r.0.key(),
        //        //                    "value": r.1,
        //        //                })
        //        //            ).collect::<Vec<_>>(),
        //        //            "client": schema.db.find_client(&props.client.value).map(|c| StringSpan::new(c.name(), &c.identifier().span())).unwrap_or_else(|| StringSpan::new(&props.client.value, &props.client.span)),

        //        //        })
        //        //    }
        //        //).collect::<Vec<_>>(),
        //    })
    }
}

// impl WithJsonSchema for FieldType {
//     fn json_schema(&self) -> Value {
//         match self {
//             FieldType::Identifier(_, idn) => match idn {
//                 Identifier::Primitive(t, ..) => json!({
//                     "type": match t {
//                         TypeValue::String => "string",
//                         TypeValue::Int => "integer",
//                         TypeValue::Float => "number",
//                         TypeValue::Bool => "boolean",
//                         TypeValue::Null => "undefined",
//                         TypeValue::Char => "string",
//                     }
//                 }),
//                 Identifier::Local(name, _) => json!({
//                     "$ref": format!("#/definitions/{}", name),
//                 }),
//                 _ => panic!("Not implemented"),
//             },
//             FieldType::List(item, dims, _) => {
//                 let mut inner = json!({
//                     "type": "array",
//                     "items": (*item).json_schema()
//                 });
//                 for _ in 1..*dims {
//                     inner = json!({
//                         "type": "array",
//                         "items": inner
//                     });
//                 }
//
//                 return inner;
//             }
//             FieldType::Dictionary(kv, _) => json!({
//                 "type": "object",
//                 "additionalProperties": {
//                     "type": (*kv).1.json_schema(),
//                 }
//             }),
//             FieldType::Union(_, t, _) => json!({
//                 "anyOf": t.iter().map(|t| {
//                     let res = t.json_schema();
//                     // if res is a map, add a "title" field
//                     if let Value::Object(res) = &res {
//                         let mut res = res.clone();
//                         res.insert("title".to_string(), json!(t.to_string()));
//                         return json!(res);
//                     }
//                     res
//                 }
//             ).collect::<Vec<_>>(),
//             }),
//             FieldType::Tuple(_, t, _) => json!({
//                 "type": "array",
//                 "items": t.iter().map(|t| t.json_schema()).collect::<Vec<_>>(),
//             }),
//         }
//     }
// }
