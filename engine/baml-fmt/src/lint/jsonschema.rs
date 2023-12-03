use baml_lib::{
    internal_baml_parser_database::walkers::{ClassWalker, EnumWalker},
    internal_baml_schema_ast::ast::{FieldType, Identifier, TypeValue, WithName},
};
use serde_json::{json, Value};

pub(crate) trait WithJsonSchema {
    fn json_schema(&self) -> Value;
}

impl WithJsonSchema for EnumWalker<'_> {
    fn json_schema(&self) -> Value {
        json!({
            self.name(): {
                "type": "string",
                "enum": self
                    .values()
                    .map(|v| v.name().to_string())
                    .collect::<Vec<_>>(),
            }
        })
    }
}

impl WithJsonSchema for ClassWalker<'_> {
    fn json_schema(&self) -> Value {
        let mut properties = json!({});
        for field in self.static_fields() {
            properties[field.name()] = field.ast_field().field_type.json_schema();
        }
        json!({
            self.name(): {
                "type": "object",
                "properties": properties,
            }
        })
    }
}

impl WithJsonSchema for FieldType {
    fn json_schema(&self) -> Value {
        match self {
            FieldType::Identifier(_, idn) => match idn {
                Identifier::Primitive(t, ..) => json!({
                    "type": match t {
                        TypeValue::String => "string",
                        TypeValue::Int => "integer",
                        TypeValue::Float => "number",
                        TypeValue::Bool => "boolean",
                        TypeValue::Null => "undefined",
                        TypeValue::Char => "string",
                    }
                }),
                Identifier::Local(name, _) => json!({
                    "$ref": format!("#/definitions/{}", name),
                }),
                _ => panic!("Not implemented"),
            },
            FieldType::List(item, dims, _) => {
                let mut inner = json!({
                    "type": "array",
                    "items": (*item).json_schema()
                });
                for _ in 1..*dims {
                    inner = json!({
                        "type": "array",
                        "items": inner
                    });
                }

                return inner;
            }
            FieldType::Dictionary(kv, _) => json!({
                "type": "object",
                "additionalProperties": {
                    "type": (*kv).1.json_schema(),
                }
            }),
            FieldType::Union(_, t, _) => json!({
                "anyOf": t.iter().map(|t| {
                    let res = t.json_schema();
                    // if res is a map, add a "title" field
                    if let Value::Object(res) = &res {
                        let mut res = res.clone();
                        res.insert("title".to_string(), json!(t.to_string()));
                        return json!(res);
                    }
                    res
                }
            ).collect::<Vec<_>>(),
            }),
            FieldType::Tuple(_, t, _) => json!({
                "type": "array",
                "items": t.iter().map(|t| t.json_schema()).collect::<Vec<_>>(),
            }),
        }
    }
}
