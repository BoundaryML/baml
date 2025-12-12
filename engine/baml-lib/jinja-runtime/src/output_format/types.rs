use std::{ops::Deref, sync::Arc};

use anyhow::Result;
use baml_types::{ir_type::UnionTypeViewGeneric, type_meta, Constraint, TypeIR, TypeValue};
use indexmap::{IndexMap, IndexSet};

#[derive(Debug, PartialEq, Eq)]
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

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.name)?;
        if let Some(rendered_name) = &self.rendered_name {
            write!(f, "({rendered_name:?})")?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Enum {
    pub name: Name,
    // name and description
    pub values: Vec<(Name, Option<String>)>,
    pub constraints: Vec<Constraint>,
}

pub struct EnumValue {
    pub name: Name,
    pub description: Option<String>,
}

/// The components of a Class needed to render `OutputFormatContent`.
/// This type is also used by `jsonish` to drive flexible parsing.
#[derive(Debug)]
pub struct Class {
    pub name: Name,
    pub description: Option<String>,
    pub namespace: baml_types::StreamingMode,
    // fields have name, type, description, and streaming_needed.
    pub fields: Vec<(Name, TypeIR, Option<String>, bool)>,
    pub constraints: Vec<Constraint>,
    // We use this for parsing
    pub streaming_behavior: type_meta::base::StreamingBehavior,
}

impl std::fmt::Display for Class {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Class {{")?;
        writeln!(f, "    name: {}.{}", self.namespace, self.name)?;
        writeln!(f, "    fields: {{")?;
        for (name, r#type, description, streaming_needed) in &self.fields {
            writeln!(
                f,
                "      {name}: {type} (description: {description:?}, streaming_needed: {streaming_needed})",
            )?;
        }
        writeln!(f, "    }}")?;
        writeln!(f, "    constraints: {:?}", self.constraints)?;
        writeln!(f, "    streaming_behavior: {:?}", self.streaming_behavior)?;
        writeln!(f, "  }}")
    }
}

#[derive(Debug, Clone)]
pub struct OutputFormatContent {
    pub enums: Arc<IndexMap<String, Enum>>,
    pub classes: Arc<IndexMap<(String, baml_types::StreamingMode), Class>>,
    pub recursive_classes: Arc<IndexSet<String>>,
    pub structural_recursive_aliases: Arc<IndexMap<String, TypeIR>>,
    pub target: TypeIR,
}

impl std::fmt::Display for OutputFormatContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "OutputFormatContent {{")?;
        writeln!(f, "enums: {:?}", self.enums)?;
        writeln!(f, "classes:")?;
        for ((name, _), class) in self.classes.iter() {
            writeln!(f, "  {name}: {class}")?;
        }
        writeln!(f, "recursive_classes: {:?}", self.recursive_classes)?;
        writeln!(
            f,
            "structural_recursive_aliases: {:?}",
            self.structural_recursive_aliases
        )?;
        writeln!(f, "target: {}", self.target)?;
        writeln!(f, "}}")
    }
}

/// Builder for [`OutputFormatContent`].
pub struct Builder {
    enums: Vec<Enum>,
    classes: Vec<Class>,
    /// Order matters for this one.
    recursive_classes: IndexSet<String>,
    /// Recursive aliases introduced maps and lists.
    structural_recursive_aliases: IndexMap<String, TypeIR>,
    target: TypeIR,
}

impl Builder {
    pub fn new(target: TypeIR) -> Self {
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
        structural_recursive_aliases: IndexMap<String, TypeIR>,
    ) -> Self {
        self.structural_recursive_aliases = structural_recursive_aliases;
        self
    }

    pub fn target(mut self, target: TypeIR) -> Self {
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
                    .map(|c| ((c.name.name.clone(), c.namespace), c))
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

#[derive(Default)]
enum RenderSetting<T> {
    #[default]
    Auto,
    Always(T),
    Never,
}

#[derive(strum::EnumString, strum::VariantNames)]
pub(crate) enum MapStyle {
    #[strum(serialize = "angle")]
    TypeParameters,

    #[strum(serialize = "object")]
    ObjectLiteral,
}

/// Hoist classes setting.
///
/// Recursive classes are always hoisted.
#[derive(Debug)]
pub(crate) enum HoistClasses {
    /// Hoist all classes.
    All,
    /// Hoist only the specified subset.
    Subset(Vec<String>),
    /// Default behavior, hoist only recursive classes.
    Auto,
}

/// Maximum number of variants in the enum that we render without hoisting.
const INLINE_RENDER_ENUM_MAX_VALUES: usize = 6;

pub struct RenderOptions {
    prefix: RenderSetting<String>,
    pub(crate) or_splitter: String,
    enum_value_prefix: RenderSetting<String>,
    hoisted_class_prefix: RenderSetting<String>,
    hoist_classes: HoistClasses,
    always_hoist_enums: RenderSetting<bool>,
    map_style: MapStyle,
    quote_class_fields: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            prefix: RenderSetting::Auto,
            or_splitter: Self::DEFAULT_OR_SPLITTER.to_string(),
            enum_value_prefix: RenderSetting::Auto,
            hoisted_class_prefix: RenderSetting::Auto,
            hoist_classes: HoistClasses::Auto,
            always_hoist_enums: RenderSetting::Auto,
            map_style: MapStyle::TypeParameters,
            quote_class_fields: false,
        }
    }
}

impl RenderOptions {
    const DEFAULT_OR_SPLITTER: &'static str = " or ";
    const DEFAULT_TYPE_PREFIX_IN_RENDER_MESSAGE: &'static str = "schema";

