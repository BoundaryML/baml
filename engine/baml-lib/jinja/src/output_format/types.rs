use std::sync::Arc;

use anyhow::Result;
use baml_types::{FieldType, TypeValue};
use indexmap::{IndexMap, IndexSet};

#[derive(Debug)]
pub struct Name {
    name: String,
    rendered_name: Option<String>,
}

impl Name {
    pub fn new(name: String) -> Self {
        Self {
            name,
            rendered_name: None,
        }
    }

    pub fn new_with_alias(name: String, alias: Option<String>) -> Self {
        Self {
            name,
            rendered_name: alias,
        }
    }

    pub fn rendered_name(&self) -> &str {
        self.rendered_name.as_ref().unwrap_or(&self.name)
    }

    pub fn real_name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug)]
pub struct Enum {
    pub name: Name,
    // name and description
    pub values: Vec<(Name, Option<String>)>,
}

#[derive(Debug)]
pub struct Class {
    pub name: Name,
    // type and description
    pub fields: Vec<(Name, FieldType, Option<String>)>,
}

#[derive(Debug, Clone)]
pub struct OutputFormatContent {
    enums: Arc<IndexMap<String, Enum>>,
    classes: Arc<IndexMap<String, Class>>,
    target: FieldType,
}

enum RenderSetting<T> {
    Auto,
    Always(T),
    Never,
}

impl<T> Default for RenderSetting<T> {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(strum::EnumString, strum::VariantNames)]
pub(crate) enum MapStyle {
    #[strum(serialize = "angle")]
    TypeParameters,

    #[strum(serialize = "object")]
    ObjectLiteral,
}

pub(crate) struct RenderOptions {
    prefix: RenderSetting<String>,
    pub(crate) or_splitter: String,
    enum_value_prefix: RenderSetting<String>,
    always_hoist_enums: RenderSetting<bool>,
    map_style: MapStyle,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            prefix: RenderSetting::Auto,
            or_splitter: " or ".to_string(),
            enum_value_prefix: RenderSetting::Auto,
            always_hoist_enums: RenderSetting::Auto,
            map_style: MapStyle::TypeParameters,
        }
    }
}

impl RenderOptions {
    pub(crate) fn new(
        prefix: Option<Option<String>>,
        or_splitter: Option<String>,
        enum_value_prefix: Option<Option<String>>,
        always_hoist_enums: Option<bool>,
        map_style: Option<MapStyle>,
    ) -> Self {
        Self {
            prefix: prefix.map_or(RenderSetting::Auto, |p| {
                p.map_or(RenderSetting::Never, RenderSetting::Always)
            }),
            or_splitter: or_splitter.unwrap_or(" or ".to_string()),
            enum_value_prefix: enum_value_prefix.map_or(RenderSetting::Auto, |p| {
                p.map_or(RenderSetting::Never, RenderSetting::Always)
            }),
            always_hoist_enums: always_hoist_enums
                .map_or(RenderSetting::Auto, RenderSetting::Always),
            map_style: map_style.unwrap_or(MapStyle::TypeParameters),
        }
    }
}

struct Attribute {
    name: String,
    description: Option<String>,
}

struct EnumRender {
    name: String,
    delimiter: String,
    values: Vec<Attribute>,
}

impl EnumRender {
    fn to_string(&self, options: &RenderOptions) -> String {
        let mut result = format!("{}\n{}", self.name, self.delimiter);
        for value in &self.values {
            result.push_str(&format!(
                "\n{}{}",
                match options.enum_value_prefix {
                    RenderSetting::Auto => "- ",
                    RenderSetting::Always(ref prefix) => prefix,
                    RenderSetting::Never => "",
                },
                value.to_string()
            ));
        }
        result
    }
}

impl Attribute {
    fn to_string(&self) -> String {
        if let Some(description) = &self.description {
            format!("{}: {}", self.name, description.replace("\n", "\n  "))
        } else {
            self.name.clone()
        }
    }
}

struct ClassRender {
    #[allow(dead_code)]
    name: String,
    values: Vec<ClassFieldRender>,
}

struct ClassFieldRender {
    name: String,
    r#type: String,
    description: Option<String>,
}

impl std::fmt::Display for ClassRender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{{")?;
        for value in &self.values {
            if let Some(desc) = &value.description {
                writeln!(f, "  // {}", desc.replace("\n", "\n  // "))?;
            }
            writeln!(
                f,
                "  {}: {},",
                value.name,
                value.r#type.replace('\n', "\n  ")
            )?;
        }
        write!(f, "}}")
    }
}

