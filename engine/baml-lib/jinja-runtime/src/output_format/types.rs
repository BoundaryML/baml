use std::sync::Arc;

use anyhow::Result;
use baml_types::{Constraint, FieldType, StreamingBehavior, TypeValue};
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

// TODO: (Greg) Enum needs to carry its constraints.
#[derive(Debug)]
pub struct Enum {
    pub name: Name,
    // name and description
    pub values: Vec<(Name, Option<String>)>,
    pub constraints: Vec<Constraint>,
}

/// The components of a Class needed to render `OutputFormatContent`.
/// This type is also used by `jsonish` to drive flexible parsing.
#[derive(Debug)]
pub struct Class {
    pub name: Name,
    // fields have name, type, description, and streaming_needed.
    pub fields: Vec<(Name, FieldType, Option<String>, bool)>,
    pub constraints: Vec<Constraint>,
    pub streaming_behavior: StreamingBehavior,
}

#[derive(Debug, Clone)]
pub struct OutputFormatContent {
    pub enums: Arc<IndexMap<String, Enum>>,
    pub classes: Arc<IndexMap<String, Class>>,
    recursive_classes: Arc<IndexSet<String>>,
    structural_recursive_aliases: Arc<IndexMap<String, FieldType>>,
    pub target: FieldType,
}

/// Builder for [`OutputFormatContent`].
pub struct Builder {
    enums: Vec<Enum>,
    classes: Vec<Class>,
    /// Order matters for this one.
    recursive_classes: IndexSet<String>,
    /// Recursive aliases introduced maps and lists.
    structural_recursive_aliases: IndexMap<String, FieldType>,
    target: FieldType,
}

impl Builder {
    pub fn new(target: FieldType) -> Self {
        Self {
            enums: vec![],
            classes: vec![],
            recursive_classes: IndexSet::new(),
            structural_recursive_aliases: IndexMap::new(),
            target,
        }
    }

    pub fn enums(mut self, enums: Vec<Enum>) -> Self {
        self.enums = enums;
        self
    }

    pub fn classes(mut self, classes: Vec<Class>) -> Self {
        self.classes = classes;
        self
    }

    pub fn recursive_classes(mut self, recursive_classes: IndexSet<String>) -> Self {
        self.recursive_classes = recursive_classes;
        self
    }

    pub fn structural_recursive_aliases(
        mut self,
        structural_recursive_aliases: IndexMap<String, FieldType>,
    ) -> Self {
        self.structural_recursive_aliases = structural_recursive_aliases;
        self
    }

    pub fn target(mut self, target: FieldType) -> Self {
        self.target = target;
        self
    }

    pub fn build(self) -> OutputFormatContent {
        OutputFormatContent {
            enums: Arc::new(
                self.enums
                    .into_iter()
                    .map(|e| (e.name.name.clone(), e))
                    .collect(),
            ),
            classes: Arc::new(
                self.classes
                    .into_iter()
                    .map(|c| (c.name.name.clone(), c))
                    .collect(),
            ),
            recursive_classes: Arc::new(self.recursive_classes.into_iter().collect()),
            structural_recursive_aliases: Arc::new(
                self.structural_recursive_aliases.into_iter().collect(),
            ),
            target: self.target,
        }
    }
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
    hoisted_class_prefix: RenderSetting<String>,
    always_hoist_enums: RenderSetting<bool>,
    map_style: MapStyle,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            prefix: RenderSetting::Auto,
            or_splitter: Self::DEFAULT_OR_SPLITTER.to_string(),
            enum_value_prefix: RenderSetting::Auto,
            hoisted_class_prefix: RenderSetting::Auto,
            always_hoist_enums: RenderSetting::Auto,
            map_style: MapStyle::TypeParameters,
        }
    }
}

impl RenderOptions {
    const DEFAULT_OR_SPLITTER: &'static str = " or ";
    const DEFAULT_TYPE_PREFIX_IN_RENDER_MESSAGE: &'static str = "schema";

    pub(crate) fn new(
        prefix: Option<Option<String>>,
        or_splitter: Option<String>,
        enum_value_prefix: Option<Option<String>>,
        always_hoist_enums: Option<bool>,
        map_style: Option<MapStyle>,
        hoisted_class_prefix: Option<Option<String>>,
    ) -> Self {
        Self {
            prefix: prefix.map_or(RenderSetting::Auto, |p| {
                p.map_or(RenderSetting::Never, RenderSetting::Always)
            }),
            or_splitter: or_splitter.unwrap_or(Self::DEFAULT_OR_SPLITTER.to_string()),
            enum_value_prefix: enum_value_prefix.map_or(RenderSetting::Auto, |p| {
                p.map_or(RenderSetting::Never, RenderSetting::Always)
            }),
            always_hoist_enums: always_hoist_enums
                .map_or(RenderSetting::Auto, RenderSetting::Always),
            map_style: map_style.unwrap_or(MapStyle::TypeParameters),
            hoisted_class_prefix: hoisted_class_prefix.map_or(RenderSetting::Auto, |p| {
                p.map_or(RenderSetting::Never, RenderSetting::Always)
            }),
        }
    }

    // TODO: Might need a builder pattern for this as well.
    pub(crate) fn with_hoisted_class_prefix(prefix: &str) -> Self {
        Self {
            hoisted_class_prefix: RenderSetting::Always(prefix.to_owned()),
            ..Default::default()
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
                value
            ));
        }
        result
    }
}

