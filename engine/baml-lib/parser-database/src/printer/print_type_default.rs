use serde::{de::Error, Deserialize, Serialize};
use serde_json::Value;

// Define Rust structs to represent the TypedDicts
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Meta {
    description: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct PrimitiveType {
    // In Rust, Literal types are not supported, so we'll use String
    optional: bool,
    value: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct EnumType {
    name: String,
    optional: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct FieldType {
    name: String,
    meta: Option<Meta>,

    type_meta: Box<DataType>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ClassType {
    fields: Vec<FieldType>,
    optional: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ListType {
    dims: i32,
    inner: Box<DataType>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct InlineType {
    type_meta: Box<DataType>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct OutputType {
    value: Box<DataType>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct UnionType {
    members: Vec<DataType>,
}

// Use an enum to represent the union of types
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase", untagged)]
enum DataType {
    Primitive(PrimitiveType),
    Class(ClassType),
    List(ListType),
    Inline(InlineType),
    Union(UnionType),
    Output(OutputType),
    Enum(EnumType),
}

// Utility functions similar to the Python version
fn as_comment(text: &str) -> String {
    text.lines()
        .map(|line| format!("// {}", line.trim()))
        .collect::<Vec<String>>()
        .join("\n")
}

fn as_indented_string(content: &str, level: usize) -> String {
    let indentation = "  ".repeat(level);
    content
        .lines()
        .map(|line| format!("{}{}", indentation, line))
        .collect::<Vec<String>>()
        .join("\n")
}

fn print_optional(value: &str, is_optional: bool) -> String {
    if is_optional {
        format!("{} | null", value)
    } else {
        value.to_string()
    }
}

// The print functions will need to be adapted to Rust's ownership and borrowing rules

// Main function to print data type - this would need to be adapted to return a Rust String type
fn print_type(item: &DataType) -> String {
    match item {
        DataType::Primitive(p) => print_primitive(p),
        DataType::Class(c) => print_class(c),
        DataType::List(l) => print_list(l),
        DataType::Inline(i) => print_type(&i.type_meta),
        DataType::Union(u) => print_union(u),
        DataType::Output(o) => print_type(&o.value),
        DataType::Enum(e) => print_enum(e),
    }
}

fn print_primitive(item: &PrimitiveType) -> String {
    print_optional(&item.value, item.optional)
}

fn print_class(item: &ClassType) -> String {
    let fields: Vec<String> = item
        .fields
        .iter()
        .map(|field| {
            let description = field
                .meta
                .as_ref()
                .and_then(|m| m.description.clone())
                .unwrap_or_default();
            let comment = if !description.is_empty() {
                as_comment(&description) + "\n"
            } else {
                "".to_string()
            };
            let field_value = print_type(&field.type_meta);
            format!("{}\"{}\": {}", comment, field.name, field_value)
        })
        .collect();

    let class_content = as_indented_string(&fields.join(",\n"), 1);
    print_optional(&format!("{{\n{}\n}}", class_content), item.optional)
}

fn print_list(item: &ListType) -> String {
    let inner_type = item.inner.as_ref();
    let inner_type_str = match inner_type {
        DataType::Union(_) => format!("({})", print_type(inner_type)),
        DataType::Class(t) => {
            if t.optional {
                format!("({})", print_type(inner_type))
            } else {
                print_type(inner_type)
            }
        }
        DataType::Enum(t) => {
            if t.optional {
                format!("({})", print_type(inner_type))
            } else {
                print_type(inner_type)
            }
        }
        _ => print_type(inner_type),
    };
    let dims_str = (0..item.dims).map(|_| "[]").collect::<String>();
    format!("{inner_type_str}{dims_str}")
}

fn print_enum(item: &EnumType) -> String {
    print_optional(&format!("\"{} as string\"", item.name), item.optional)
}

fn print_union(item: &UnionType) -> String {
    let member_types: Vec<String> = item.members.iter().map(print_type).collect();
    member_types.join(" | ").to_string()
}

fn parse_field_type(json_input: &Value) -> Result<FieldType, serde_json::Error> {
    let name = json_input["name"].as_str().unwrap().to_string();
    let meta: Option<Meta> = serde_json::from_value(json_input["meta"].clone())?;
    let type_meta = parse_data_type(&json_input["type_meta"])?;
    Ok(FieldType {
        name,
        meta,
        type_meta: Box::new(type_meta),
    })
}

fn parse_data_type(json_input: &Value) -> Result<DataType, serde_json::Error> {
    let json_input = json_input.as_object().unwrap();

    match json_input["rtype"].as_str() {
        Some("primitive") => Ok(DataType::Primitive(PrimitiveType {
            optional: json_input["optional"].as_bool().unwrap_or(false),
            value: json_input["value"].as_str().unwrap().to_string(),
        })),
        Some("class") => {
            let fields: Vec<FieldType> = json_input["fields"]
                .as_array()
                .ok_or_else(|| Error::custom("Expected 'fields' to be an array"))?
                .iter()
                .map(parse_field_type)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(DataType::Class(ClassType {
                fields,
                optional: json_input["optional"].as_bool().unwrap_or(false),
            }))
        }
        Some("enum") => {
            let name = json_input["name"].as_str().unwrap().to_string();
            let optional = json_input["optional"].as_bool().unwrap_or(false);
            Ok(DataType::Enum(EnumType { name, optional }))
        }
        Some("list") => {
            let inner = parse_data_type(&json_input["inner"])?;
            Ok(DataType::List(ListType {
                dims: json_input["dims"].as_i64().unwrap() as i32,
                inner: Box::new(inner),
            }))
        }
        Some("inline") => {
            let inner = parse_data_type(&json_input["value"])?;
            Ok(DataType::Inline(InlineType {
                type_meta: Box::new(inner),
            }))
        }
        Some("output") => {
            let inner = parse_data_type(&json_input["value"])?;
            Ok(DataType::Output(OutputType {
                value: Box::new(inner),
            }))
        }
        Some("union") => {
            let members: Vec<DataType> = json_input["options"]
                .as_array()
                .ok_or_else(|| Error::custom("Expected 'options' to be an array"))?
                .iter()
                .map(parse_data_type)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(DataType::Union(UnionType { members }))
        }
        other => Err(Error::custom(format!(
            "unknown type {:?} {:?}",
            other, json_input
        ))),
    }
}

pub(crate) fn print_entry(json_input: serde_json::Value) -> String {
    // Print the type of the input
    let parsed: DataType = parse_data_type(&json_input).expect("Invalid JSON input");

    print_type(&parsed)
}