    /// Option<Option<T>> Basically means that we can have a paremeter which
    /// 1. the user can completely omit: None
    /// 2. the user can set to null:     Some(None)
    ///
    /// This might be a little annoying, maybe we can change the code in mod.rs
    /// to flatten the types Option<Option<T>> => Option<T>
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        prefix: Option<Option<String>>,
        or_splitter: Option<String>,
        enum_value_prefix: Option<Option<String>>,
        always_hoist_enums: Option<bool>,
        map_style: Option<MapStyle>,
        hoisted_class_prefix: Option<Option<String>>,
        hoist_classes: Option<HoistClasses>,
        quote_class_fields: Option<bool>,
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
            hoist_classes: hoist_classes.unwrap_or(HoistClasses::Auto),
            quote_class_fields: quote_class_fields.unwrap_or(false),
        }
    }

    // TODO: Might need a builder pattern for this as well.
    pub(crate) fn with_hoisted_class_prefix(prefix: &str) -> Self {
        Self {
            hoisted_class_prefix: RenderSetting::Always(prefix.to_owned()),
            ..Default::default()
        }
    }

    // TODO: Might need a builder pattern for this as well.
    pub(crate) fn hoist_classes(hoist_classes: HoistClasses) -> Self {
        Self {
            hoist_classes,
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
    description: Option<String>,
    values: Vec<ClassFieldRender>,
    quote_fields: bool,
}

struct ClassFieldRender {
    name: String,
    r#type: String,
    description: Option<String>,
}

impl std::fmt::Display for ClassRender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Render class description if present
        writeln!(f, "{{")?;
        if let Some(desc) = &self.description {
            // Write description as comment before opening brace
            let desc = desc.trim();
            if !desc.is_empty() {
                for line in desc.lines() {
                    writeln!(f, "  // {}", line)?;
                }
                writeln!(f)?;
            }
        }

        for value in &self.values {
            if let Some(desc) = &value.description {
                writeln!(f, "  // {}", desc.replace("\n", "\n  // "))?;
            }
            if self.quote_fields {
                writeln!(
                    f,
                    "  \"{}\": {},",
                    value.name,
                    value.r#type.replace('\n', "\n  ")
                )?;
            } else {
                writeln!(
                    f,
                    "  {}: {},",
                    value.name,
                    value.r#type.replace('\n', "\n  ")
                )?;
            }
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

struct RenderCtx {
    hoisted_enums: IndexSet<String>,
    hoisted_classes: IndexSet<String>,
}

impl OutputFormatContent {
    pub fn target(target: TypeIR) -> Builder {
        Builder::new(target)
    }

    /// A temporary OutputFormatContent constructor used by Expression functions.
    /// Expression Functions have no prompt and no client, so OutputFormatContent
    /// is not applicable to them. However one is needed for generating a
    /// PromptRenderer, which is technically needed in order to call the
    /// function-calling methods of BamlRuntime.
    pub fn mk_fake() -> OutputFormatContent {
        OutputFormatContent {
            enums: Arc::new(IndexMap::new()),
            classes: Arc::new(IndexMap::new()),
            recursive_classes: Arc::new(IndexSet::new()),
            structural_recursive_aliases: Arc::new(IndexMap::new()),
            target: TypeIR::Primitive(TypeValue::String, Default::default()),
        }
    }

    fn prefix(&self, options: &RenderOptions, render_state: &RenderCtx) -> Option<String> {
        fn auto_prefix(
            ft: &TypeIR,
            options: &RenderOptions,
            render_state: &RenderCtx,
            _output_format_content: &OutputFormatContent,
        ) -> Option<String> {
            match ft {
                TypeIR::Primitive(TypeValue::String, _) => None,
                TypeIR::Primitive(p, _) => Some(format!(
                    "Answer as {article} ",
                    article = indefinite_article_a_or_an(&p.to_string())
                )),
                TypeIR::Literal(_, _) => Some(String::from("Answer using this specific value:\n")),
                TypeIR::Enum { .. } => Some(String::from("Answer with any of the categories:\n")),
                TypeIR::Class { name: cls, .. } => {
                    let type_prefix = match &options.hoisted_class_prefix {
                        RenderSetting::Always(prefix) if !prefix.is_empty() => prefix,
                        _ => RenderOptions::DEFAULT_TYPE_PREFIX_IN_RENDER_MESSAGE,
                    };

                    // Line break if schema else just inline the name.
                    let end = if render_state.hoisted_classes.contains(cls) {
                        " "
                    } else {
                        "\n"
                    };

                    Some(format!("Answer in JSON using this {type_prefix}:{end}"))
                }
                TypeIR::RecursiveTypeAlias { .. } => {
                    let type_prefix = match &options.hoisted_class_prefix {
                        RenderSetting::Always(prefix) if !prefix.is_empty() => prefix,
                        _ => RenderOptions::DEFAULT_TYPE_PREFIX_IN_RENDER_MESSAGE,
                    };

                    Some(format!("Answer in JSON using this {type_prefix}: "))
                }
                TypeIR::List(_, _) => Some(String::from(
                    "Answer with a JSON Array using this schema:\n",
                )),
                TypeIR::Union(items, _) => match items.view() {
                    UnionTypeViewGeneric::Null => Some(String::from("Answer ONLY with null:\n")),
                    UnionTypeViewGeneric::Optional(_) => {
                        Some(String::from("Answer in JSON using this schema:\n"))
                    }
                    UnionTypeViewGeneric::OneOf(_) => {
                        Some(String::from("Answer in JSON using any of these schemas:\n"))
                    }
                    UnionTypeViewGeneric::OneOfOptional(_) => {
                        Some(String::from("Answer in JSON using any of these schemas:\n"))
                    }
                },
                TypeIR::Map(_, _, _) => Some(String::from("Answer in JSON using this schema:\n")),
                TypeIR::Tuple(_, _) => None,
                TypeIR::Arrow(_, _) => None, // TODO: Error? Arrow shouldn't appear here.
                TypeIR::Top(_) => panic!(
                    "TypeGeneric::Top should have been resolved by the compiler before code generation. \
                     This indicates a bug in the type resolution phase."
                ),
            }
        }

        match &options.prefix {
            RenderSetting::Always(prefix) => Some(prefix.to_owned()),
            RenderSetting::Never => None,
            RenderSetting::Auto => auto_prefix(&self.target, options, render_state, self),
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

    /// Renders either the schema or the name of a type.
    ///
    /// Prompt rendering is somewhat confusing because of hoisted types, so
    /// let's give a little explanation.
    ///
    /// The [`Self::inner_type_render`] function renders schemas only, say we
    /// have these classes:
    ///
    /// ```baml
    /// class Example {
    ///     a string
    ///     b string
    ///     c Nested
    /// }
    ///
    /// class Nested {
    ///     n int
    ///     m int
    /// }
    /// ```
    ///
    /// then [`Self::inner_type_render`] will return this string:
    ///
    /// ```ts
    /// {
    ///     a: string,
    ///     b: string,
    ///     c: {
    ///         n: int,
    ///         m: int,
    ///     },
    /// }
    /// ```
    ///
    /// Basically it renders all schemas recursively into one single schema.
    /// That becomes a problem when you define recursive classes, because
    /// there's no way to render them "inline" as above. Here's an example:
    ///
    /// ```baml
    /// class Node {
    ///     data int
    ///     next Node?
    /// }
    /// ```
    ///
    /// If we wanted to render this as above we'd stack overflow:
    ///
    /// ```ts
    /// {
    ///     data: int,
    ///     next: {
    ///         data: int,
    ///         next: {
    ///             data: int,
    ///             next: <<< STACK OVERFLOW >>>
    ///         },
    ///     },
    /// }
    /// ```
    ///
    /// So the solution is to hoist the class and use its name instead. This is
    /// how the complete prompt would look like:
    ///
    /// ```text
    /// Node {
    ///     data: int,
    ///     next: Node,
    /// }
    ///
    /// Answer in JSON using this schema: Node
    /// ```
    ///
    /// Obviously, we want to be able to embed recursive classes in other
    /// non-recursive classes, something like this:
    ///
    /// ```baml
    /// class Example {
    ///     a string
    ///     b string
    ///     c Nested
    ///     d LinkedList
    /// }
    /// ```
    ///
    /// Which requires this prompt:
    ///
    /// ```text
    /// Node {
    ///     data: int,
    ///     next: Node,
    /// }
    ///
    /// Answer in JSON using this schema:
    /// {
    ///     a: string,
    ///     b: string,
    ///     c: {
    ///         n: int,
    ///         m: int,
    ///     },
    ///     d: Node,
    /// }
    /// ```
    ///
    /// We need to render both schemas and names, which makes deciding when to
    /// "stop" recursion complicated. And that's what this function does, it
    /// saves us from writing if statements in every case where we might
    /// encounter a nested recursive type in [`Self::inner_type_render`].
    ///
    /// Users can also decide to hoist non-recursive classes for other reasons
    /// such as saving tokens or improve the adherence to the schema of the
    /// model response.
    ///
    /// Rule of thumb is, call [`Self::inner_type_render`] as an entry point
    /// and inside [`Self::inner_type_render`] call this function for each
    /// nested/inner type and let it handle the rest of recursion.
    fn render_possibly_hoisted_type(
        &self,
        options: &RenderOptions,
        field_type: &TypeIR,
        render_ctx: &RenderCtx,
    ) -> Result<String, minijinja::Error> {
        match field_type {
            TypeIR::Class {
                name: nested_class, ..
            } if render_ctx.hoisted_classes.contains(nested_class) => Ok(nested_class.to_owned()),

            _ => self.inner_type_render(options, field_type, render_ctx),
        }
    }

    /// This function is the entry point for recursive schema rendering.
    ///
    /// Read the documentation of [`Self::render_possibly_hoisted_type`] for
    /// more details.
    fn inner_type_render(
        &self,
        options: &RenderOptions,
        field: &TypeIR,
        render_ctx: &RenderCtx,
    ) -> Result<String, minijinja::Error> {
        Ok(match field {
            TypeIR::Primitive(t, _) => match t {
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
            TypeIR::Literal(v, _) => v.to_string(),
            TypeIR::Enum { name: e, .. } => {
                let Some(enm) = self.enums.get(e) else {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::BadSerialization,
                        format!("Enum {e} not found"),
                    ));
                };

                if render_ctx.hoisted_enums.contains(&enm.name.name) {
                    enm.name.rendered_name().to_string()
                } else {
                    enm.values
                        .iter()
                        .map(|(n, _)| format!("'{}'", n.rendered_name()))
                        .collect::<Vec<_>>()
                        .join(&options.or_splitter)
                }
            }
            TypeIR::Class {
                name: cls, mode, ..
            } => {
                let Some(class) = self.classes.get(&(cls.clone(), *mode)) else {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::BadSerialization,
                        format!("Class {cls} not found"),
                    ));
                };

                ClassRender {
                    name: class.name.rendered_name().to_string(),
                    description: class.description.clone(),
                    values: class
                        .fields
                        .iter()
                        .map(|(name, field_type, description, _streaming_needed)| {
                            Ok(ClassFieldRender {
                                name: name.rendered_name().to_string(),
                                description: description.clone(),
                                r#type: self.render_possibly_hoisted_type(
                                    options, field_type, render_ctx,
                                )?,
                            })
                        })
                        .collect::<Result<_, minijinja::Error>>()?,
                    quote_fields: options.quote_class_fields,
                }
                .to_string()
            }
            TypeIR::RecursiveTypeAlias { name, .. } => name.to_owned(),
            TypeIR::List(inner, _) => {
                let is_hoisted = match inner.as_ref() {
                    TypeIR::Class {
                        name: nested_class, ..
                    } => render_ctx.hoisted_classes.contains(nested_class),
                    TypeIR::RecursiveTypeAlias { name, .. } => {
                        self.structural_recursive_aliases.contains_key(name)
                    }
                    _ => false,
                };

                let inner_str = self.render_possibly_hoisted_type(options, inner, render_ctx)?;

                if !is_hoisted
                    && match inner.as_ref() {
                        TypeIR::Primitive(_, _) => false,
                        TypeIR::Enum { .. } => inner_str.len() > 15,
                        TypeIR::Union(items, _) => {
                            items.iter_include_null().iter().all(|t| !t.is_primitive())
                        }
                        _ => true,
                    }
                {
                    format!("[\n  {}\n]", inner_str.replace('\n', "\n  "))
                } else if matches!(inner.as_ref(), TypeIR::Union(_, _)) {
                    format!("({inner_str})[]")
                } else {
                    format!("{inner_str}[]")
                }
            }
            TypeIR::Union(items, _) => items
                .iter_include_null()
                .iter()
                .map(|t| self.render_possibly_hoisted_type(options, t, render_ctx))
                .collect::<Result<Vec<_>, minijinja::Error>>()?
                .join(&options.or_splitter),
            TypeIR::Tuple(_, _) => {
                return Err(minijinja::Error::new(
                    minijinja::ErrorKind::BadSerialization,
                    "Tuple type is not supported in outputs",
                ))
            }
            TypeIR::Map(key_type, value_type, _) => MapRender {
                style: &options.map_style,
                key_type: self.render_possibly_hoisted_type(options, key_type, render_ctx)?,
                value_type: self.render_possibly_hoisted_type(options, value_type, render_ctx)?,
            }
            .to_string(),
            TypeIR::Arrow(_, _) => {
                return Err(minijinja::Error::new(
                    minijinja::ErrorKind::BadSerialization,
                    "Arrow type is not supported in LLM function outputs",
                ))
            }
            TypeIR::Top(_) => panic!(
                "TypeIR::Top should have been resolved by the compiler before code generation. \
                 This indicates a bug in the type resolution phase."
            ),
        })
    }

    pub fn render(&self, options: RenderOptions) -> Result<Option<String>, minijinja::Error> {
        // Render context. Only contains hoisted types for now.
        let mut render_ctx = RenderCtx {
            hoisted_enums: IndexSet::new(),
            // Recursive classes are always hoisted so we start with those as base.
            // TODO: Figure out memory gymnastics to avoid this clone.
            hoisted_classes: self.recursive_classes.deref().clone(),
        };

        // Precompute hoisted enums.
        //
        // Original code had the "group_hoisted_literals" logic here but it
        // was always false, so not actually used. See this code:
        // https://github.com/BoundaryML/baml/blob/ee15d0f379f53a93f2d80b39909c74495b19930b/engine/baml-lib/jinja-runtime/src/output_format/types.rs#L480-L496
        for enm in self.enums.values() {
            if enm.values.len() > INLINE_RENDER_ENUM_MAX_VALUES
                || enm.values.iter().any(|(_, desc)| desc.is_some())
                || matches!(options.always_hoist_enums, RenderSetting::Always(true))
            //  || group_hoisted_literals
            {
                render_ctx.hoisted_enums.insert(enm.name.name.clone());
            }
        }

        // Now figure out what to hoist besides recursive classes.
        match &options.hoist_classes {
            // Nothing here, default behavior.
            HoistClasses::Auto => {}

            // Hoist all classes.
            HoistClasses::All => render_ctx
                .hoisted_classes
                .extend(self.classes.keys().map(|(name, _)| name.clone())),

            // Hoist only the specified subset.
            HoistClasses::Subset(classes) => {
                let mut not_found = IndexSet::new();

                for cls in classes {
                    if self
                        .classes
                        .contains_key(&(cls.clone(), baml_types::StreamingMode::NonStreaming))
                        || self
                            .classes
                            .contains_key(&(cls.clone(), baml_types::StreamingMode::Streaming))
                    {
                        render_ctx.hoisted_classes.insert(cls.to_owned());
                    } else {
                        not_found.insert(cls.to_owned());
                    }
                }

                // Error message if class/classes not found.
                if !not_found.is_empty() {
                    let (class_or_classes, it_does_or_they_do) = if not_found.len() == 1 {
                        ("class", "it does")
                    } else {
                        ("classes", "they do")
                    };

                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::BadSerialization,
                        format!(
                            "Cannot hoist {class_or_classes} {} because {it_does_or_they_do} not exist",
                            not_found
                                .iter()
                                .map(|cls| format!("\"{cls}\""))
                                .collect::<Vec<_>>()
                                .join(", "),
                        ),
                    ));
                }
            }
        };

        // Schema prefix (Answer in JSON using...)
        let prefix = self.prefix(&options, &render_ctx);

        let mut message = match &self.target {
            TypeIR::Primitive(TypeValue::String, _) if prefix.is_none() => None,
            TypeIR::Enum { name: e, .. } => {
                let Some(enm) = self.enums.get(e) else {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::BadSerialization,
                        format!("Enum {e} not found"),
                    ));
                };

                Some(self.enum_to_string(enm, &options))
            }
            _ => Some(self.inner_type_render(&options, &self.target, &render_ctx)?),
        };

        // Top level recursive classes will just use their name instead of the
        // entire schema which should already be hoisted.
        if let TypeIR::Class { name: class, .. } = &self.target {
            if render_ctx.hoisted_classes.contains(class) {
                message = Some(class.to_owned());
            }
        }

        // Top level hoisted enums will just use their name instead of the
        // entire schema which should already be hoisted.
        let target_is_hoisted_enum = if let TypeIR::Enum {
            name: enum_name, ..
        } = &self.target
        {
            render_ctx.hoisted_enums.contains(enum_name)
        } else {
            false
        };

        if target_is_hoisted_enum {
            message = None;
        }

        let mut class_definitions = Vec::new();
        let mut type_alias_definitions = Vec::new();

        for class_name in &render_ctx.hoisted_classes {
            let schema =
                self.inner_type_render(&options, &TypeIR::class(class_name), &render_ctx)?;

            // Extract description comments from the beginning of schema
            let (description_lines, schema_body) = if schema.starts_with("//") {
                let lines: Vec<&str> = schema.lines().collect();
                let mut desc_lines = Vec::new();
                let mut body_start = 0;

                for (i, line) in lines.iter().enumerate() {
                    if line.trim_start().starts_with("//") {
                        desc_lines.push(*line);
                        body_start = i + 1;
                    } else {
                        break;
                    }
                }

                let body = lines[body_start..].join("\n");
                (desc_lines, body)
            } else {
                (Vec::new(), schema)
            };

            let class_def = match &options.hoisted_class_prefix {
                RenderSetting::Always(prefix) if !prefix.is_empty() => {
                    format!("{prefix} {class_name} {schema_body}")
                }
                _ => format!("{class_name} {schema_body}"),
            };

            // Prepend description if present
            if !description_lines.is_empty() {
                class_definitions.push(format!("{}\n{}", description_lines.join("\n"), class_def));
            } else {
                class_definitions.push(class_def);
            }
        }

        for (alias, target) in self.structural_recursive_aliases.iter() {
            let recursive_pointer = self.inner_type_render(&options, target, &render_ctx)?;

            type_alias_definitions.push(match &options.hoisted_class_prefix {
                RenderSetting::Always(prefix) if !prefix.is_empty() => {
                    format!("{prefix} {alias} = {recursive_pointer}")
                }
                _ => format!("{alias} = {recursive_pointer}"),
            });
        }

        let enum_definitions = Vec::from_iter(render_ctx.hoisted_enums.into_iter().map(|e| {
            let enm = self.enums.get(&e).expect("Enum not found"); // TODO: Jinja Err
            let enum_str = self.enum_to_string(enm, &options);

            // If this is the target enum, prepend the prefix
            if target_is_hoisted_enum && e == enm.name.real_name() {
                if let Some(p) = &prefix {
                    format!("{p}{enum_str}")
                } else {
                    enum_str
                }
            } else {
                enum_str
            }
        }));

        let mut output = String::new();

        if !enum_definitions.is_empty() {
            output.push_str(&enum_definitions.join("\n\n"));
            // Only add double newline if target enum doesn't already include prefix
            if !target_is_hoisted_enum {
                output.push_str("\n\n");
            }
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
            // Only add prefix if it hasn't already been included in a hoisted target enum
            if !target_is_hoisted_enum {
                output.push_str(&p);
            }
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
        Self::target(TypeIR::List(Box::new(TypeIR::string()), Default::default())).build()
    }

    pub fn new_string() -> Self {
        Self::target(TypeIR::string()).build()
    }
}