struct MapRender<'s> {
    style: &'s MapStyle,
    key_type: String,
    value_type: String,
}

impl<'s> std::fmt::Display for MapRender<'s> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.style {
            MapStyle::TypeParameters => write!(f, "map<{}, {}>", self.key_type, self.value_type),
            MapStyle::ObjectLiteral => write!(f, "{{{}: {}}}", self.key_type, self.value_type),
        }
    }
}

struct RenderState {
    hoisted_enums: IndexSet<String>,
}

impl OutputFormatContent {
    pub fn new(enums: Vec<Enum>, classes: Vec<Class>, target: FieldType) -> Self {
        Self {
            enums: Arc::new(
                enums
                    .into_iter()
                    .map(|e| (e.name.name.clone(), e))
                    .collect(),
            ),
            classes: Arc::new(
                classes
                    .into_iter()
                    .map(|c| (c.name.name.clone(), c))
                    .collect(),
            ),
            target,
        }
    }

    fn prefix<'a>(&self, options: &'a RenderOptions) -> Option<&'a str> {
        match &options.prefix {
            RenderSetting::Always(prefix) => Some(prefix.as_str()),
            RenderSetting::Never => None,
            RenderSetting::Auto => match &self.target {
                FieldType::Primitive(TypeValue::String) => None,
                FieldType::Primitive(_) => Some("Answer as a: "),
                FieldType::Enum(_) => Some("Answer with any of the categories:\n"),
                FieldType::Class(_) => Some("Answer in JSON using this schema:\n"),
                FieldType::List(_) => Some("Answer with a JSON Array using this schema:\n"),
                FieldType::Union(_) => Some("Answer in JSON using any of these schemas:\n"),
                FieldType::Optional(_) => Some("Answer in JSON using this schema:\n"),
                FieldType::Map(_, _) => None,
                FieldType::Tuple(_) => None,
            },
        }
    }

    fn enum_to_string(&self, enm: &Enum, options: &RenderOptions) -> String {
        EnumRender {
            name: enm.name.rendered_name().to_string(),
            delimiter: "----".into(),
            values: enm
                .values
                .iter()
                .map(|(name, description)| Attribute {
                    name: name.rendered_name().to_string(),
                    description: description.clone(),
                })
                .collect(),
        }
        .to_string(options)
    }

    fn inner_type_render(
        &self,
        options: &RenderOptions,
        field: &FieldType,
        render_state: &mut RenderState,
        group_hoisted_literals: bool,
    ) -> Result<String, minijinja::Error> {
        Ok(match field {
            FieldType::Primitive(t) => match t {
                TypeValue::String => "string".to_string(),
                TypeValue::Int => "int".to_string(),
                TypeValue::Float => "float".to_string(),
                TypeValue::Bool => "bool".to_string(),
                TypeValue::Null => "null".to_string(),
                TypeValue::Image => {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::BadSerialization,
                        "Image type is not supported in outputs",
                    ))
                }
                TypeValue::Audio => {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::BadSerialization,
                        "Audio type is not supported in outputs",
                    ))
                }
            },
            FieldType::Enum(e) => {
                let Some(enm) = self.enums.get(e) else {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::BadSerialization,
                        format!("Enum {} not found", e),
                    ));
                };

                if enm.values.len() <= 6
                    && enm.values.iter().all(|(_, d)| d.is_none())
                    && !group_hoisted_literals
                    && !matches!(options.always_hoist_enums, RenderSetting::Always(true))
                {
                    let values = enm
                        .values
                        .iter()
                        .map(|(n, _)| format!("'{}'", n.rendered_name()))
                        .collect::<Vec<_>>()
                        .join(&options.or_splitter);

                    values
                } else {
                    render_state.hoisted_enums.insert(enm.name.name.clone());
                    enm.name.rendered_name().to_string()
                }
            }
            FieldType::Class(cls) => {
                let Some(class) = self.classes.get(cls) else {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::BadSerialization,
                        format!("Class {} not found", cls),
                    ));
                };

                ClassRender {
                    name: class.name.rendered_name().to_string(),
                    values: class
                        .fields
                        .iter()
                        .map(|(n, t, d)| {
                            Ok(ClassFieldRender {
                                name: n.rendered_name().to_string(),
                                r#type: self.inner_type_render(options, t, render_state, false)?,
                                description: d.clone(),
                            })
                        })
                        .collect::<Result<_, minijinja::Error>>()?,
                }
                .to_string()
            }
            FieldType::List(inner) => {
                let inner_str = self.inner_type_render(options, inner, render_state, false)?;

                if match inner.as_ref() {
                    FieldType::Primitive(_) => false,
                    FieldType::Optional(t) => !t.is_primitive(),
                    FieldType::Enum(e) => inner_str.len() > 15,
                    _ => true,
                } {
                    format!("[\n  {}\n]", inner_str.replace('\n', "\n  "))
                } else {
                    if matches!(inner.as_ref(), FieldType::Optional(_)) {
                        format!("({})[]", inner_str)
                    } else {
                        format!("{}[]", inner_str)
                    }
                }
            }
            FieldType::Union(items) => items
                .iter()
                .map(|t| self.inner_type_render(options, t, render_state, true))
                .collect::<Result<Vec<_>, minijinja::Error>>()?
                .join(&options.or_splitter),
            FieldType::Optional(inner) => {
                let inner_str = self.inner_type_render(options, inner, render_state, false)?;
                if inner.is_optional() {
                    inner_str
                } else {
                    format!("{}{}null", inner_str, &options.or_splitter)
                }
            }
            FieldType::Tuple(_) => {
                return Err(minijinja::Error::new(
                    minijinja::ErrorKind::BadSerialization,
                    "Tuple type is not supported in outputs",
                ))
            }
            FieldType::Map(key_type, value_type) => MapRender {
                style: &options.map_style,
                key_type: self.inner_type_render(options, key_type, render_state, false)?,
                value_type: self.inner_type_render(options, value_type, render_state, false)?,
            }
            .to_string(),
        })
    }

    pub(crate) fn render(
        &self,
        options: RenderOptions,
    ) -> Result<Option<String>, minijinja::Error> {
        let prefix = self.prefix(&options);

        let mut render_state = RenderState {
            hoisted_enums: IndexSet::new(),
        };

        let message = match &self.target {
            FieldType::Primitive(TypeValue::String) if prefix.is_none() => None,
            FieldType::Enum(e) => {
                let Some(enm) = self.enums.get(e) else {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::BadSerialization,
                        format!("Enum {} not found", e),
                    ));
                };

                Some(self.enum_to_string(enm, &options))
            }
            _ => Some(self.inner_type_render(&options, &self.target, &mut render_state, false)?),
        };

        let enum_definitions = render_state
            .hoisted_enums
            .iter()
            .map(|e| {
                let enm = self.enums.get(e).expect("Enum not found");
                self.enum_to_string(enm, &options)
            })
            .collect::<Vec<_>>();

        match (prefix, message) {
            (Some(prefix), Some(message)) => {
                if enum_definitions.len() > 0 {
                    Ok(Some(format!(
                        "{}\n\n{}{}",
                        enum_definitions.join("\n\n"),
                        prefix,
                        message,
                    )))
                } else {
                    Ok(Some(format!("{}{}", prefix, message)))
                }
            }
            (None, Some(message)) => {
                if enum_definitions.len() > 0 {
                    Ok(Some(format!(
                        "{}\n\n{}",
                        enum_definitions.join("\n\n"),
                        message
                    )))
                } else {
                    Ok(Some(message))
                }
            }
            (Some(prefix), None) => {
                if enum_definitions.len() > 0 {
                    Ok(Some(format!(
                        "{}\n\n{}",
                        prefix,
                        enum_definitions.join("\n\n")
                    )))
                } else {
                    Ok(Some(prefix.to_string()))
                }
            }
            (None, None) => {
                if enum_definitions.len() > 0 {
                    Ok(Some(enum_definitions.join("\n\n")))
                } else {
                    Ok(None)
                }
            }
        }
    }
}

#[cfg(test)]
impl OutputFormatContent {
    pub fn new_array() -> Self {
        Self::new(
            vec![],
            vec![],
            FieldType::List(Box::new(FieldType::Primitive(TypeValue::String))),
        )
    }

    pub fn new_string() -> Self {
        Self::new(vec![], vec![], FieldType::Primitive(TypeValue::String))
    }
}

impl OutputFormatContent {
    pub fn find_enum(&self, name: &str) -> Result<&Enum> {
        self.enums
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Enum {} not found", name))
    }

    pub fn find_class(&self, name: &str) -> Result<&Class> {
        self.classes
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Class {} not found", name))
    }
}