impl std::fmt::Display for Attribute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(description) = &self.description {
            write!(f, "{}: {}", self.name, description.replace("\n", "\n  "))
        } else {
            write!(f, "{}", self.name)
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

/// Basic grammar for "a" VS "an" indefinite articles.
///
/// It does NOT cover all rules & exceptions.
fn indefinite_article_a_or_an(word: &str) -> &str {
    match word.chars().next() {
        Some(c) if matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u') => "an",
        _ => "a",
    }
}

struct RenderState {
    hoisted_enums: IndexSet<String>,
}

impl OutputFormatContent {
    pub fn target(target: FieldType) -> Builder {
        Builder::new(target)
    }

    fn prefix(&self, options: &RenderOptions) -> Option<String> {
        fn auto_prefix(
            ft: &FieldType,
            options: &RenderOptions,
            output_format_content: &OutputFormatContent,
        ) -> Option<String> {
            match ft {
                FieldType::Primitive(TypeValue::String) => None,
                FieldType::Primitive(p) => Some(format!(
                    "Answer as {article} ",
                    article = indefinite_article_a_or_an(&p.to_string())
                )),
                FieldType::Literal(_) => Some(String::from("Answer using this specific value:\n")),
                FieldType::Enum(_) => Some(String::from("Answer with any of the categories:\n")),
                FieldType::Class(cls) => {
                    let type_prefix = match &options.hoisted_class_prefix {
                        RenderSetting::Always(prefix) if !prefix.is_empty() => prefix,
                        _ => RenderOptions::DEFAULT_TYPE_PREFIX_IN_RENDER_MESSAGE,
                    };

                    // Line break if schema else just inline the name.
                    let end = if output_format_content.recursive_classes.contains(cls) {
                        " "
                    } else {
                        "\n"
                    };

                    Some(format!("Answer in JSON using this {type_prefix}:{end}"))
                }
                FieldType::RecursiveTypeAlias(_) => {
                    let type_prefix = match &options.hoisted_class_prefix {
                        RenderSetting::Always(prefix) if !prefix.is_empty() => prefix,
                        _ => RenderOptions::DEFAULT_TYPE_PREFIX_IN_RENDER_MESSAGE,
                    };

                    Some(format!("Answer in JSON using this {type_prefix}: "))
                }
                FieldType::List(_) => Some(String::from(
                    "Answer with a JSON Array using this schema:\n",
                )),
                FieldType::Union(_) => {
                    Some(String::from("Answer in JSON using any of these schemas:\n"))
                }
                FieldType::Optional(_) => Some(String::from("Answer in JSON using this schema:\n")),
                FieldType::Map(_, _) => Some(String::from("Answer in JSON using this schema:\n")),
                FieldType::Tuple(_) => None,
                FieldType::WithMetadata { base, .. } => {
                    auto_prefix(base, options, output_format_content)
                }
            }
        }

        match &options.prefix {
            RenderSetting::Always(prefix) => Some(prefix.to_owned()),
            RenderSetting::Never => None,
            RenderSetting::Auto => auto_prefix(&self.target, options, self),
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

    /// Recursive classes are rendered using their name instead of schema.
    ///
    /// The schema must be hoisted and named, otherwise there's no way to refer
    /// to a recursive class.
    ///
    /// This function stops the recursion if it finds a recursive class and
    /// simply returns its name. It acts as wrapper for
    /// [`Self::inner_type_render`] and must be called wherever we could
    /// encounter a recursive type when rendering.
    ///
    /// Do not call this function as an entry point because if the target type
    /// is recursive itself you own't get any rendering! You'll just get the
    /// name of the type. Instead call [`Self::inner_type_render`] as an entry
    /// point and that will render the schema considering recursive fields.
    fn render_possibly_recursive_type(
        &self,
        options: &RenderOptions,
        field_type: &FieldType,
        render_state: &mut RenderState,
        group_hoisted_literals: bool,
    ) -> Result<String, minijinja::Error> {
        match field_type {
            FieldType::Class(nested_class) if self.recursive_classes.contains(nested_class) => {
                Ok(nested_class.to_owned())
            }

            _ => self.inner_type_render(options, field_type, render_state, group_hoisted_literals),
        }
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
                TypeValue::Media(media_type) => {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::BadSerialization,
                        format!("type '{media_type}' is not supported in outputs"),
                    ))
                }
            },
            FieldType::Literal(v) => v.to_string(),
            FieldType::WithMetadata { base, .. } => self.render_possibly_recursive_type(
                options,
                base,
                render_state,
                group_hoisted_literals,
            )?,
            FieldType::Enum(e) => {
                let Some(enm) = self.enums.get(e) else {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::BadSerialization,
                        format!("Enum {e} not found"),
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
                        format!("Class {cls} not found"),
                    ));
                };

                ClassRender {
                    name: class.name.rendered_name().to_string(),
                    values: class
                        .fields
                        .iter()
                        .map(|(name, field_type, description, _streaming_needed)| {
                            Ok(ClassFieldRender {
                                name: name.rendered_name().to_string(),
                                description: description.clone(),
                                r#type: self.render_possibly_recursive_type(
                                    options,
                                    field_type,
                                    render_state,
                                    false,
                                )?,
                            })
                        })
                        .collect::<Result<_, minijinja::Error>>()?,
                }
                .to_string()
            }
            FieldType::RecursiveTypeAlias(name) => name.to_owned(),
            FieldType::List(inner) => {
                let is_recursive = match inner.as_ref() {
                    FieldType::Class(nested_class) => self.recursive_classes.contains(nested_class),
                    FieldType::RecursiveTypeAlias(name) => {
                        self.structural_recursive_aliases.contains_key(name)
                    }
                    _ => false,
                };

                let inner_str =
                    self.render_possibly_recursive_type(options, inner, render_state, false)?;

                if !is_recursive
                    && match inner.as_ref() {
                        FieldType::Primitive(_) => false,
                        FieldType::Optional(t) => !t.is_primitive(),
                        FieldType::Enum(_e) => inner_str.len() > 15,
                        _ => true,
                    }
                {
                    format!("[\n  {}\n]", inner_str.replace('\n', "\n  "))
                } else if matches!(inner.as_ref(), FieldType::Optional(_)) {
                    format!("({})[]", inner_str)
                } else {
                    format!("{}[]", inner_str)
                }
            }
            FieldType::Union(items) => items
                .iter()
                .map(|t| self.render_possibly_recursive_type(options, t, render_state, false))
                .collect::<Result<Vec<_>, minijinja::Error>>()?
                .join(&options.or_splitter),
            FieldType::Optional(inner) => {
                let inner_str =
                    self.render_possibly_recursive_type(options, inner, render_state, false)?;
                if inner.is_optional() {
                    inner_str
                } else {
                    format!("{inner_str}{}null", options.or_splitter)
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
                // NOTE: Key can't be recursive because we only support strings
                // as keys.
                key_type: self.render_possibly_recursive_type(
                    options,
                    key_type,
                    render_state,
                    false,
                )?,
                value_type: self.render_possibly_recursive_type(
                    options,
                    value_type,
                    render_state,
                    false,
                )?,
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

        let mut message = match &self.target {
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

        // Top level recursive classes will just use their name instead of the
        // entire schema which should already be hoisted.
        if let FieldType::Class(class) = &self.target {
            if self.recursive_classes.contains(class) {
                message = Some(class.to_owned());
            }
        }

        let enum_definitions = Vec::from_iter(render_state.hoisted_enums.iter().map(|e| {
            let enm = self.enums.get(e).expect("Enum not found"); // TODO: Jinja Err
            self.enum_to_string(enm, &options)
        }));

        let mut class_definitions = Vec::new();
        let mut type_alias_definitions = Vec::new();

        // Hoist recursive classes. The render_state struct doesn't need to
        // contain these classes because we already know that we're gonna hoist
        // them beforehand. Recursive cycles are computed after the AST
        // validation stage.
        for class_name in self.recursive_classes.iter() {
            let schema = self.inner_type_render(
                &options,
                &FieldType::Class(class_name.to_owned()),
                &mut render_state,
                false,
            )?;

            class_definitions.push(match &options.hoisted_class_prefix {
                RenderSetting::Always(prefix) if !prefix.is_empty() => {
                    format!("{prefix} {class_name} {schema}")
                }
                _ => format!("{class_name} {schema}"),
            });
        }

        for (alias, target) in self.structural_recursive_aliases.iter() {
            let recursive_pointer =
                self.inner_type_render(&options, target, &mut render_state, false)?;

            type_alias_definitions.push(match &options.hoisted_class_prefix {
                RenderSetting::Always(prefix) if !prefix.is_empty() => {
                    format!("{prefix} {alias} = {recursive_pointer}")
                }
                _ => format!("{alias} = {recursive_pointer}"),
            });
        }

        let mut output = String::new();

        if !enum_definitions.is_empty() {
            output.push_str(&enum_definitions.join("\n\n"));
            output.push_str("\n\n");
        }

        if !class_definitions.is_empty() {
            output.push_str(&class_definitions.join("\n\n"));
            output.push_str("\n\n");
        }

        if !type_alias_definitions.is_empty() {
            output.push_str(&type_alias_definitions.join("\n"));
            output.push_str("\n\n");
        }

        if let Some(p) = prefix {
            output.push_str(&p);
        }

        if let Some(m) = message {
            output.push_str(&m);
        }

        // Trim end.
        while let Some('\n') = output.chars().last() {
            output.pop();
        }

        if output.is_empty() {
            Ok(None)
        } else {
            Ok(Some(output))
        }
    }
}

#[cfg(test)]
impl OutputFormatContent {
    pub fn new_array() -> Self {
        Self::target(FieldType::List(Box::new(FieldType::string()))).build()
    }

    pub fn new_string() -> Self {
        Self::target(FieldType::string()).build()
    }
}

impl OutputFormatContent {
    pub fn find_enum(&self, name: &str) -> Result<&Enum> {
        self.enums
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Enum {name} not found"))
    }

    pub fn find_class(&self, name: &str) -> Result<&Class> {
        self.classes
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Class {name} not found"))
    }

    pub fn find_recursive_alias_target(&self, name: &str) -> Result<&FieldType> {
        self.structural_recursive_aliases
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Recursive alias {name} not found"))
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;

    #[test]
    fn render_string() {
        let content = OutputFormatContent::new_string();
        let rendered = content.render(RenderOptions::default()).unwrap();
        assert_eq!(rendered, None);
    }

    #[test]
    fn render_int() {
        let content = OutputFormatContent::target(FieldType::int()).build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        assert_eq!(rendered, Some("Answer as an int".into()));
    }

    #[test]
    fn render_float() {
        let content = OutputFormatContent::target(FieldType::float()).build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        assert_eq!(rendered, Some("Answer as a float".into()));
    }

    #[test]
    fn render_array() {
        let content = OutputFormatContent::new_array();
        let rendered = content.render(RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer with a JSON Array using this schema:\nstring[]".to_string())
        );
    }

    #[test]
    fn render_enum() {
        let enums = vec![Enum {
            name: Name::new("Color".to_string()),
            values: vec![
                (Name::new("Red".to_string()), None),
                (Name::new("Green".to_string()), None),
                (Name::new("Blue".to_string()), None),
            ],
            constraints: Vec::new(),
        }];

        let content = OutputFormatContent::target(FieldType::Enum("Color".to_string()))
            .enums(enums)
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                "Answer with any of the categories:\nColor\n----\n- Red\n- Green\n- Blue"
            ))
        );
    }

    #[test]
    fn render_class() {
        let classes = vec![Class {
            name: Name::new("Person".to_string()),
            fields: vec![
                (
                    Name::new("name".to_string()),
                    FieldType::string(),
                    Some("The person's name".to_string()),
                    false,
                ),
                (
                    Name::new("age".to_string()),
                    FieldType::int(),
                    Some("The person's age".to_string()),
                    false,
                ),
            ],
            constraints: Vec::new(),
            streaming_behavior: StreamingBehavior::default(),
        }];

        let content = OutputFormatContent::target(FieldType::class("Person"))
            .classes(classes)
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                "Answer in JSON using this schema:\n{\n  // The person's name\n  name: string,\n  // The person's age\n  age: int,\n}"
            ))
        );
    }

    #[test]
    fn render_class_with_multiline_descriptions() {
        let classes = vec![Class {
            name: Name::new("Education".to_string()),
            fields: vec![
                (
                    Name::new("school".to_string()),
                    FieldType::optional(FieldType::string()),
                    Some("111\n  ".to_string()),
                    false,
                ),
                (
                    Name::new("degree".to_string()),
                    FieldType::string(),
                    Some("2222222".to_string()),
                    false,
                ),
                (Name::new("year".to_string()), FieldType::int(), None, false),
            ],
            constraints: Vec::new(),
            streaming_behavior: StreamingBehavior::default(),
        }];

        let content = OutputFormatContent::target(FieldType::class("Education"))
            .classes(classes)
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                "Answer in JSON using this schema:\n{\n  // 111\n  //   \n  school: string or null,\n  // 2222222\n  degree: string,\n  year: int,\n}"
            ))
        );
    }

    #[test]
    fn render_top_level_union() {
        let classes = vec![
            Class {
                name: Name::new("Bug".to_string()),
                fields: vec![
                    (
                        Name::new("description".to_string()),
                        FieldType::string(),
                        None,
                        false,
                    ),
                    (Name::new("severity".to_string()), FieldType::string(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("Enhancement".to_string()),
                fields: vec![
                    (Name::new("title".to_string()), FieldType::string(), None, false),
                    (
                        Name::new("description".to_string()),
                        FieldType::string(),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("Documentation".to_string()),
                fields: vec![
                    (Name::new("module".to_string()), FieldType::string(), None, false),
                    (Name::new("format".to_string()), FieldType::string(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::Union(vec![
            FieldType::class("Bug"),
            FieldType::class("Enhancement"),
            FieldType::class("Documentation"),
        ]))
        .classes(classes)
        .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"Answer in JSON using any of these schemas:
{
  description: string,
  severity: string,
} or {
  title: string,
  description: string,
} or {
  module: string,
  format: string,
}"#
            ))
        );
    }

    #[test]
    fn render_nested_union() {
        let classes = vec![
            Class {
                name: Name::new("Issue".to_string()),
                fields: vec![
                    (
                        Name::new("category".to_string()),
                        FieldType::Union(vec![
                            FieldType::class("Bug"),
                            FieldType::class("Enhancement"),
                            FieldType::class("Documentation"),
                        ]),
                        None,
                        false,
                    ),
                    (Name::new("date".to_string()), FieldType::string(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("Bug".to_string()),
                fields: vec![
                    (
                        Name::new("description".to_string()),
                        FieldType::string(),
                        None,
                        false,
                    ),
                    (Name::new("severity".to_string()), FieldType::string(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("Enhancement".to_string()),
                fields: vec![
                    (Name::new("title".to_string()), FieldType::string(), None, false),
                    (
                        Name::new("description".to_string()),
                        FieldType::string(),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("Documentation".to_string()),
                fields: vec![
                    (Name::new("module".to_string()), FieldType::string(), None, false),
                    (Name::new("format".to_string()), FieldType::string(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::class("Issue"))
            .classes(classes)
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"Answer in JSON using this schema:
{
  category: {
    description: string,
    severity: string,
  } or {
    title: string,
    description: string,
  } or {
    module: string,
    format: string,
  },
  date: string,
}"#
            ))
        );
    }

    #[test]
    fn render_top_level_simple_recursive_class() {
        let classes = vec![Class {
            name: Name::new("Node".to_string()),
            fields: vec![
                (Name::new("data".to_string()), FieldType::int(), None, false),
                (
                    Name::new("next".to_string()),
                    FieldType::optional(FieldType::class("Node")),
                    None,
                    false,
                ),
            ],
            constraints: Vec::new(),
            streaming_behavior: StreamingBehavior::default(),
        }];

        let content = OutputFormatContent::target(FieldType::class("Node"))
            .classes(classes)
            .recursive_classes(IndexSet::from_iter(["Node".to_string()]))
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this schema: Node"#
            ))
        );
    }

    #[test]
    fn render_nested_simple_recursive_class() {
        let classes = vec![
            Class {
                name: Name::new("Node".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        FieldType::optional(FieldType::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("LinkedList".to_string()),
                fields: vec![
                    (
                        Name::new("head".to_string()),
                        FieldType::optional(FieldType::class("Node")),
                        None,
                        false,
                    ),
                    (Name::new("len".to_string()), FieldType::int(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::class("LinkedList"))
            .classes(classes)
            .recursive_classes(IndexSet::from_iter(["Node".to_string()]))
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this schema:
{
  head: Node or null,
  len: int,
}"#
            ))
        );
    }

    #[test]
    fn top_level_recursive_cycle() {
        let classes = vec![
            Class {
                name: Name::new("A".to_string()),
                fields: vec![(
                    Name::new("pointer".to_string()),
                    FieldType::class("B"),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("B".to_string()),
                fields: vec![(
                    Name::new("pointer".to_string()),
                    FieldType::class("C"),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("C".to_string()),
                fields: vec![(
                    Name::new("pointer".to_string()),
                    FieldType::optional(FieldType::class("A")),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::class("A"))
            .classes(classes)
            .recursive_classes(IndexSet::from_iter(
                ["A", "B", "C"].map(ToString::to_string),
            ))
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"A {
  pointer: B,
}

B {
  pointer: C,
}

C {
  pointer: A or null,
}

Answer in JSON using this schema: A"#
            ))
        );
    }

    #[test]
    fn nested_recursive_cycle() {
        let classes = vec![
            Class {
                name: Name::new("A".to_string()),
                fields: vec![(
                    Name::new("pointer".to_string()),
                    FieldType::class("B"),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("B".to_string()),
                fields: vec![(
                    Name::new("pointer".to_string()),
                    FieldType::class("C"),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("C".to_string()),
                fields: vec![(
                    Name::new("pointer".to_string()),
                    FieldType::optional(FieldType::class("A")),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("NonRecursive".to_string()),
                fields: vec![
                    (
                        Name::new("pointer".to_string()),
                        FieldType::class("A"),
                        None,
                        false,
                    ),
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (Name::new("field".to_string()), FieldType::bool(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::class("NonRecursive"))
            .classes(classes)
            .recursive_classes(IndexSet::from_iter(
                ["A", "B", "C"].map(ToString::to_string),
            ))
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"A {
  pointer: B,
}

B {
  pointer: C,
}

C {
  pointer: A or null,
}

Answer in JSON using this schema:
{
  pointer: A,
  data: int,
  field: bool,
}"#
            ))
        );
    }

    #[test]
    fn nested_class_in_hoisted_recursive_class() {
        let classes = vec![
            Class {
                name: Name::new("A".to_string()),
                fields: vec![
                    (
                        Name::new("pointer".to_string()),
                        FieldType::class("B"),
                        None,
                        false,
                    ),
                    (
                        Name::new("nested".to_string()),
                        FieldType::class("Nested"),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("B".to_string()),
                fields: vec![(
                    Name::new("pointer".to_string()),
                    FieldType::class("C"),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("C".to_string()),
                fields: vec![(
                    Name::new("pointer".to_string()),
                    FieldType::optional(FieldType::class("A")),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("NonRecursive".to_string()),
                fields: vec![
                    (
                        Name::new("pointer".to_string()),
                        FieldType::class("A"),
                        None,
                        false,
                    ),
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (Name::new("field".to_string()), FieldType::bool(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("Nested".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (Name::new("field".to_string()), FieldType::bool(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::class("NonRecursive"))
            .classes(classes)
            .recursive_classes(IndexSet::from_iter(
                ["A", "B", "C"].map(ToString::to_string),
            ))
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
            assert_eq!(
                rendered,
                Some(String::from(
r#"A {
  pointer: B,
  nested: {
    data: int,
    field: bool,
  },
}

B {
  pointer: C,
}

C {
  pointer: A or null,
}

Answer in JSON using this schema:
{
  pointer: A,
  data: int,
  field: bool,
}"#
            ))
        );
    }

    #[test]
    fn mutually_recursive_list() {
        let classes = vec![
            Class {
                name: Name::new("Tree".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("children".to_string()),
                        FieldType::class("Forest"),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("Forest".to_string()),
                fields: vec![(
                    Name::new("trees".to_string()),
                    FieldType::list(FieldType::class("Tree")),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::class("Tree"))
            .classes(classes)
            .recursive_classes(IndexSet::from_iter(
                ["Tree", "Forest"].map(ToString::to_string),
            ))
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
            assert_eq!(
                rendered,
                Some(String::from(
r#"Tree {
  data: int,
  children: Forest,
}

Forest {
  trees: Tree[],
}

Answer in JSON using this schema: Tree"#
            ))
        );
    }

    #[test]
    fn self_referential_union() {
        let classes = vec![Class {
            name: Name::new("SelfReferential".to_string()),
            fields: vec![(
                Name::new("recursion".to_string()),
                FieldType::Union(vec![
                    FieldType::int(),
                    FieldType::string(),
                    FieldType::optional(FieldType::class("SelfReferential")),
                ]),
                None,
                false,
            )],
            constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
        }];

        let content = OutputFormatContent::target(FieldType::class("SelfReferential"))
            .classes(classes)
            .recursive_classes(IndexSet::from_iter(
                ["SelfReferential"].map(ToString::to_string),
            ))
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
            assert_eq!(
                rendered,
                Some(String::from(
r#"SelfReferential {
  recursion: int or string or SelfReferential or null,
}

Answer in JSON using this schema: SelfReferential"#
            ))
        );
    }

    #[test]
    fn top_level_recursive_union() {
        let classes = vec![
            Class {
                name: Name::new("Node".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        FieldType::optional(FieldType::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("Tree".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("children".to_string()),
                        FieldType::list(FieldType::class("Tree")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::Union(vec![
            FieldType::class("Node"),
            FieldType::class("Tree"),
        ]))
        .classes(classes)
        .recursive_classes(IndexSet::from_iter(
            ["Node", "Tree"].map(ToString::to_string),
        ))
        .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
            assert_eq!(
                rendered,
                Some(String::from(
r#"Node {
  data: int,
  next: Node or null,
}

Tree {
  data: int,
  children: Tree[],
}

Answer in JSON using any of these schemas:
Node or Tree"#
            ))
        );
    }

    #[test]
    fn nested_recursive_union() {
        let classes = vec![
            Class {
                name: Name::new("DataType".to_string()),
                fields: vec![
                    (
                        Name::new("data_type".to_string()),
                        FieldType::Union(vec![FieldType::class("Node"), FieldType::class("Tree")]),
                        None,
                        false,
                    ),
                    (Name::new("len".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("description".to_string()),
                        FieldType::string(),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("Node".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        FieldType::optional(FieldType::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("Tree".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("children".to_string()),
                        FieldType::list(FieldType::class("Tree")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::class("DataType"))
            .classes(classes)
            .recursive_classes(IndexSet::from_iter(
                ["Node", "Tree"].map(ToString::to_string),
            ))
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
            assert_eq!(
                rendered,
                Some(String::from(
r#"Node {
  data: int,
  next: Node or null,
}

Tree {
  data: int,
  children: Tree[],
}

Answer in JSON using this schema:
{
  data_type: Node or Tree,
  len: int,
  description: string,
}"#
            ))
        );
    }

    #[test]
    fn top_level_recursive_union_with_non_recursive_class() {
        let classes = vec![
            Class {
                name: Name::new("Node".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        FieldType::optional(FieldType::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("Tree".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("children".to_string()),
                        FieldType::list(FieldType::class("Tree")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("NonRecursive".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (Name::new("tag".to_string()), FieldType::string(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::Union(vec![
            FieldType::class("Node"),
            FieldType::class("Tree"),
            FieldType::class("NonRecursive"),
        ]))
        .classes(classes)
        .recursive_classes(IndexSet::from_iter(
            ["Node", "Tree"].map(ToString::to_string),
        ))
        .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
            assert_eq!(
                rendered,
                Some(String::from(
r#"Node {
  data: int,
  next: Node or null,
}

Tree {
  data: int,
  children: Tree[],
}

Answer in JSON using any of these schemas:
Node or Tree or {
  data: int,
  tag: string,
}"#
            ))
        );
    }

    #[test]
    fn nested_recursive_union_with_non_recursive_class() {
        let classes = vec![
            Class {
                name: Name::new("DataType".to_string()),
                fields: vec![
                    (
                        Name::new("data_type".to_string()),
                        FieldType::Union(vec![
                            FieldType::class("Node"),
                            FieldType::class("Tree"),
                            FieldType::class("NonRecursive"),
                        ]),
                        None,
                        false,
                    ),
                    (Name::new("len".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("description".to_string()),
                        FieldType::string(),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("Node".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        FieldType::optional(FieldType::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("Tree".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("children".to_string()),
                        FieldType::list(FieldType::class("Tree")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("NonRecursive".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (Name::new("tag".to_string()), FieldType::string(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::class("DataType"))
            .classes(classes)
            .recursive_classes(IndexSet::from_iter(
                ["Node", "Tree"].map(ToString::to_string),
            ))
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
            assert_eq!(
                rendered,
                Some(String::from(
r#"Node {
  data: int,
  next: Node or null,
}

Tree {
  data: int,
  children: Tree[],
}

Answer in JSON using this schema:
{
  data_type: Node or Tree or {
    data: int,
    tag: string,
  },
  len: int,
  description: string,
}"#
            ))
        );
    }

    #[test]
    fn render_hoisted_classes_with_prefix() {
        let classes = vec![
            Class {
                name: Name::new("A".to_string()),
                fields: vec![(
                    Name::new("pointer".to_string()),
                    FieldType::class("B"),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("B".to_string()),
                fields: vec![(
                    Name::new("pointer".to_string()),
                    FieldType::class("C"),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("C".to_string()),
                fields: vec![(
                    Name::new("pointer".to_string()),
                    FieldType::optional(FieldType::class("A")),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("NonRecursive".to_string()),
                fields: vec![
                    (
                        Name::new("pointer".to_string()),
                        FieldType::class("A"),
                        None,
                        false,
                    ),
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (Name::new("field".to_string()), FieldType::bool(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::class("NonRecursive"))
            .classes(classes)
            .recursive_classes(IndexSet::from_iter(
                ["A", "B", "C"].map(ToString::to_string),
            ))
            .build();
        let rendered = content
            .render(RenderOptions::with_hoisted_class_prefix("interface"))
            .unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"interface A {
  pointer: B,
}

interface B {
  pointer: C,
}

interface C {
  pointer: A or null,
}

Answer in JSON using this interface:
{
  pointer: A,
  data: int,
  field: bool,
}"#
            ))
        );
    }

    #[test]
    fn top_level_union_of_unions_pointing_to_recursive_class() {
        let classes = vec![
            Class {
                name: Name::new("Node".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        FieldType::optional(FieldType::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("Tree".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("children".to_string()),
                        FieldType::list(FieldType::class("Tree")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::Union(vec![
            FieldType::Union(vec![FieldType::class("Node"), FieldType::int()]),
            FieldType::Union(vec![FieldType::string(), FieldType::class("Tree")]),
        ]))
        .classes(classes)
        .recursive_classes(IndexSet::from_iter(
            ["Node", "Tree"].map(ToString::to_string),
        ))
        .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
            assert_eq!(
                rendered,
                Some(String::from(
r#"Node {
  data: int,
  next: Node or null,
}

Tree {
  data: int,
  children: Tree[],
}

Answer in JSON using any of these schemas:
Node or int or string or Tree"#
            ))
        );
    }

    #[test]
    fn nested_union_of_unions_pointing_to_recursive_class() {
        let classes = vec![
            Class {
                name: Name::new("Node".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        FieldType::optional(FieldType::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("Tree".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("children".to_string()),
                        FieldType::list(FieldType::class("Tree")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("NonRecursive".to_string()),
                fields: vec![
                    (
                        Name::new("the_union".to_string()),
                        FieldType::Union(vec![
                            FieldType::Union(vec![FieldType::class("Node"), FieldType::int()]),
                            FieldType::Union(vec![FieldType::string(), FieldType::class("Tree")]),
                        ]),
                        None,
                        false,
                    ),
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (Name::new("field".to_string()), FieldType::bool(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::class("NonRecursive"))
            .classes(classes)
            .recursive_classes(IndexSet::from_iter(
                ["Node", "Tree"].map(ToString::to_string),
            ))
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
            assert_eq!(
                rendered,
                Some(String::from(
r#"Node {
  data: int,
  next: Node or null,
}

Tree {
  data: int,
  children: Tree[],
}

Answer in JSON using this schema:
{
  the_union: Node or int or string or Tree,
  data: int,
  field: bool,
}"#
            ))
        );
    }

    #[test]
    fn render_top_level_list_with_recursive_items() {
        let classes = vec![Class {
            name: Name::new("Node".to_string()),
            fields: vec![
                (Name::new("data".to_string()), FieldType::int(), None, false),
                (
                    Name::new("next".to_string()),
                    FieldType::optional(FieldType::class("Node")),
                    None,
                    false,
                ),
            ],
            constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
        }];

        let content = OutputFormatContent::target(FieldType::list(FieldType::class("Node")))
            .classes(classes)
            .recursive_classes(IndexSet::from_iter(["Node".to_string()]))
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"Node {
  data: int,
  next: Node or null,
}

Answer with a JSON Array using this schema:
Node[]"#
            ))
        );
    }

    #[test]
    fn render_top_level_class_with_self_referential_map() {
        let classes = vec![Class {
            name: Name::new("RecursiveMap".to_string()),
            fields: vec![(
                Name::new("data".to_string()),
                FieldType::map(FieldType::string(), FieldType::class("RecursiveMap")),
                None,
                false
            )],
            constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
        }];

        let content = OutputFormatContent::target(FieldType::class("RecursiveMap"))
            .classes(classes)
            .recursive_classes(IndexSet::from_iter(["RecursiveMap".to_string()]))
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"RecursiveMap {
  data: map<string, RecursiveMap>,
}

Answer in JSON using this schema: RecursiveMap"#
            ))
        );
    }

    #[test]
    fn render_nested_self_referential_map() {
        let classes = vec![
            Class {
                name: Name::new("RecursiveMap".to_string()),
                fields: vec![(
                    Name::new("data".to_string()),
                    FieldType::map(FieldType::string(), FieldType::class("RecursiveMap")),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("NonRecursive".to_string()),
                fields: vec![(
                    Name::new("rec_map".to_string()),
                    FieldType::Class("RecursiveMap".to_string()),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::class("NonRecursive"))
            .classes(classes)
            .recursive_classes(IndexSet::from_iter(["RecursiveMap".to_string()]))
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"RecursiveMap {
  data: map<string, RecursiveMap>,
}

Answer in JSON using this schema:
{
  rec_map: RecursiveMap,
}"#
            ))
        );
    }

    #[test]
    fn render_top_level_map_pointing_to_another_recursive_class() {
        let classes = vec![Class {
            name: Name::new("Node".to_string()),
            fields: vec![
                (Name::new("data".to_string()), FieldType::int(), None, false),
                (
                    Name::new("next".to_string()),
                    FieldType::optional(FieldType::class("Node")),
                    None,
                    false,
                ),
            ],
            constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
        }];

        let content = OutputFormatContent::target(FieldType::map(
            FieldType::string(),
            FieldType::class("Node"),
        ))
        .classes(classes)
        .recursive_classes(IndexSet::from_iter(["Node".to_string()]))
        .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this schema:
map<string, Node>"#
            ))
        );
    }

    #[test]
    fn render_nested_map_pointing_to_another_recursive_class() {
        let classes = vec![
            Class {
                name: Name::new("MapWithRecValue".to_string()),
                fields: vec![(
                    Name::new("data".to_string()),
                    FieldType::map(FieldType::string(), FieldType::class("Node")),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("Node".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        FieldType::optional(FieldType::class("Node")),
                        None,
                        false
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::class("MapWithRecValue"))
            .classes(classes)
            .recursive_classes(IndexSet::from_iter(["Node".to_string()]))
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this schema:
{
  data: map<string, Node>,
}"#
            ))
        );
    }

    #[test]
    fn render_nested_map_pointing_to_another_optional_recursive_class() {
        let classes = vec![
            Class {
                name: Name::new("MapWithRecValue".to_string()),
                fields: vec![(
                    Name::new("data".to_string()),
                    FieldType::map(
                        FieldType::string(),
                        FieldType::optional(FieldType::class("Node")),
                    ),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("Node".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        FieldType::optional(FieldType::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::class("MapWithRecValue"))
            .classes(classes)
            .recursive_classes(IndexSet::from_iter(["Node".to_string()]))
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this schema:
{
  data: map<string, Node or null>,
}"#
            ))
        );
    }

    #[test]
    fn render_top_level_map_pointing_to_recursive_union() {
        let classes = vec![
            Class {
                name: Name::new("Node".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        FieldType::optional(FieldType::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("NonRecursive".to_string()),
                fields: vec![
                    (Name::new("field".to_string()), FieldType::string(), None, false),
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::map(
            FieldType::string(),
            FieldType::union(vec![
                FieldType::class("Node"),
                FieldType::int(),
                FieldType::class("NonRecursive"),
            ]),
        ))
        .classes(classes)
        .recursive_classes(IndexSet::from_iter(["Node".to_string()]))
        .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this schema:
map<string, Node or int or {
  field: string,
  data: int,
}>"#
            ))
        );
    }

    #[test]
    fn render_nested_map_pointing_to_recursive_union() {
        let classes = vec![
            Class {
                name: Name::new("MapWithRecUnion".to_string()),
                fields: vec![(
                    Name::new("data".to_string()),
                    FieldType::map(
                        FieldType::string(),
                        FieldType::union(vec![
                            FieldType::class("Node"),
                            FieldType::int(),
                            FieldType::class("NonRecursive"),
                        ]),
                    ),
                    None,
                    false
                )],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("Node".to_string()),
                fields: vec![
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        FieldType::optional(FieldType::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
            Class {
                name: Name::new("NonRecursive".to_string()),
                fields: vec![
                    (Name::new("field".to_string()), FieldType::string(), None, false),
                    (Name::new("data".to_string()), FieldType::int(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: StreamingBehavior::default(),
            },
        ];

        let content = OutputFormatContent::target(FieldType::class("MapWithRecUnion"))
            .classes(classes)
            .recursive_classes(IndexSet::from_iter(["Node".to_string()]))
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this schema:
{
  data: map<string, Node or int or {
    field: string,
    data: int,
  }>,
}"#
            ))
        );
    }

    #[test]
    fn render_simple_recursive_aliases() {
        let content = OutputFormatContent::target(FieldType::RecursiveTypeAlias(
            "RecursiveMapAlias".to_string(),
        ))
        .structural_recursive_aliases(IndexMap::from([(
            "RecursiveMapAlias".to_string(),
            FieldType::map(
                FieldType::string(),
                FieldType::RecursiveTypeAlias("RecursiveMapAlias".to_string()),
            ),
        )]))
        .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"RecursiveMapAlias = map<string, RecursiveMapAlias>

Answer in JSON using this schema: RecursiveMapAlias"#
            ))
        );
    }

    #[test]
    fn render_recursive_alias_cycle() {
        let content = OutputFormatContent::target(FieldType::RecursiveTypeAlias("A".to_string()))
            .structural_recursive_aliases(IndexMap::from([
                (
                    "A".to_string(),
                    FieldType::RecursiveTypeAlias("B".to_string()),
                ),
                (
                    "B".to_string(),
                    FieldType::RecursiveTypeAlias("C".to_string()),
                ),
                (
                    "C".to_string(),
                    FieldType::list(FieldType::RecursiveTypeAlias("A".to_string())),
                ),
            ]))
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"A = B
B = C
C = A[]

Answer in JSON using this schema: A"#
            ))
        );
    }

    #[test]
    fn render_recursive_alias_cycle_with_hoist_prefix() {
        let content = OutputFormatContent::target(FieldType::RecursiveTypeAlias("A".to_string()))
            .structural_recursive_aliases(IndexMap::from([
                (
                    "A".to_string(),
                    FieldType::RecursiveTypeAlias("B".to_string()),
                ),
                (
                    "B".to_string(),
                    FieldType::RecursiveTypeAlias("C".to_string()),
                ),
                (
                    "C".to_string(),
                    FieldType::list(FieldType::RecursiveTypeAlias("A".to_string())),
                ),
            ]))
            .build();
        let rendered = content
            .render(RenderOptions::with_hoisted_class_prefix("type"))
            .unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"type A = B
type B = C
type C = A[]

Answer in JSON using this type: A"#
            ))
        );
    }
}