impl OutputFormatContent {
    pub fn find_enum(&self, name: &str) -> Result<&Enum> {
        self.enums
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Enum {name} not found"))
    }

    pub fn find_class(&self, mode: &baml_types::StreamingMode, name: &str) -> Result<&Class> {
        self.classes
            .get(&(name.to_string(), *mode))
            .ok_or_else(|| anyhow::anyhow!("Class {name} not found"))
    }

    pub fn find_recursive_alias_target(&self, name: &str) -> Result<&TypeIR> {
        self.structural_recursive_aliases
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Recursive alias {name} not found"))
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use baml_types::ir_type::UnionConstructor;

    use super::*;

    #[test]
    fn render_string() {
        let content = OutputFormatContent::new_string();
        let rendered = content.render(RenderOptions::default()).unwrap();
        assert_eq!(rendered, None);
    }

    #[test]
    fn render_int() {
        let content = OutputFormatContent::target(TypeIR::int()).build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        assert_eq!(rendered, Some("Answer as an int".into()));
    }

    #[test]
    fn render_float() {
        let content = OutputFormatContent::target(TypeIR::float()).build();
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

        let content = OutputFormatContent::target(TypeIR::r#enum("Color"))
            .enums(enums)
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                "Answer with any of the categories:
Color
----
- Red
- Green
- Blue"
            ))
        );
    }

    #[test]
    fn render_class() {
        let classes = vec![Class {
            name: Name::new("Person".to_string()),
            description: None,
            namespace: baml_types::StreamingMode::NonStreaming,
            fields: vec![
                (
                    Name::new("name".to_string()),
                    TypeIR::string(),
                    Some("The person's name".to_string()),
                    false,
                ),
                (
                    Name::new("age".to_string()),
                    TypeIR::int(),
                    Some("The person's age".to_string()),
                    false,
                ),
            ],
            constraints: Vec::new(),
            streaming_behavior: Default::default(),
        }];

        let content = OutputFormatContent::target(TypeIR::class("Person"))
            .classes(classes)
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Answer in JSON using this schema:
{
  // The person's name
  name: string,
  // The person's age
  age: int,
}"#
            ))
        );
    }

    #[test]
    fn render_class_with_multiline_descriptions() {
        let classes = vec![Class {
            name: Name::new("Education".to_string()),
            description: None,
            namespace: baml_types::StreamingMode::NonStreaming,
            fields: vec![
                (
                    Name::new("school".to_string()),
                    TypeIR::optional(TypeIR::string()),
                    Some("111\n  ".to_string()),
                    false,
                ),
                (
                    Name::new("degree".to_string()),
                    TypeIR::string(),
                    Some("2222222".to_string()),
                    false,
                ),
                (Name::new("year".to_string()), TypeIR::int(), None, false),
            ],
            constraints: Vec::new(),
            streaming_behavior: Default::default(),
        }];

        let content = OutputFormatContent::target(TypeIR::class("Education"))
            .classes(classes)
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Answer in JSON using this schema:
{
  // 111
  //   
  school: string or null,
  // 2222222
  degree: string,
  year: int,
}"#
            ))
        );
    }

    #[test]
    fn test_class_with_block_description() {
        let classes = vec![Class {
            name: Name::new("User".to_string()),
            description: Some("Represents a system user".to_string()),
            namespace: baml_types::StreamingMode::NonStreaming,
            fields: vec![(Name::new("name".to_string()), TypeIR::string(), None, false)],
            constraints: Vec::new(),
            streaming_behavior: Default::default(),
        }];

        let content = OutputFormatContent::target(TypeIR::class("User"))
            .classes(classes)
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();

        assert_eq!(
            rendered,
            Some(String::from(
                "Answer in JSON using this schema:\n{\n  // Represents a system user\n\n  name: string,\n}"
            ))
        );
    }

    #[test]
    fn test_hoisted_class_with_description() {
        let classes = vec![Class {
            name: Name::new("Node".to_string()),
            description: Some("A node in a linked list".to_string()),
            namespace: baml_types::StreamingMode::NonStreaming,
            fields: vec![
                (Name::new("value".to_string()), TypeIR::int(), None, false),
                (
                    Name::new("next".to_string()),
                    TypeIR::optional(TypeIR::class("Node")),
                    None,
                    false,
                ),
            ],
            constraints: Vec::new(),
            streaming_behavior: Default::default(),
        }];

        let content = OutputFormatContent::target(TypeIR::class("Node"))
            .classes(classes)
            .recursive_classes(IndexSet::from_iter(["Node".to_string()]))
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();

        assert_eq!(
            rendered,
            Some(String::from(
                "Node {\n  // A node in a linked list\n\n  value: int,\n  next: Node or null,\n}\n\nAnswer in JSON using this schema: Node"
            ))
        );
    }

    #[test]
    fn test_class_with_multiline_description() {
        let classes = vec![Class {
            name: Name::new("Resume".to_string()),
            description: Some(
                "A professional resume\ncontaining work history\nand qualifications".to_string(),
            ),
            namespace: baml_types::StreamingMode::NonStreaming,
            fields: vec![(Name::new("name".to_string()), TypeIR::string(), None, false)],
            constraints: Vec::new(),
            streaming_behavior: Default::default(),
        }];

        let content = OutputFormatContent::target(TypeIR::class("Resume"))
            .classes(classes)
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();

        assert_eq!(
            rendered,
            Some(String::from(
                "Answer in JSON using this schema:\n{\n  // A professional resume\n  // containing work history\n  // and qualifications\n\n  name: string,\n}"
            ))
        );
    }

    #[test]
    fn render_class_with_quoted_fields() {
        let classes = vec![Class {
            name: Name::new("Person".to_string()),
            description: None,
            namespace: baml_types::StreamingMode::NonStreaming,
            fields: vec![
                (
                    Name::new("name".to_string()),
                    TypeIR::string(),
                    Some("The person's name".to_string()),
                    false,
                ),
                (
                    Name::new("age".to_string()),
                    TypeIR::int(),
                    Some("The person's age".to_string()),
                    false,
                ),
            ],
            constraints: Vec::new(),
            streaming_behavior: Default::default(),
        }];

        let content = OutputFormatContent::target(TypeIR::class("Person"))
            .classes(classes)
            .build();
        let rendered = content
            .render(RenderOptions {
                quote_class_fields: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Answer in JSON using this schema:
{
  // The person's name
  "name": string,
  // The person's age
  "age": int,
}"#
            ))
        );
    }

    #[test]
    fn hoist_enum_if_more_than_max_values() {
        let enums = vec![Enum {
            name: Name::new("Enm".to_string()),
            values: vec![
                (Name::new("A".to_string()), None),
                (Name::new("B".to_string()), None),
                (Name::new("C".to_string()), None),
                (Name::new("D".to_string()), None),
                (Name::new("E".to_string()), None),
                (Name::new("F".to_string()), None),
                (Name::new("G".to_string()), None),
            ],
            constraints: Vec::new(),
        }];

        let classes = vec![Class {
            name: Name::new("Output".to_string()),
            description: None,
            namespace: baml_types::StreamingMode::NonStreaming,
            fields: vec![(
                Name::new("output".to_string()),
                TypeIR::r#enum("Enm"),
                None,
                false,
            )],
            constraints: Vec::new(),
            streaming_behavior: Default::default(),
        }];

        let content = OutputFormatContent::target(TypeIR::class("Output"))
            .enums(enums)
            .classes(classes)
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Enm
----
- A
- B
- C
- D
- E
- F
- G

Answer in JSON using this schema:
{
  output: Enm,
}"#
            ))
        );
    }

    #[test]
    fn hoist_enum_if_variant_has_description() {
        let enums = vec![Enum {
            name: Name::new("Enm".to_string()),
            values: vec![
                (
                    Name::new("A".to_string()),
                    Some("A description".to_string()),
                ),
                (Name::new("B".to_string()), None),
                (Name::new("C".to_string()), None),
                (Name::new("D".to_string()), None),
                (Name::new("E".to_string()), None),
                (Name::new("F".to_string()), None),
            ],
            constraints: Vec::new(),
        }];

        let classes = vec![Class {
            name: Name::new("Output".to_string()),
            description: None,
            namespace: baml_types::StreamingMode::NonStreaming,
            fields: vec![(
                Name::new("output".to_string()),
                TypeIR::r#enum("Enm"),
                None,
                false,
            )],
            constraints: Vec::new(),
            streaming_behavior: Default::default(),
        }];

        let content = OutputFormatContent::target(TypeIR::class("Output"))
            .enums(enums)
            .classes(classes)
            .build();
        let rendered = content.render(RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Enm
----
- A: A description
- B
- C
- D
- E
- F

Answer in JSON using this schema:
{
  output: Enm,
}"#
            ))
        );
    }

    #[test]
    fn hoist_enum_if_setting_always_hoist_enum() {
        let enums = vec![Enum {
            name: Name::new("Enm".to_string()),
            values: vec![
                (Name::new("A".to_string()), None),
                (Name::new("B".to_string()), None),
                (Name::new("C".to_string()), None),
                (Name::new("D".to_string()), None),
                (Name::new("E".to_string()), None),
                (Name::new("F".to_string()), None),
            ],
            constraints: Vec::new(),
        }];

        let classes = vec![Class {
            name: Name::new("Output".to_string()),
            description: None,
            namespace: baml_types::StreamingMode::NonStreaming,
            fields: vec![(
                Name::new("output".to_string()),
                TypeIR::r#enum("Enm"),
                None,
                false,
            )],
            constraints: Vec::new(),
            streaming_behavior: Default::default(),
        }];

        let content = OutputFormatContent::target(TypeIR::class("Output"))
            .enums(enums)
            .classes(classes)
            .build();
        let rendered = content
            .render(RenderOptions {
                always_hoist_enums: RenderSetting::Always(true),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Enm
----
- A
- B
- C
- D
- E
- F

Answer in JSON using this schema:
{
  output: Enm,
}"#
            ))
        );
    }

    #[test]
    fn render_top_level_union() {
        let classes = vec![
            Class {
                name: Name::new("Bug".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (
                        Name::new("description".to_string()),
                        TypeIR::string(),
                        None,
                        false,
                    ),
                    (
                        Name::new("severity".to_string()),
                        TypeIR::string(),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Enhancement".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (
                        Name::new("title".to_string()),
                        TypeIR::string(),
                        None,
                        false,
                    ),
                    (
                        Name::new("description".to_string()),
                        TypeIR::string(),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Documentation".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (
                        Name::new("module".to_string()),
                        TypeIR::string(),
                        None,
                        false,
                    ),
                    (
                        Name::new("format".to_string()),
                        TypeIR::string(),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::union(vec![
            TypeIR::class("Bug"),
            TypeIR::class("Enhancement"),
            TypeIR::class("Documentation"),
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
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (
                        Name::new("category".to_string()),
                        TypeIR::union(vec![
                            TypeIR::class("Bug"),
                            TypeIR::class("Enhancement"),
                            TypeIR::class("Documentation"),
                        ]),
                        None,
                        false,
                    ),
                    (Name::new("date".to_string()), TypeIR::string(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Bug".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (
                        Name::new("description".to_string()),
                        TypeIR::string(),
                        None,
                        false,
                    ),
                    (
                        Name::new("severity".to_string()),
                        TypeIR::string(),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Enhancement".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (
                        Name::new("title".to_string()),
                        TypeIR::string(),
                        None,
                        false,
                    ),
                    (
                        Name::new("description".to_string()),
                        TypeIR::string(),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Documentation".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (
                        Name::new("module".to_string()),
                        TypeIR::string(),
                        None,
                        false,
                    ),
                    (
                        Name::new("format".to_string()),
                        TypeIR::string(),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::class("Issue"))
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
            description: None,
            namespace: baml_types::StreamingMode::NonStreaming,
            fields: vec![
                (Name::new("data".to_string()), TypeIR::int(), None, false),
                (
                    Name::new("next".to_string()),
                    TypeIR::optional(TypeIR::class("Node")),
                    None,
                    false,
                ),
            ],
            constraints: Vec::new(),
            streaming_behavior: Default::default(),
        }];

        let content = OutputFormatContent::target(TypeIR::class("Node"))
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
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        TypeIR::optional(TypeIR::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("LinkedList".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (
                        Name::new("head".to_string()),
                        TypeIR::optional(TypeIR::class("Node")),
                        None,
                        false,
                    ),
                    (Name::new("len".to_string()), TypeIR::int(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::class("LinkedList"))
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
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(
                    Name::new("pointer".to_string()),
                    TypeIR::class("B"),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("B".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(
                    Name::new("pointer".to_string()),
                    TypeIR::class("C"),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("C".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(
                    Name::new("pointer".to_string()),
                    TypeIR::optional(TypeIR::class("A")),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::class("A"))
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
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(
                    Name::new("pointer".to_string()),
                    TypeIR::class("B"),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("B".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(
                    Name::new("pointer".to_string()),
                    TypeIR::class("C"),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("C".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(
                    Name::new("pointer".to_string()),
                    TypeIR::optional(TypeIR::class("A")),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("NonRecursive".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (
                        Name::new("pointer".to_string()),
                        TypeIR::class("A"),
                        None,
                        false,
                    ),
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (Name::new("field".to_string()), TypeIR::bool(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::class("NonRecursive"))
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
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (
                        Name::new("pointer".to_string()),
                        TypeIR::class("B"),
                        None,
                        false,
                    ),
                    (
                        Name::new("nested".to_string()),
                        TypeIR::class("Nested"),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("B".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(
                    Name::new("pointer".to_string()),
                    TypeIR::class("C"),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("C".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(
                    Name::new("pointer".to_string()),
                    TypeIR::optional(TypeIR::class("A")),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("NonRecursive".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (
                        Name::new("pointer".to_string()),
                        TypeIR::class("A"),
                        None,
                        false,
                    ),
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (Name::new("field".to_string()), TypeIR::bool(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Nested".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (Name::new("field".to_string()), TypeIR::bool(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::class("NonRecursive"))
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
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("children".to_string()),
                        TypeIR::class("Forest"),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Forest".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(
                    Name::new("trees".to_string()),
                    TypeIR::list(TypeIR::class("Tree")),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::class("Tree"))
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
            description: None,
            namespace: baml_types::StreamingMode::NonStreaming,
            fields: vec![(
                Name::new("recursion".to_string()),
                TypeIR::union(vec![
                    TypeIR::int(),
                    TypeIR::string(),
                    TypeIR::optional(TypeIR::class("SelfReferential")),
                ]),
                None,
                false,
            )],
            constraints: Vec::new(),
            streaming_behavior: Default::default(),
        }];

        let content = OutputFormatContent::target(TypeIR::class("SelfReferential"))
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
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        TypeIR::optional(TypeIR::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Tree".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("children".to_string()),
                        TypeIR::list(TypeIR::class("Tree")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::union(vec![
            TypeIR::class("Node"),
            TypeIR::class("Tree"),
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
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (
                        Name::new("data_type".to_string()),
                        TypeIR::union(vec![TypeIR::class("Node"), TypeIR::class("Tree")]),
                        None,
                        false,
                    ),
                    (Name::new("len".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("description".to_string()),
                        TypeIR::string(),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Node".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        TypeIR::optional(TypeIR::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Tree".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("children".to_string()),
                        TypeIR::list(TypeIR::class("Tree")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::class("DataType"))
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
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        TypeIR::optional(TypeIR::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Tree".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("children".to_string()),
                        TypeIR::list(TypeIR::class("Tree")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("NonRecursive".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (Name::new("tag".to_string()), TypeIR::string(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::union(vec![
            TypeIR::class("Node"),
            TypeIR::class("Tree"),
            TypeIR::class("NonRecursive"),
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
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (
                        Name::new("data_type".to_string()),
                        TypeIR::union(vec![
                            TypeIR::class("Node"),
                            TypeIR::class("Tree"),
                            TypeIR::class("NonRecursive"),
                        ]),
                        None,
                        false,
                    ),
                    (Name::new("len".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("description".to_string()),
                        TypeIR::string(),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Node".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        TypeIR::optional(TypeIR::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Tree".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("children".to_string()),
                        TypeIR::list(TypeIR::class("Tree")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("NonRecursive".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (Name::new("tag".to_string()), TypeIR::string(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::class("DataType"))
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
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(
                    Name::new("pointer".to_string()),
                    TypeIR::class("B"),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("B".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(
                    Name::new("pointer".to_string()),
                    TypeIR::class("C"),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("C".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(
                    Name::new("pointer".to_string()),
                    TypeIR::optional(TypeIR::class("A")),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("NonRecursive".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (
                        Name::new("pointer".to_string()),
                        TypeIR::class("A"),
                        None,
                        false,
                    ),
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (Name::new("field".to_string()), TypeIR::bool(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::class("NonRecursive"))
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
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        TypeIR::optional(TypeIR::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Tree".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("children".to_string()),
                        TypeIR::list(TypeIR::class("Tree")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::union(vec![
            TypeIR::union(vec![TypeIR::class("Node"), TypeIR::int()]),
            TypeIR::union(vec![TypeIR::string(), TypeIR::class("Tree")]),
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
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        TypeIR::optional(TypeIR::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Tree".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("children".to_string()),
                        TypeIR::list(TypeIR::class("Tree")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("NonRecursive".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (
                        Name::new("the_union".to_string()),
                        TypeIR::union(vec![
                            TypeIR::union(vec![TypeIR::class("Node"), TypeIR::int()]),
                            TypeIR::union(vec![TypeIR::string(), TypeIR::class("Tree")]),
                        ]),
                        None,
                        false,
                    ),
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (Name::new("field".to_string()), TypeIR::bool(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::class("NonRecursive"))
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
            description: None,
            namespace: baml_types::StreamingMode::NonStreaming,
            fields: vec![
                (Name::new("data".to_string()), TypeIR::int(), None, false),
                (
                    Name::new("next".to_string()),
                    TypeIR::optional(TypeIR::class("Node")),
                    None,
                    false,
                ),
            ],
            constraints: Vec::new(),
            streaming_behavior: Default::default(),
        }];

        let content = OutputFormatContent::target(TypeIR::list(TypeIR::class("Node")))
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
            description: None,
            namespace: baml_types::StreamingMode::NonStreaming,
            fields: vec![(
                Name::new("data".to_string()),
                TypeIR::map(TypeIR::string(), TypeIR::class("RecursiveMap")),
                None,
                false,
            )],
            constraints: Vec::new(),
            streaming_behavior: Default::default(),
        }];

        let content = OutputFormatContent::target(TypeIR::class("RecursiveMap"))
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
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(
                    Name::new("data".to_string()),
                    TypeIR::map(TypeIR::string(), TypeIR::class("RecursiveMap")),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("NonRecursive".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(
                    Name::new("rec_map".to_string()),
                    TypeIR::class("RecursiveMap"),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::class("NonRecursive"))
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
            description: None,
            namespace: baml_types::StreamingMode::NonStreaming,
            fields: vec![
                (Name::new("data".to_string()), TypeIR::int(), None, false),
                (
                    Name::new("next".to_string()),
                    TypeIR::optional(TypeIR::class("Node")),
                    None,
                    false,
                ),
            ],
            constraints: Vec::new(),
            streaming_behavior: Default::default(),
        }];

        let content =
            OutputFormatContent::target(TypeIR::map(TypeIR::string(), TypeIR::class("Node")))
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
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(
                    Name::new("data".to_string()),
                    TypeIR::map(TypeIR::string(), TypeIR::class("Node")),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Node".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        TypeIR::optional(TypeIR::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::class("MapWithRecValue"))
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
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(
                    Name::new("data".to_string()),
                    TypeIR::map(TypeIR::string(), TypeIR::optional(TypeIR::class("Node"))),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Node".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        TypeIR::optional(TypeIR::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::class("MapWithRecValue"))
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
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        TypeIR::optional(TypeIR::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("NonRecursive".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (
                        Name::new("field".to_string()),
                        TypeIR::string(),
                        None,
                        false,
                    ),
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::map(
            TypeIR::string(),
            TypeIR::union(vec![
                TypeIR::class("Node"),
                TypeIR::int(),
                TypeIR::class("NonRecursive"),
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
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(
                    Name::new("data".to_string()),
                    TypeIR::map(
                        TypeIR::string(),
                        TypeIR::union(vec![
                            TypeIR::class("Node"),
                            TypeIR::int(),
                            TypeIR::class("NonRecursive"),
                        ]),
                    ),
                    None,
                    false,
                )],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Node".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                    (
                        Name::new("next".to_string()),
                        TypeIR::optional(TypeIR::class("Node")),
                        None,
                        false,
                    ),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("NonRecursive".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (
                        Name::new("field".to_string()),
                        TypeIR::string(),
                        None,
                        false,
                    ),
                    (Name::new("data".to_string()), TypeIR::int(), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::class("MapWithRecUnion"))
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
        let content =
            OutputFormatContent::target(TypeIR::recursive_type_alias("RecursiveMapAlias"))
                .structural_recursive_aliases(IndexMap::from([(
                    "RecursiveMapAlias".to_string(),
                    TypeIR::map(
                        TypeIR::string(),
                        TypeIR::recursive_type_alias("RecursiveMapAlias"),
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
        let content = OutputFormatContent::target(TypeIR::recursive_type_alias("A"))
            .structural_recursive_aliases(IndexMap::from([
                ("A".to_string(), TypeIR::recursive_type_alias("B")),
                ("B".to_string(), TypeIR::recursive_type_alias("C")),
                (
                    "C".to_string(),
                    TypeIR::list(TypeIR::recursive_type_alias("A")),
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
        let content = OutputFormatContent::target(TypeIR::recursive_type_alias("A"))
            .structural_recursive_aliases(IndexMap::from([
                ("A".to_string(), TypeIR::recursive_type_alias("B")),
                ("B".to_string(), TypeIR::recursive_type_alias("C")),
                (
                    "C".to_string(),
                    TypeIR::list(TypeIR::recursive_type_alias("A")),
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

    #[test]
    fn render_hoisted_classes_subset() {
        let classes = vec![
            Class {
                name: Name::new("A".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(Name::new("prop".to_string()), TypeIR::int(), None, false)],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("B".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(Name::new("prop".to_string()), TypeIR::string(), None, false)],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("C".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(Name::new("prop".to_string()), TypeIR::float(), None, false)],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Ret".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("a".to_string()), TypeIR::class("A"), None, false),
                    (Name::new("b".to_string()), TypeIR::class("B"), None, false),
                    (Name::new("c".to_string()), TypeIR::class("C"), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::class("Ret"))
            .classes(classes)
            .build();
        let rendered = content
            .render(RenderOptions::hoist_classes(HoistClasses::Subset(vec![
                "A".to_string(),
                "B".to_string(),
            ])))
            .unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"A {
  prop: int,
}

B {
  prop: string,
}

Answer in JSON using this schema:
{
  a: A,
  b: B,
  c: {
    prop: float,
  },
}"#
            ))
        );
    }

    #[test]
    fn render_hoist_all_classes() {
        let classes = vec![
            Class {
                name: Name::new("A".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(Name::new("prop".to_string()), TypeIR::int(), None, false)],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("B".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(Name::new("prop".to_string()), TypeIR::string(), None, false)],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("C".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![(Name::new("prop".to_string()), TypeIR::float(), None, false)],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
            Class {
                name: Name::new("Ret".to_string()),
                description: None,
                namespace: baml_types::StreamingMode::NonStreaming,
                fields: vec![
                    (Name::new("a".to_string()), TypeIR::class("A"), None, false),
                    (Name::new("b".to_string()), TypeIR::class("B"), None, false),
                    (Name::new("c".to_string()), TypeIR::class("C"), None, false),
                ],
                constraints: Vec::new(),
                streaming_behavior: Default::default(),
            },
        ];

        let content = OutputFormatContent::target(TypeIR::class("Ret"))
            .classes(classes)
            .build();
        let rendered = content
            .render(RenderOptions::hoist_classes(HoistClasses::All))
            .unwrap();
        #[rustfmt::skip]
        assert_eq!(
            rendered,
            Some(String::from(
r#"A {
  prop: int,
}

B {
  prop: string,
}

C {
  prop: float,
}

Ret {
  a: A,
  b: B,
  c: C,
}

Answer in JSON using this schema: Ret"#
            ))
        );
    }

    #[test]
    fn render_enum_with_descriptions() {
        // This test reproduces the bug where enums with descriptions
        // would be rendered twice - once as hoisted enum and once as target
        let enums = vec![Enum {
            name: Name::new("EnumOutput".to_string()),
            values: vec![
                (
                    Name::new("ONE".to_string()),
                    Some("The first enum.".to_string()),
                ),
                (
                    Name::new_with_alias("TWO".to_string(), Some("two".to_string())),
                    Some("The second enum.".to_string()),
                ),
                (
                    Name::new_with_alias("THREE".to_string(), Some("hi".to_string())),
                    Some("three".to_string()),
                ),
            ],
            constraints: Vec::new(),
        }];

        let content = OutputFormatContent::target(TypeIR::r#enum("EnumOutput"))
            .enums(enums)
            .build();

        // Use null prefix to avoid any additional text
        let options = RenderOptions::new(
            Some(None), // prefix = null
            None,       // or_splitter
            None,       // enum_value_prefix
            None,       // always_hoist_enums
            None,       // map_style
            None,       // hoisted_class_prefix
            None,       // hoist_classes
            None,       // quote_class_fields
        );

        let rendered = content.render(options).unwrap().unwrap();

        // After the fix, it should appear once without any prefix since prefix=null:
        // EnumOutput\\n----\\n- ONE: The first enum.\\n- two: The second enum.\\n- hi: three

        let enum_definition_count = rendered.matches("EnumOutput\n----").count();
        assert_eq!(
            enum_definition_count, 1,
            "Enum definition should only appear once, but found: {enum_definition_count}"
        );

        // Verify the complete expected output (no prefix since it's set to null)
        assert_eq!(
            rendered,
            r"EnumOutput
----
- ONE: The first enum.
- two: The second enum.
- hi: three"
        );
    }

    #[test]
    fn render_enum_with_descriptions_default_prefix() {
        // This test verifies that when prefix is not set (uses default),
        // the default prefix appears before the hoisted enum definition
        let enums = vec![Enum {
            name: Name::new_with_alias("EnumOutput".to_string(), Some("VALUE_ENUM".to_string())),
            values: vec![
                (
                    Name::new("ONE".to_string()),
                    Some("The first enum.".to_string()),
                ),
                (
                    Name::new_with_alias("TWO".to_string(), Some("two".to_string())),
                    Some("The second enum.".to_string()),
                ),
                (
                    Name::new_with_alias("THREE".to_string(), Some("hi".to_string())),
                    Some("three".to_string()),
                ),
            ],
            constraints: Vec::new(),
        }];

        let content = OutputFormatContent::target(TypeIR::r#enum("EnumOutput"))
            .enums(enums)
            .build();

        // Use default options (prefix not explicitly set)
        let options = RenderOptions::default();
        let rendered = content.render(options).unwrap().unwrap();

        // Should have default prefix "Answer with any of the categories:" before enum
        let enum_definition_count = rendered.matches("VALUE_ENUM\n----").count();
        assert_eq!(
            enum_definition_count, 1,
            "Enum definition should only appear once, but found: {enum_definition_count}"
        );

        // Verify default prefix appears before enum
        assert!(
            rendered.contains(
                r"Answer with any of the categories:
VALUE_ENUM
----"
            ),
            "Default prefix should appear before enum definition, but got: {rendered}"
        );

        // Verify the complete expected output
        assert_eq!(
            rendered,
            r"Answer with any of the categories:
VALUE_ENUM
----
- ONE: The first enum.
- two: The second enum.
- hi: three"
        );
    }
}
