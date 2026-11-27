//! The library's interface as a typed tree: every export with its
//! parameters, result and throws type, and every class and enum they reach,
//! numbered. Parsed once at load from the JSON the library carries
//! (`baml_lib_interface_json`, what `native_pack --lib --describe` prints),
//! so a host builds its plans from types, never from their text.

use serde_json::Value;

use crate::SdkError;

/// Everything a library exports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interface {
    /// The interface hash, the one [`crate::Library::load`] checked.
    pub hash: u64,
    /// In export order: a function's number is its place here.
    pub functions: Vec<Function>,
    /// The classes and enums, numbered by their place here: the number
    /// [`crate::BamlValue::class_of`] returns.
    pub types: Vec<TypeDef>,
}

/// One exported function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    /// With its namespace, without the package:
    /// `complex_models.round_trip_complex_profile`.
    pub name: String,
    pub params: Vec<Param>,
    pub result: Type,
    /// What it throws, as a typed value. `None` when it declares nothing,
    /// or when what it throws cannot cross: its errors then arrive
    /// described.
    pub throws: Option<Type>,
    /// The signature as text: `add(a: int, b: int) -> int`.
    pub signature: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    /// The host may pass `OMITTED` for it.
    pub has_default: bool,
}

/// A class or an enum.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeDef {
    /// With its namespace, without the package: `complex_models.Invoice`.
    pub name: String,
    pub kind: TypeDefKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeDefKind {
    /// Its fields in declaration order: a field's place here is the index
    /// [`crate::BamlValue::field`] and [`crate::Session::instance`] take.
    Class { fields: Vec<Field> },
    /// Its variants in order: a variant's place here is its index.
    Enum { variants: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub ty: Type,
}

/// A type that crosses the boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    Int,
    Float,
    Bool,
    Null,
    String,
    LiteralInt(i64),
    LiteralBool(bool),
    LiteralString(String),
    /// The class numbered so in [`Interface::types`].
    Class(u32),
    /// The enum numbered so in [`Interface::types`].
    Enum(u32),
    /// One variant of an enum, by its index.
    EnumVariant {
        of: u32,
        index: u32,
    },
    List(Box<Type>),
    /// A map; its keys are strings.
    Map(Box<Type>),
    Union(Vec<Type>),
}

fn bad(what: &str) -> SdkError {
    SdkError::Load(format!("the library's interface: {what}"))
}

fn text<'v>(value: &'v Value, key: &str) -> Result<&'v str, SdkError> {
    value[key]
        .as_str()
        .ok_or_else(|| bad(&format!("no text `{key}`")))
}

fn list<'v>(value: &'v Value, key: &str) -> Result<&'v Vec<Value>, SdkError> {
    value[key]
        .as_array()
        .ok_or_else(|| bad(&format!("no list `{key}`")))
}

fn number(value: &Value) -> Result<u32, SdkError> {
    value
        .as_u64()
        .and_then(|number| u32::try_from(number).ok())
        .ok_or_else(|| bad("a type number"))
}

impl Type {
    fn parse(value: &Value) -> Result<Self, SdkError> {
        if let Some(name) = value.as_str() {
            return match name {
                "int" => Ok(Self::Int),
                "float" => Ok(Self::Float),
                "bool" => Ok(Self::Bool),
                "null" => Ok(Self::Null),
                "string" => Ok(Self::String),
                other => Err(bad(&format!("an unknown type `{other}`"))),
            };
        }
        let object = value.as_object().ok_or_else(|| bad("a type"))?;
        if let Some(literal) = object.get("literal") {
            return match literal {
                Value::Bool(value) => Ok(Self::LiteralBool(*value)),
                Value::String(value) => Ok(Self::LiteralString(value.clone())),
                Value::Number(value) => value
                    .as_i64()
                    .map(Self::LiteralInt)
                    .ok_or_else(|| bad("an int literal")),
                _ => Err(bad("a literal")),
            };
        }
        if let Some(class) = object.get("class") {
            return number(class).map(Self::Class);
        }
        if let Some(of) = object.get("enum") {
            let of = number(of)?;
            return match object.get("variant") {
                Some(index) => Ok(Self::EnumVariant {
                    of,
                    index: number(index)?,
                }),
                None => Ok(Self::Enum(of)),
            };
        }
        if let Some(element) = object.get("list") {
            return Ok(Self::List(Box::new(Self::parse(element)?)));
        }
        if let Some(value) = object.get("map") {
            return Ok(Self::Map(Box::new(Self::parse(value)?)));
        }
        if let Some(members) = object.get("union") {
            let members = members.as_array().ok_or_else(|| bad("a union"))?;
            return members
                .iter()
                .map(Self::parse)
                .collect::<Result<_, _>>()
                .map(Self::Union);
        }
        Err(bad("a type of an unknown shape"))
    }
}

impl Interface {
    /// Parse the JSON a library carries, as `native_pack --lib --describe`
    /// prints it.
    pub fn from_json(json: &str) -> Result<Self, SdkError> {
        let root: Value =
            serde_json::from_str(json).map_err(|error| bad(&format!("not JSON: {error}")))?;
        let hash =
            u64::from_str_radix(text(&root, "interface_hash")?, 16).map_err(|_| bad("the hash"))?;
        let functions = list(&root, "functions")?
            .iter()
            .map(|function| {
                let params = list(function, "params")?
                    .iter()
                    .map(|param| {
                        Ok(Param {
                            name: text(param, "name")?.to_owned(),
                            ty: Type::parse(&param["tree"])?,
                            has_default: param["has_default"].as_bool().unwrap_or(false),
                        })
                    })
                    .collect::<Result<_, SdkError>>()?;
                let throws = match &function["throws_tree"] {
                    Value::Null => None,
                    tree => Some(Type::parse(tree)?),
                };
                Ok(Function {
                    name: text(function, "name")?.to_owned(),
                    params,
                    result: Type::parse(&function["result_tree"])?,
                    throws,
                    signature: text(function, "signature")?.to_owned(),
                })
            })
            .collect::<Result<_, SdkError>>()?;
        let types = list(&root, "types")?
            .iter()
            .map(|ty| {
                let kind = match text(ty, "kind")? {
                    "class" => TypeDefKind::Class {
                        fields: list(ty, "fields")?
                            .iter()
                            .map(|field| {
                                Ok(Field {
                                    name: text(field, "name")?.to_owned(),
                                    ty: Type::parse(&field["tree"])?,
                                })
                            })
                            .collect::<Result<_, SdkError>>()?,
                    },
                    "enum" => TypeDefKind::Enum {
                        variants: list(ty, "variants")?
                            .iter()
                            .map(|variant| {
                                variant
                                    .as_str()
                                    .map(str::to_owned)
                                    .ok_or_else(|| bad("a variant"))
                            })
                            .collect::<Result<_, SdkError>>()?,
                    },
                    other => return Err(bad(&format!("a type of kind `{other}`"))),
                };
                Ok(TypeDef {
                    name: text(ty, "name")?.to_owned(),
                    kind,
                })
            })
            .collect::<Result<_, SdkError>>()?;
        Ok(Self {
            hash,
            functions,
            types,
        })
    }
}
