use std::fmt::Write as _;

use baml_base::Literal as LiteralValue;
use baml_type::RuntimeTy;
use indexmap::IndexMap;
use thiserror::Error;

/// Error type for output format rendering.
#[derive(Clone, Debug, Error)]
pub enum RenderError {
    #[error("Enum '{0}' not found")]
    EnumNotFound(String),
    #[error("Class '{0}' not found")]
    ClassNotFound(String),
    #[error("Type '{0}' is not supported in outputs")]
    UnsupportedType(String),
    #[error(
        "Non-regular recursive generic class '{class}' expands from '{ancestor}' to '{instantiation}'"
    )]
    NonRegularRecursiveGeneric {
        class: String,
        ancestor: String,
        instantiation: String,
    },
    #[error(
        "Classes '{first}' and '{second}' both render as '{rendered_name}' in the output schema"
    )]
    RenderedClassNameCollision {
        rendered_name: String,
        first: String,
        second: String,
    },
}

/// A value within an enum definition for output format rendering.
#[derive(Clone, Debug)]
pub struct EnumValue {
    pub name: String,
    pub alias: Option<String>,
    pub description: Option<String>,
}

/// An enum definition for output format rendering.
#[derive(Clone, Debug)]
pub struct Enum {
    pub name: String,
    pub alias: Option<String>,
    pub description: Option<String>,
    pub values: Vec<EnumValue>,
}

/// A field within a class definition for output format rendering.
#[derive(Clone, Debug)]
pub struct ClassField {
    pub name: String,
    pub alias: Option<String>,
    pub field_type: RuntimeTy,
    pub description: Option<String>,
}

/// A class definition for output format rendering.
#[derive(Clone, Debug)]
pub struct Class {
    pub name: String,
    pub alias: Option<String>,
    pub description: Option<String>,
    pub fields: Vec<ClassField>,
}

/// Content for rendering output format schemas.
#[derive(Clone, Debug)]
pub struct OutputFormatContent {
    pub enums: IndexMap<String, Enum>,
    pub classes: IndexMap<String, Class>,
    pub target: RuntimeTy,
    pub recursive_classes: indexmap::IndexSet<String>,
    /// Recursive type aliases: alias name → target type.
    pub recursive_type_aliases: IndexMap<String, RuntimeTy>,
    build_error: Option<RenderError>,
}

impl OutputFormatContent {
    /// Create a new `OutputFormatContent` with the given target type.
    pub fn new(target: RuntimeTy) -> Self {
        Self {
            enums: IndexMap::new(),
            classes: IndexMap::new(),
            target,
            recursive_classes: indexmap::IndexSet::new(),
            recursive_type_aliases: IndexMap::new(),
            build_error: None,
        }
    }

    /// Add an enum definition.
    #[must_use]
    pub fn with_enum(mut self, enm: Enum) -> Self {
        self.enums.insert(enm.name.clone(), enm);
        self
    }

    /// Add a class definition.
    #[must_use]
    pub fn with_class(mut self, cls: Class) -> Self {
        self.classes.insert(cls.name.clone(), cls);
        self
    }

    /// Find an enum by name.
    pub fn find_enum(&self, name: &str) -> Option<&Enum> {
        self.enums.get(name)
    }

    /// Find a class by name.
    pub fn find_class(&self, name: &str) -> Option<&Class> {
        self.classes.get(name)
    }

    /// Render the output format schema as a string.
    pub fn render(&self, options: &RenderOptions) -> Result<Option<String>, RenderError> {
        self.render_impl(options)
    }

    fn render_impl(&self, options: &RenderOptions) -> Result<Option<String>, RenderError> {
        if let Some(error) = &self.build_error {
            return Err(error.clone());
        }

        if matches!(options.prefix, RenderSetting::Auto) {
            if let Some(instruction) = media_output_instruction(&self.target, options) {
                return Ok(Some(instruction));
            }
        }

        // For string target with no explicit prefix, return None
        if matches!(self.target, RuntimeTy::String { .. })
            && matches!(options.prefix, RenderSetting::Auto)
        {
            return Ok(None);
        }

        // The `json` type alias is an opaque leaf from the LLM's perspective.
        // Regardless of rendering options, the only thing we ask the model to produce
        // is arbitrary JSON — no schema body, no prefix enumeration.
        if let RuntimeTy::TypeAlias(tn, _) = &self.target {
            if tn.display_name().as_str() == ::baml_base::qualified_name::BAML_JSON_JSON {
                return Ok(Some("Respond with valid JSON.".to_string()));
            }
        }

        // Compute which classes and enums to hoist
        let hoisted_classes = self.compute_hoisted_classes(options);
        let hoisted_enums = self.compute_hoisted_enums(options);
        self.validate_hoisted_class_names(&hoisted_classes)?;

        let prefix = self.get_prefix(options, &hoisted_classes);

        // For simple primitives (int, bigint, float, bool) with Auto prefix, the prefix IS the full message
        // But with explicit prefix, we need to append the type
        if matches!(
            self.target,
            RuntimeTy::Int { .. }
                | RuntimeTy::Bigint { .. }
                | RuntimeTy::Float { .. }
                | RuntimeTy::Bool { .. }
        ) && matches!(options.prefix, RenderSetting::Auto)
        {
            return Ok(prefix);
        }

        // Check if the target is a hoisted enum
        let target_is_hoisted_enum = if let RuntimeTy::Enum(tn, _) = &self.target {
            hoisted_enums.contains(tn.display_name().as_str())
        } else {
            false
        };

        // Render hoisted enum definitions
        let enum_definitions: Vec<String> = hoisted_enums
            .iter()
            .filter_map(|name| {
                let enm = self.find_enum(name)?;
                let enum_str = self.render_enum(enm, options);
                // If this is the target enum, prepend prefix
                if target_is_hoisted_enum
                    && enm_display_name(&self.target)
                        .as_ref()
                        .map(baml_type::Name::as_str)
                        == Some(name.as_str())
                {
                    match &prefix {
                        Some(p) => Some(format!("{p}{enum_str}")),
                        None => Some(enum_str),
                    }
                } else {
                    Some(enum_str)
                }
            })
            .collect();

        // Render hoisted class definitions
        let mut class_defs = Vec::new();
        for name in &hoisted_classes {
            if let Some(cls) = self.find_class(name) {
                let body = self.render_class_hoisted(
                    cls,
                    options,
                    &hoisted_classes,
                    &hoisted_enums,
                    true,
                )?;

                let hoisted_prefix = match &options.hoisted_class_prefix {
                    RenderSetting::Always(p) if !p.is_empty() => format!("{p} "),
                    _ => String::new(),
                };

                let display_name = rendered_hoisted_definition_name(name, cls);

                // Render class description above the name for hoisted classes
                let mut def = String::new();
                if let Some(ref desc) = cls.description {
                    let desc = desc.trim();
                    if !desc.is_empty() {
                        for line in desc.lines() {
                            let _ = writeln!(def, "/// {line}");
                        }
                    }
                }
                let _ = write!(def, "{hoisted_prefix}{display_name} {body}");

                class_defs.push(def);
            }
        }

        // Render hoisted type alias definitions
        let mut alias_defs = Vec::new();
        for (alias_name, target_ty) in &self.recursive_type_aliases {
            let target_str = self
                .render_type_hoisted(target_ty, options, &hoisted_classes, &hoisted_enums)?
                .unwrap_or_else(|| "unknown".to_string());

            let def = match &options.hoisted_class_prefix {
                RenderSetting::Always(p) if !p.is_empty() => {
                    format!("{p} {alias_name} = {target_str}")
                }
                _ => format!("{alias_name} = {target_str}"),
            };
            alias_defs.push(def);
        }

        // Render the target type with hoisting awareness
        let message = if let RuntimeTy::Class(tn, _, _) | RuntimeTy::Interface(tn, _, _, _) =
            &self.target
        {
            let tn_display_name = tn.display_name();
            let class_key = class_instantiation_key(&self.target);
            if hoisted_classes.contains(&class_key) {
                Some(rendered_hoisted_class_name(
                    &self.target,
                    self.find_class(&class_key)
                        .or_else(|| self.find_class(tn_display_name.as_str())),
                ))
            } else {
                self.render_type_hoisted(&self.target, options, &hoisted_classes, &hoisted_enums)?
            }
        } else if let RuntimeTy::Enum(tn, _) = &self.target {
            if target_is_hoisted_enum {
                // Hoisted target enum: rendered in enum_definitions block
                None
            } else if let Some(enm) = self.find_enum(tn.display_name().as_str()) {
                // Non-hoisted target enum: render full block format (not inline)
                Some(self.render_enum(enm, options))
            } else {
                Some(tn.display_name().to_string())
            }
        } else if let RuntimeTy::TypeAlias(fqn, _) = &self.target {
            Some(fqn.display_name().to_string())
        } else {
            self.render_type_hoisted(&self.target, options, &hoisted_classes, &hoisted_enums)?
        };

        // Assemble: enum defs + class defs + alias defs + prefix + target
        let mut output = String::new();

        if !enum_definitions.is_empty() {
            output.push_str(&enum_definitions.join("\n\n"));
            if !target_is_hoisted_enum {
                output.push_str("\n\n");
            }
        }

        if !class_defs.is_empty() {
            output.push_str(&class_defs.join("\n\n"));
            output.push_str("\n\n");
        }

        if !alias_defs.is_empty() {
            output.push_str(&alias_defs.join("\n"));
            output.push_str("\n\n");
        }

        if let Some(p) = &prefix {
            // Only add prefix if not already included in hoisted target enum
            if !target_is_hoisted_enum {
                output.push_str(p);
            }
        }
        if let Some(t) = &message {
            output.push_str(t);
        }

        // Trim trailing newlines
        while output.ends_with('\n') {
            output.pop();
        }

        if output.is_empty() {
            Ok(None)
        } else {
            Ok(Some(output))
        }
    }

    /// Max enum values before auto-hoisting (matches old engine).
    const INLINE_RENDER_ENUM_MAX_VALUES: usize = 6;

    /// Compute which enums should be hoisted (rendered as top-level definitions).
    fn compute_hoisted_enums(&self, options: &RenderOptions) -> indexmap::IndexSet<String> {
        let mut hoisted = indexmap::IndexSet::new();
        for (name, enm) in &self.enums {
            if enm.values.len() > Self::INLINE_RENDER_ENUM_MAX_VALUES
                || enm.description.is_some()
                || enm.values.iter().any(|v| v.description.is_some())
                || matches!(options.always_hoist_enums, RenderSetting::Always(true))
            {
                hoisted.insert(name.clone());
            }
        }
        hoisted
    }

    /// Compute which classes should be hoisted (rendered as top-level definitions).
    fn compute_hoisted_classes(&self, options: &RenderOptions) -> indexmap::IndexSet<String> {
        let mut hoisted = indexmap::IndexSet::new();

        // Recursive classes are always hoisted
        hoisted.extend(self.recursive_classes.iter().cloned());

        // Additional hoisting based on options
        match &options.hoist_classes {
            HoistClasses::All => {
                hoisted.extend(self.classes.keys().cloned());
            }
            HoistClasses::Subset(names) => {
                for requested_name in names {
                    hoisted.extend(
                        self.classes
                            .iter()
                            .filter(|(definition_key, cls)| {
                                *definition_key == requested_name || cls.name == *requested_name
                            })
                            .map(|(definition_key, _)| definition_key.clone()),
                    );
                }
            }
            HoistClasses::Auto => {
                // Only recursive classes (already added above)
            }
        }

        hoisted
    }

    fn validate_hoisted_class_names(
        &self,
        hoisted: &indexmap::IndexSet<String>,
    ) -> Result<(), RenderError> {
        let mut definitions_by_rendered_name = IndexMap::<String, String>::new();
        for definition_key in hoisted {
            let Some(cls) = self.find_class(definition_key) else {
                continue;
            };
            let rendered_name = rendered_hoisted_definition_name(definition_key, cls);
            if let Some(first) = definitions_by_rendered_name.get(&rendered_name) {
                if first != definition_key {
                    return Err(RenderError::RenderedClassNameCollision {
                        rendered_name,
                        first: first.clone(),
                        second: definition_key.clone(),
                    });
                }
            } else {
                definitions_by_rendered_name.insert(rendered_name, definition_key.clone());
            }
        }
        Ok(())
    }

    fn get_prefix(
        &self,
        options: &RenderOptions,
        hoisted: &indexmap::IndexSet<String>,
    ) -> Option<String> {
        match &options.prefix {
            RenderSetting::Always(p) => Some(p.clone()),
            RenderSetting::Never => None,
            RenderSetting::Auto => {
                let type_word = match &options.hoisted_class_prefix {
                    RenderSetting::Always(p) if !p.is_empty() => p.as_str(),
                    _ => "schema",
                };

                Self::auto_prefix(&self.target, type_word, hoisted)
            }
        }
    }

    /// The `Auto`-mode schema prefix for a given target type. A nullable union
    /// (`T?` == `T | null`) delegates to its non-null part — so `string?` has no
    /// prefix like `string`, and `Class?` uses the class prefix.
    fn auto_prefix(
        ty: &RuntimeTy,
        type_word: &str,
        hoisted: &indexmap::IndexSet<String>,
    ) -> Option<String> {
        match ty {
            RuntimeTy::String { .. } => None,
            RuntimeTy::Int { .. } => Some("Answer as an int".to_string()),
            RuntimeTy::Bigint { .. } => Some("Answer as a bigint".to_string()),
            RuntimeTy::Float { .. } => Some("Answer as a float".to_string()),
            RuntimeTy::Bool { .. } => Some("Answer as a bool".to_string()),
            RuntimeTy::List(..) => {
                Some("Answer with a JSON Array using this schema:\n".to_string())
            }
            RuntimeTy::Class(_, _, _) | RuntimeTy::Interface(_, _, _, _) => {
                let end = if class_is_hoisted(ty, hoisted) {
                    " "
                } else {
                    "\n"
                };
                Some(format!("Answer in JSON using this {type_word}:{end}"))
            }
            RuntimeTy::Map { .. } => Some(format!("Answer in JSON using this {type_word}:\n")),
            RuntimeTy::Enum(..) => Some("Answer with any of the categories:\n".to_string()),
            RuntimeTy::Union(variants, _) => {
                let non_null: Vec<&RuntimeTy> = variants
                    .iter()
                    .filter(|v| !matches!(v, RuntimeTy::Null { .. }))
                    .collect();
                // `T?` (single non-null member + null) follows the inner type's
                // prefix; a true multi-member union gets the union prefix.
                if non_null.len() == 1 && non_null.len() < variants.len() {
                    Self::auto_prefix(non_null[0], type_word, hoisted)
                } else if non_null.len() > 1 {
                    Some(format!("Answer in JSON using any of these {type_word}s:\n"))
                } else {
                    Some(format!("Answer in JSON using this {type_word}:\n"))
                }
            }
            RuntimeTy::TypeAlias(tn, _)
                if tn.display_name().as_str() == ::baml_base::qualified_name::BAML_JSON_JSON =>
            {
                None
            }
            RuntimeTy::TypeAlias(..) => Some(format!("Answer in JSON using this {type_word}: ")),
            RuntimeTy::Literal(..) => Some("Answer using this specific value:\n".to_string()),
            _ => None,
        }
    }

    /// Render a type, with hoisted classes rendered as just their name.
    fn render_type_hoisted(
        &self,
        ty: &RuntimeTy,
        options: &RenderOptions,
        hoisted_classes: &indexmap::IndexSet<String>,
        hoisted_enums: &indexmap::IndexSet<String>,
    ) -> Result<Option<String>, RenderError> {
        // Intercept hoisted classes: return just the (aliased) name
        if let RuntimeTy::Class(tn, _, _) | RuntimeTy::Interface(tn, _, _, _) = ty {
            let tn_display_name = tn.display_name();
            let class_key = class_instantiation_key(ty);
            if hoisted_classes.contains(&class_key) {
                return Ok(Some(rendered_hoisted_class_name(
                    ty,
                    self.find_class(&class_key)
                        .or_else(|| self.find_class(tn_display_name.as_str())),
                )));
            }
        }

        let or_splitter = match &options.or_splitter {
            RenderSetting::Always(s) => s.as_str(),
            RenderSetting::Auto | RenderSetting::Never => " or ",
        };

        match ty {
            RuntimeTy::String { .. } => Ok(Some("string".to_string())),
            RuntimeTy::Int { .. } => Ok(Some("int".to_string())),
            RuntimeTy::Bigint { .. } => Ok(Some("bigint".to_string())),
            RuntimeTy::Float { .. } => Ok(Some("float".to_string())),
            RuntimeTy::Bool { .. } => Ok(Some("bool".to_string())),
            RuntimeTy::Null { .. } => Ok(Some(rendered_null_type(options).to_string())),

            RuntimeTy::List(inner, _) => {
                let inner_str = self
                    .render_type_hoisted(inner, options, hoisted_classes, hoisted_enums)?
                    .unwrap_or_else(|| "unknown".to_string());

                // Determine if we need multiline rendering
                let is_hoisted = match inner.as_ref() {
                    RuntimeTy::Class(..) | RuntimeTy::Interface(..) => {
                        class_is_hoisted(inner, hoisted_classes)
                    }
                    RuntimeTy::TypeAlias(tn, _) => self
                        .recursive_type_aliases
                        .contains_key(tn.display_name().as_str()),
                    _ => false,
                };
                let needs_multiline = !is_hoisted
                    && match inner.as_ref() {
                        RuntimeTy::String { .. }
                        | RuntimeTy::Int { .. }
                        | RuntimeTy::Float { .. }
                        | RuntimeTy::Bool { .. }
                        | RuntimeTy::Null { .. } => false,
                        RuntimeTy::Enum(tn, _) => {
                            // Inline enums render short; hoisted ones are just a name
                            !hoisted_enums.contains(tn.display_name().as_str())
                                && inner_str.len() > 15
                        }
                        RuntimeTy::Union(items, _) => items.iter().all(|t| {
                            !matches!(
                                t,
                                RuntimeTy::String { .. }
                                    | RuntimeTy::Int { .. }
                                    | RuntimeTy::Float { .. }
                                    | RuntimeTy::Bool { .. }
                                    | RuntimeTy::Null { .. }
                            )
                        }),
                        _ => true,
                    };

                if needs_multiline {
                    Ok(Some(format!("[\n  {}\n]", inner_str.replace('\n', "\n  "))))
                } else if matches!(inner.as_ref(), RuntimeTy::Union(_, _)) {
                    Ok(Some(format!("({inner_str})[]")))
                } else {
                    Ok(Some(format!("{inner_str}[]")))
                }
            }

            RuntimeTy::Map { key, value, .. } => {
                let key_str = self
                    .render_type_hoisted(key, options, hoisted_classes, hoisted_enums)?
                    .unwrap_or_else(|| "string".to_string());
                let value_str = self
                    .render_type_hoisted(value, options, hoisted_classes, hoisted_enums)?
                    .unwrap_or_else(|| "unknown".to_string());
                match options.map_style {
                    MapStyle::TypeParameters => Ok(Some(format!("map<{key_str}, {value_str}>"))),
                    MapStyle::ObjectLiteral => {
                        Ok(Some(format!("{{ \"<{key_str}>\": {value_str} }}")))
                    }
                }
            }

            RuntimeTy::Union(variants, _) => {
                let rendered: Vec<String> = variants
                    .iter()
                    .filter_map(|v| {
                        self.render_type_hoisted(v, options, hoisted_classes, hoisted_enums)
                            .ok()
                            .flatten()
                    })
                    .collect();
                Ok(Some(rendered.join(or_splitter)))
            }

            RuntimeTy::Enum(tn, _) => {
                let tn_display_name = tn.display_name();
                if hoisted_enums.contains(tn_display_name.as_str()) {
                    // Hoisted enum: render as just the display name
                    let enm = self.find_enum(tn_display_name.as_str());
                    let display_name = enm
                        .and_then(|e| e.alias.as_deref())
                        .unwrap_or(tn_display_name.as_str());
                    Ok(Some(display_name.to_string()))
                } else if let Some(enm) = self.find_enum(tn_display_name.as_str()) {
                    // Inline enum: render as 'val1' or 'val2' or 'val3'
                    let values: Vec<String> = enm
                        .values
                        .iter()
                        .map(|v| {
                            let name = rendered_name(&v.name, v.alias.as_ref());
                            format!("'{name}'")
                        })
                        .collect();
                    Ok(Some(values.join(or_splitter)))
                } else {
                    Ok(Some(tn.display_name().to_string()))
                }
            }

            RuntimeTy::Class(tn, _, _) | RuntimeTy::Interface(tn, _, _, _) => {
                let class_key = class_instantiation_key(ty);
                if let Some(cls) = self
                    .find_class(&class_key)
                    .or_else(|| self.find_class(tn.display_name().as_str()))
                {
                    Ok(Some(self.render_class_hoisted(
                        cls,
                        options,
                        hoisted_classes,
                        hoisted_enums,
                        false,
                    )?))
                } else {
                    Ok(Some(class_instantiation_key(ty)))
                }
            }

            RuntimeTy::Uint8Array { .. } => {
                Err(RenderError::UnsupportedType("uint8array".to_string()))
            }
            RuntimeTy::Media(kind, _) => Ok(Some(kind.to_string())),

            RuntimeTy::Literal(lit, _, _) => Ok(Some(render_literal(lit))),

            // Opaque leaf types have no JSON output-format schema. They surface
            // as `UnsupportedType` named the same way `RuntimeTy`'s `Display` renders
            // them (`reflect.Type`, or the fixed qualified name).
            RuntimeTy::Type { .. } => {
                Err(RenderError::UnsupportedType("reflect.Type".to_string()))
            }
            RuntimeTy::Resource { .. } => {
                Err(RenderError::UnsupportedType("ai.Resource".to_string()))
            }
            RuntimeTy::PromptAst { .. } => {
                Err(RenderError::UnsupportedType("ai.Prompt".to_string()))
            }

            RuntimeTy::TypeAlias(fqn, _) => {
                // Recursive type aliases render as just their display name
                Ok(Some(fqn.display_name().to_string()))
            }

            RuntimeTy::Function { .. }
            | RuntimeTy::Void { .. }
            | RuntimeTy::BuiltinUnknown { .. }
            | RuntimeTy::EnumVariant(..)
            | RuntimeTy::Future(..)
            | RuntimeTy::TypeVar(..)
            | RuntimeTy::AssociatedTypeProjection { .. }
            | RuntimeTy::Never { .. }
            // Checked LLM execution and render-companion paths reject these at
            // `validate_output_type`. Throws-never low-level output-format
            // helpers may still degrade this error to an empty string, so keep
            // the formatter fallible rather than aborting the process.
            | RuntimeTy::RustType { .. } => Err(RenderError::UnsupportedType(ty.to_string())),
        }
    }

    #[allow(clippy::unused_self)]
    fn render_enum(&self, enm: &Enum, options: &RenderOptions) -> String {
        use std::fmt::Write;

        let display_name = rendered_name(&enm.name, enm.alias.as_ref());

        let mut result = String::new();
        // Enum-level description as /// comments above the name
        if let Some(ref d) = enm.description {
            let d = d.trim();
            if !d.is_empty() {
                for line in d.lines() {
                    let _ = writeln!(result, "/// {line}");
                }
            }
        }

        // Header: "EnumName\n----"
        let _ = write!(result, "{display_name}\n----");

        // Values with prefix (default "- ")
        for v in &enm.values {
            let value_name = rendered_name(&v.name, v.alias.as_ref());
            let prefix = match &options.enum_value_prefix {
                RenderSetting::Auto => "- ",
                RenderSetting::Always(p) => p.as_str(),
                RenderSetting::Never => "",
            };
            let line = match &v.description {
                Some(d) => format!("{prefix}{value_name}: {d}"),
                None => format!("{prefix}{value_name}"),
            };
            result.push('\n');
            result.push_str(&line);
        }

        result
    }

    /// Render a class body, with hoisted classes/enums rendered as just their name in field types.
    /// When `skip_class_description` is true, the class-level description is omitted from the body
    /// (used for hoisted classes where the description is rendered above the name line).
    fn render_class_hoisted(
        &self,
        cls: &Class,
        options: &RenderOptions,
        hoisted_classes: &indexmap::IndexSet<String>,
        hoisted_enums: &indexmap::IndexSet<String>,
        skip_class_description: bool,
    ) -> Result<String, RenderError> {
        use std::fmt::Write;

        let mut fields_str = Vec::new();

        for field in &cls.fields {
            let ty_str = self
                .render_type_hoisted(&field.field_type, options, hoisted_classes, hoisted_enums)?
                .unwrap_or_else(|| "unknown".to_string());
            // Re-indent multi-line type strings for proper nesting
            let ty_str = if ty_str.contains('\n') {
                ty_str.replace('\n', "\n  ")
            } else {
                ty_str
            };
            let display_name = rendered_name(&field.name, field.alias.as_ref());
            let quote_fields = matches!(options.quote_class_fields, RenderSetting::Always(true));
            let field_name = if quote_fields {
                format!("\"{display_name}\"")
            } else {
                display_name.to_string()
            };
            // Field description as /// comment above the field
            if let Some(d) = &field.description {
                fields_str.push(format!("  /// {}", d.replace('\n', "\n  /// ")));
            }
            fields_str.push(format!("  {field_name}: {ty_str},"));
        }

        let mut output = String::new();
        output.push_str("{\n");
        if !skip_class_description {
            if let Some(ref d) = cls.description {
                let d = d.trim();
                if !d.is_empty() {
                    for line in d.lines() {
                        let _ = writeln!(output, "  /// {line}");
                    }
                    output.push('\n');
                }
            }
        }
        output.push_str(&fields_str.join("\n"));
        output.push_str("\n}");

        Ok(output)
    }
}

/// Return alias if set, otherwise the real name.
fn rendered_name<'a>(name: &'a str, alias: Option<&'a String>) -> &'a str {
    alias.map(String::as_str).unwrap_or(name)
}

fn rendered_hoisted_definition_name(definition_key: &str, cls: &Class) -> String {
    let rendered_base = rendered_name(&cls.name, cls.alias.as_ref());
    match definition_key.strip_prefix(&cls.name) {
        Some("") => rendered_base.to_string(),
        Some(type_args) if type_args.starts_with('<') => {
            format!("{rendered_base}{type_args}")
        }
        _ => definition_key.to_string(),
    }
}

fn rendered_hoisted_class_name(ty: &RuntimeTy, class_def: Option<&Class>) -> String {
    let class_key = class_instantiation_key(ty);
    class_def.map_or(class_key.clone(), |cls| {
        rendered_hoisted_definition_name(&class_key, cls)
    })
}

fn class_is_hoisted(ty: &RuntimeTy, hoisted: &indexmap::IndexSet<String>) -> bool {
    hoisted.contains(&class_instantiation_key(ty))
}

/// Key a class definition by its realized generic instantiation. A generic
/// class's display name alone is insufficient: `Box<int>` and `Box<string>`
/// have different field schemas even though both are named `Box`.
fn class_instantiation_key(ty: &RuntimeTy) -> String {
    let (RuntimeTy::Class(type_name, type_args, _)
    | RuntimeTy::Interface(type_name, type_args, _, _)) = ty
    else {
        unreachable!("class_instantiation_key called for a non-class type")
    };
    if type_args.is_empty() {
        type_name.display_name().to_string()
    } else {
        format!(
            "{}<{}>",
            type_name.display_name(),
            type_args
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// Extract the display name from an enum target type.
fn enm_display_name(ty: &RuntimeTy) -> Option<baml_type::Name> {
    match ty {
        RuntimeTy::Enum(tn, _) => Some(tn.display_name()),
        _ => None,
    }
}

/// Render a literal value following engine's `LiteralValue::Display` convention.
fn render_literal(lit: &LiteralValue) -> String {
    match lit {
        LiteralValue::String(s) => format!("\"{s}\""),
        LiteralValue::Int(n) => n.to_string(),
        LiteralValue::Bigint(n) => format!("{n}n"),
        LiteralValue::Float(f) => f.clone(),
        LiteralValue::Bool(b) => b.to_string(),
    }
}

fn rendered_null_type(options: &RenderOptions) -> &str {
    match &options.render_null_as {
        RenderSetting::Always(value) => value.as_str(),
        RenderSetting::Auto | RenderSetting::Never => "null",
    }
}

fn media_output_instruction(target: &RuntimeTy, options: &RenderOptions) -> Option<String> {
    let null_type = rendered_null_type(options);
    match target {
        RuntimeTy::Media(kind, _) => Some(format!("Return an {kind} output.")),
        RuntimeTy::Union(variants, _) if nullable_media_union_kind(variants).is_some() => {
            let kind = nullable_media_union_kind(variants).expect("checked above");
            Some(format!("Return an {kind} output or {null_type}."))
        }
        RuntimeTy::List(inner, _) => match inner.as_ref() {
            RuntimeTy::Media(kind, _) => Some(format!("Return one or more {kind} outputs.")),
            inner if is_text_or_image_union(inner) => {
                Some("Return an ordered sequence of text and image outputs.".to_string())
            }
            _ => None,
        },
        target if is_text_or_image_union(target) => {
            Some("Return either text or an image output.".to_string())
        }
        _ => None,
    }
}

fn nullable_media_union_kind(variants: &[RuntimeTy]) -> Option<baml_base::MediaKind> {
    let mut kind = None;
    let mut has_null = false;
    for variant in variants {
        match variant {
            RuntimeTy::Media(media_kind, _) => {
                if kind
                    .replace(*media_kind)
                    .is_some_and(|prev| prev != *media_kind)
                {
                    return None;
                }
            }
            RuntimeTy::Null { .. } => has_null = true,
            _ => return None,
        }
    }

    if has_null { kind } else { None }
}

pub(crate) fn is_text_or_image_union(target: &RuntimeTy) -> bool {
    let RuntimeTy::Union(variants, _) = target else {
        return false;
    };

    let mut has_string = false;
    let mut has_image = false;
    for variant in variants {
        match variant {
            RuntimeTy::String { .. } => has_string = true,
            RuntimeTy::Media(baml_base::MediaKind::Image, _) => has_image = true,
            RuntimeTy::Null { .. } => {}
            _ => return false,
        }
    }

    has_string && has_image
}

/// Tri-state setting: Auto (default behavior), Always(value), or Never.
/// Ported from engine/baml-lib/jinja-runtime/src/output_format/types.rs:193-199
#[derive(Clone, Debug, Default)]
pub enum RenderSetting<T> {
    #[default]
    Auto,
    Always(T),
    Never,
}

/// Map rendering style.
/// Ported from engine/baml-lib/jinja-runtime/src/output_format/types.rs:201-208
#[derive(Clone, Debug, Default)]
pub enum MapStyle {
    /// Render as `map<K, V>` (angle bracket style). Opt-in escape hatch: the
    /// raw BAML type gives the model no example of the JSON object it must emit.
    TypeParameters,
    /// Render as `{ "<K>": V }` (JSON object shape). Default: the model answers
    /// with JSON, so the hint mirrors the object it needs to produce (consistent
    /// with how classes and lists render) instead of leaking BAML type syntax.
    #[default]
    ObjectLiteral,
}

/// Hoist classes setting.
/// Ported from engine/baml-lib/jinja-runtime/src/output_format/types.rs:213-221
#[derive(Clone, Debug, Default)]
pub enum HoistClasses {
    /// Hoist all classes.
    All,
    /// Hoist only the specified subset.
    Subset(Vec<String>),
    /// Default behavior: hoist only recursive classes.
    #[default]
    Auto,
}

/// Options for rendering output format.
/// Ported from engine/baml-lib/jinja-runtime/src/output_format/types.rs:226-235
#[derive(Clone, Debug)]
pub struct RenderOptions {
    /// Prefix for the output format (e.g., "Answer in JSON using this schema:")
    pub prefix: RenderSetting<String>,
    /// Separator for union/or types (default: " or ")
    pub or_splitter: RenderSetting<String>,
    /// Prefix for enum values
    pub enum_value_prefix: RenderSetting<String>,
    /// Prefix for hoisted class definitions
    pub hoisted_class_prefix: RenderSetting<String>,
    /// Which classes to hoist
    pub hoist_classes: HoistClasses,
    /// Whether to always hoist enums
    pub always_hoist_enums: RenderSetting<bool>,
    /// Map rendering style
    pub map_style: MapStyle,
    /// Whether to quote class field names
    pub quote_class_fields: RenderSetting<bool>,
    /// String to use when rendering the `null` type.
    pub render_null_as: RenderSetting<String>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            prefix: RenderSetting::Auto,
            or_splitter: RenderSetting::Auto,
            enum_value_prefix: RenderSetting::Auto,
            hoisted_class_prefix: RenderSetting::Auto,
            hoist_classes: HoistClasses::Auto,
            always_hoist_enums: RenderSetting::Auto,
            map_style: MapStyle::ObjectLiteral,
            quote_class_fields: RenderSetting::Auto,
            render_null_as: RenderSetting::Auto,
        }
    }
}

impl RenderOptions {
    pub fn new() -> Self {
        Self::default()
    }
}

// ============================================================================
// Clean (owned-type) entry points for trait-based dispatch
// ============================================================================

/// Render `return_type`'s schema with default options for
/// `ctx.output_format`. Build the `OutputFormatContent`, then render with
/// `RenderOptions::default()`. An empty or `None`
/// render (e.g. a primitive return type with no schema) becomes the empty string.
pub fn render_output_format(
    return_type: &baml_type::RuntimeTy,
    ctx: &::sys_types::SysOpContext,
) -> String {
    build_output_format_content(return_type, ctx)
        .render(&self::RenderOptions::default())
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Render a prebuilt [`OutputFormatContent`] with caller-supplied options,
/// preserving any deferred build or render error for the sys-op boundary.
/// This backs the parameterized `ctx.output_format_with(...)` accessor. The content is
/// carried as an opaque handle on `Context` (built once by
/// [`build_output_format_content`]); this only re-renders. The option mapping
/// (`RenderSetting`/`RenderOptions`) stays crate-internal: a value overrides
/// that aspect (`Always`), `None` keeps the default (`Auto`).
// Options arrive by value from the sys-op glue; most are moved into
// `RenderOptions`, `map_style` is only read — taking it by ref too would just
// shift the clone to the caller.
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
pub fn render_output_format_content(
    content: &self::OutputFormatContent,
    prefix: Option<String>,
    or_splitter: Option<String>,
    enum_value_prefix: Option<String>,
    hoisted_class_prefix: Option<String>,
    always_hoist_enums: Option<bool>,
    quote_class_fields: Option<bool>,
    hoist_classes: Option<Vec<String>>,
    map_style: Option<String>,
    render_null_as: Option<String>,
) -> Result<String, RenderError> {
    use self::{HoistClasses, MapStyle, RenderOptions, RenderSetting};
    fn setting<V>(o: Option<V>) -> RenderSetting<V> {
        o.map_or(RenderSetting::Auto, RenderSetting::Always)
    }
    let options = RenderOptions {
        prefix: setting(prefix),
        or_splitter: setting(or_splitter),
        enum_value_prefix: setting(enum_value_prefix),
        hoisted_class_prefix: setting(hoisted_class_prefix),
        always_hoist_enums: setting(always_hoist_enums),
        quote_class_fields: setting(quote_class_fields),
        hoist_classes: hoist_classes.map_or(HoistClasses::Auto, HoistClasses::Subset),
        map_style: match map_style.as_deref() {
            // `type_parameters` is the opt-in escape hatch (`map<K, V>`); every
            // other value (including the default) renders the JSON object shape.
            Some("type_parameters") => MapStyle::TypeParameters,
            _ => MapStyle::ObjectLiteral,
        },
        render_null_as: setting(render_null_as),
    };
    content.render(&options).map(Option::unwrap_or_default)
}

/// Build an `OutputFormatContent` by walking a `RuntimeTy` and collecting all
/// referenced class/enum/type-alias definitions from `SysOpContext`.
pub fn build_output_format_content(
    ty: &baml_type::RuntimeTy,
    ctx: &::sys_types::SysOpContext,
) -> self::OutputFormatContent {
    use std::collections::HashSet;

    let mut content = self::OutputFormatContent::new(ty.clone());
    let mut visited = HashSet::new();
    let mut ancestry = Vec::new();

    if let Err(error) = walk_ty(
        ty,
        &baml_type::template::TyTemplateOrigins::root(),
        ctx,
        &mut content,
        &mut visited,
        &mut ancestry,
    ) {
        content.build_error = Some(error);
    }

    content
}

fn find_class_definition<'a>(
    ctx: &'a ::sys_types::SysOpContext,
    type_name: &baml_type::TypeName,
) -> Option<&'a ::sys_types::ClassDefinition> {
    ctx.class_definitions.get(type_name).or_else(|| {
        let mut matches = ctx
            .class_definitions
            .iter()
            .filter(|(name, _)| name.display_name() == type_name.display_name())
            .map(|(_, def)| def);
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    })
}

fn find_enum_definition<'a>(
    ctx: &'a ::sys_types::SysOpContext,
    type_name: &baml_type::TypeName,
) -> Option<&'a ::sys_types::EnumDefinition> {
    ctx.enum_definitions.get(type_name).or_else(|| {
        let mut matches = ctx
            .enum_definitions
            .iter()
            .filter(|(name, _)| name.display_name() == type_name.display_name())
            .map(|(_, def)| def);
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    })
}

fn find_type_alias_definition<'a>(
    ctx: &'a ::sys_types::SysOpContext,
    type_name: &baml_type::TypeName,
) -> Option<&'a baml_type::RuntimeTy> {
    ctx.type_alias_definitions.get(type_name).or_else(|| {
        let mut matches = ctx
            .type_alias_definitions
            .iter()
            .filter(|(name, _)| name.display_name() == type_name.display_name())
            .map(|(_, ty)| ty);
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum OutputVisitKey {
    Enum(baml_type::TypeName),
    TypeAlias(baml_type::TypeName),
}

struct ClassFrame {
    ty: RuntimeTy,
    name: baml_type::TypeName,
    output_name: String,
    arity: usize,
}

/// Recursive DFS walk of a type tree. `ancestry` tracks the realized classes
/// currently on the call stack so mutual recursion (A → B → A) is detected and
/// transformed generic recursion (`Box<int> → Box<Box<int>> → ...`) is
/// rejected.
fn walk_ty(
    ty: &baml_type::RuntimeTy,
    origins: &baml_type::template::TyTemplateOrigins,
    ctx: &::sys_types::SysOpContext,
    content: &mut self::OutputFormatContent,
    visited: &mut std::collections::HashSet<OutputVisitKey>,
    ancestry: &mut Vec<ClassFrame>,
) -> Result<(), RenderError> {
    use baml_type::RuntimeTy;

    match ty {
        RuntimeTy::Class(type_name, type_args, _) => {
            let output_key = class_instantiation_key(ty);

            // If this class is already on the ancestry stack, it's a recursive cycle.
            // Only mark classes from the cycle start, not unrelated ancestors.
            if let Some(start) = ancestry.iter().position(|frame| frame.ty == *ty) {
                for frame in &ancestry[start..] {
                    content.recursive_classes.insert(frame.output_name.clone());
                }
                return Ok(());
            }

            for (index, ancestor) in ancestry.iter().enumerate() {
                if ancestor.name == *type_name
                    && origins.class_transform_expands(index, type_name, ancestor.arity)
                {
                    return Err(RenderError::NonRegularRecursiveGeneric {
                        class: type_name.display_name().to_string(),
                        ancestor: ancestor.output_name.clone(),
                        instantiation: output_key,
                    });
                }
            }

            if let Some(class_def) = find_class_definition(ctx, type_name) {
                // `field_type` is the erased/runtime shape used by older
                // callers. Emitted classes also carry a symbolic template so
                // a generic class visited as `Outer<Choice>` can substitute
                // `T` through nested positions such as `Inner<T>.values`.
                let field_types: Vec<RuntimeTy> = class_def
                    .fields
                    .iter()
                    .filter(|f| !f.skip)
                    .map(|f| {
                        f.field_template
                            .as_ref()
                            .map(|template| template.substitute_symbolic(type_args))
                            .unwrap_or_else(|| f.field_type.clone())
                    })
                    .collect();
                let fields: Vec<self::ClassField> = class_def
                    .fields
                    .iter()
                    .filter(|f| !f.skip)
                    .zip(field_types.iter())
                    .map(|(field, field_type)| self::ClassField {
                        name: field.name.clone(),
                        alias: field.alias.clone(),
                        field_type: field_type.clone(),
                        description: field.description.clone(),
                    })
                    .collect();

                content.classes.insert(
                    output_key.clone(),
                    self::Class {
                        name: type_name.display_name().to_string(),
                        alias: class_def.alias.clone(),
                        description: class_def.description.clone(),
                        fields,
                    },
                );

                // Push onto ancestry before recursing into fields
                ancestry.push(ClassFrame {
                    ty: ty.clone(),
                    name: type_name.clone(),
                    output_name: output_key,
                    arity: type_args.len(),
                });
                for (field, field_type) in class_def
                    .fields
                    .iter()
                    .filter(|field| !field.skip)
                    .zip(&field_types)
                {
                    let field_origins = if let Some(template) = &field.field_template {
                        origins.through_field(type_name, type_args.len(), template)
                    } else {
                        baml_type::template::TyTemplateOrigins::opaque(ancestry.len())
                    };
                    walk_ty(field_type, &field_origins, ctx, content, visited, ancestry)?;
                }
                ancestry.pop();
            }
        }
        RuntimeTy::Enum(type_name, _) => {
            let key = OutputVisitKey::Enum(type_name.clone());
            if !visited.insert(key) {
                return Ok(());
            }
            if let Some(enum_def) = find_enum_definition(ctx, type_name) {
                // Skipped variants are already filtered out in bex_engine extraction.
                let values: Vec<self::EnumValue> = enum_def
                    .variants
                    .iter()
                    .map(|v| self::EnumValue {
                        name: v.name.clone(),
                        alias: v.alias.clone(),
                        description: v.description.clone(),
                    })
                    .collect();

                content.enums.insert(
                    type_name.display_name().to_string(),
                    self::Enum {
                        name: type_name.display_name().to_string(),
                        alias: enum_def.alias.clone(),
                        description: enum_def.description.clone(),
                        values,
                    },
                );
            }
        }
        RuntimeTy::TypeAlias(type_name, _) => {
            // The `baml.json.json` recursive alias is an opaque leaf for output-format
            // rendering — it has no schema body to collect.  Record the sentinel visit so
            // any later reference is de-duped, but do *not* insert it into
            // `recursive_type_aliases` (which would trigger schema emission) and do *not*
            // recurse into the alias body (which would diverge on the self-referential
            // `json[]` / `map<string, json>` arms).
            if type_name.display_name().as_str() == ::baml_base::qualified_name::BAML_JSON_JSON {
                visited.insert(OutputVisitKey::TypeAlias(type_name.clone()));
                return Ok(());
            }
            let key = OutputVisitKey::TypeAlias(type_name.clone());
            if !visited.insert(key) {
                return Ok(());
            }
            if let Some(target_ty) = find_type_alias_definition(ctx, type_name) {
                content
                    .recursive_type_aliases
                    .insert(type_name.display_name().to_string(), target_ty.clone());
                let target_origins = baml_type::template::TyTemplateOrigins::opaque(ancestry.len());
                walk_ty(target_ty, &target_origins, ctx, content, visited, ancestry)?;
            }
        }
        RuntimeTy::List(inner, _) => {
            let inner_origins = origins.list_element();
            walk_ty(inner, &inner_origins, ctx, content, visited, ancestry)?;
        }
        RuntimeTy::Map { key, value, .. } => {
            let key_origins = origins.map_key();
            let value_origins = origins.map_value();
            walk_ty(key, &key_origins, ctx, content, visited, ancestry)?;
            walk_ty(value, &value_origins, ctx, content, visited, ancestry)?;
        }
        RuntimeTy::Union(members, _) => {
            for (index, member) in members.iter().enumerate() {
                let member_origins = origins.union_member(index);
                walk_ty(member, &member_origins, ctx, content, visited, ancestry)?;
            }
        }
        _ => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_type::{Freshness, TyAttr, TypeName};

    use super::*;

    // -------------------------------------------------------------------------
    // Phase 3: json alias sentinel
    // -------------------------------------------------------------------------

    /// `RuntimeTy::TypeAlias("baml.json.json")` as the target type renders as the static
    /// literal "Respond with valid JSON." regardless of render options, with no
    /// schema body appended.
    #[test]
    fn test_render_json_alias_sentinel() {
        let json_tn = TypeName::from_dotted_path(::baml_base::qualified_name::BAML_JSON_JSON);
        let json_ty = RuntimeTy::TypeAlias(json_tn, TyAttr::default());
        let content = OutputFormatContent::new(json_ty);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Respond with valid JSON.".to_string()),
            "json alias should render as the static prompt literal"
        );
    }

    /// With `RenderSetting::Always(prefix)`, the sentinel still overrides and
    /// returns "Respond with valid JSON." — no prefix + no alias body.
    #[test]
    fn test_render_json_alias_sentinel_ignores_explicit_prefix() {
        let json_tn = TypeName::from_dotted_path(::baml_base::qualified_name::BAML_JSON_JSON);
        let json_ty = RuntimeTy::TypeAlias(json_tn, TyAttr::default());
        let content = OutputFormatContent::new(json_ty);

        let options = RenderOptions {
            prefix: RenderSetting::Always("CUSTOM PREFIX: ".to_string()),
            ..RenderOptions::default()
        };
        let rendered = content.render(&options).unwrap();
        assert_eq!(
            rendered,
            Some("Respond with valid JSON.".to_string()),
            "json alias sentinel overrides explicit prefix"
        );
    }

    /// A non-json alias does NOT trigger the sentinel and renders normally.
    #[test]
    fn test_render_non_json_alias_does_not_sentinel() {
        let other_tn = TypeName::from_dotted_path("baml.other.SomeAlias");
        let other_ty = RuntimeTy::TypeAlias(other_tn, TyAttr::default());
        // Without any class/enum definitions or recursive_type_aliases, the alias
        // renders as just its display name (the existing fallback).
        let content = OutputFormatContent::new(other_ty);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        // Should NOT be "Respond with valid JSON." — exact value depends on the
        // general TypeAlias rendering path (display name + prefix).
        assert_ne!(
            rendered,
            Some("Respond with valid JSON.".to_string()),
            "non-json alias should not trigger the json sentinel"
        );
    }

    #[test]
    fn test_render_string() {
        let content = OutputFormatContent::new(RuntimeTy::String {
            attr: TyAttr::default(),
        });
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(rendered, None);
    }

    #[test]
    fn test_render_int() {
        let content = OutputFormatContent::new(RuntimeTy::Int {
            attr: TyAttr::default(),
        });
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(rendered, Some("Answer as an int".to_string()));
    }

    #[test]
    fn test_render_bigint() {
        let content = OutputFormatContent::new(RuntimeTy::Bigint {
            attr: TyAttr::default(),
        });
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(rendered, Some("Answer as a bigint".to_string()));
    }

    #[test]
    fn test_render_float() {
        let content = OutputFormatContent::new(RuntimeTy::Float {
            attr: TyAttr::default(),
        });
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(rendered, Some("Answer as a float".to_string()));
    }

    #[test]
    fn test_render_bool() {
        let content = OutputFormatContent::new(RuntimeTy::Bool {
            attr: TyAttr::default(),
        });
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(rendered, Some("Answer as a bool".to_string()));
    }

    #[test]
    fn test_render_list() {
        let content = OutputFormatContent::new(RuntimeTy::List(
            Box::new(RuntimeTy::String {
                attr: TyAttr::default(),
            }),
            TyAttr::default(),
        ));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer with a JSON Array using this schema:\nstring[]".to_string())
        );
    }

    #[test]
    fn test_render_list_of_int() {
        let content = OutputFormatContent::new(RuntimeTy::List(
            Box::new(RuntimeTy::Int {
                attr: TyAttr::default(),
            }),
            TyAttr::default(),
        ));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer with a JSON Array using this schema:\nint[]".to_string())
        );
    }

    #[test]
    fn test_render_media_output_instructions() {
        let image = RuntimeTy::Media(baml_base::MediaKind::Image, TyAttr::default());

        let rendered = OutputFormatContent::new(image.clone())
            .render(&RenderOptions::default())
            .unwrap();
        assert_eq!(rendered, Some("Return an image output.".to_string()));

        let rendered =
            OutputFormatContent::new(RuntimeTy::List(Box::new(image.clone()), TyAttr::default()))
                .render(&RenderOptions::default())
                .unwrap();
        assert_eq!(
            rendered,
            Some("Return one or more image outputs.".to_string())
        );

        let text_or_image = RuntimeTy::Union(
            vec![
                RuntimeTy::String {
                    attr: TyAttr::default(),
                },
                image.clone(),
            ],
            TyAttr::default(),
        );

        let rendered = OutputFormatContent::new(text_or_image.clone())
            .render(&RenderOptions::default())
            .unwrap();
        assert_eq!(
            rendered,
            Some("Return either text or an image output.".to_string())
        );

        let rendered =
            OutputFormatContent::new(RuntimeTy::List(Box::new(text_or_image), TyAttr::default()))
                .render(&RenderOptions::default())
                .unwrap();
        assert_eq!(
            rendered,
            Some("Return an ordered sequence of text and image outputs.".to_string())
        );

        let rendered = OutputFormatContent::new(RuntimeTy::optional(image.clone()))
            .render(&RenderOptions::default())
            .unwrap();
        assert_eq!(
            rendered,
            Some("Return an image output or null.".to_string())
        );

        let rendered = OutputFormatContent::new(RuntimeTy::optional(image.clone()))
            .render(&RenderOptions {
                render_null_as: RenderSetting::Always("omit".to_string()),
                ..RenderOptions::default()
            })
            .unwrap();
        assert_eq!(
            rendered,
            Some("Return an image output or omit.".to_string())
        );

        let rendered = OutputFormatContent::new(RuntimeTy::Union(
            vec![
                image,
                RuntimeTy::Null {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        ))
        .render(&RenderOptions::default())
        .unwrap();
        assert_eq!(
            rendered,
            Some("Return an image output or null.".to_string())
        );
    }

    #[test]
    fn test_render_optional() {
        let content = OutputFormatContent::new(RuntimeTy::optional(RuntimeTy::String {
            attr: TyAttr::default(),
        }));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(rendered, Some("string or null".to_string()));
    }

    #[test]
    fn test_render_optional_with_custom_null_type() {
        let content = OutputFormatContent::new(RuntimeTy::optional(RuntimeTy::String {
            attr: TyAttr::default(),
        }));
        let rendered = content
            .render(&RenderOptions {
                render_null_as: RenderSetting::Always("omit".to_string()),
                ..RenderOptions::default()
            })
            .unwrap();
        assert_eq!(rendered, Some("string or omit".to_string()));
    }

    #[test]
    fn test_render_map() {
        let content = OutputFormatContent::new(RuntimeTy::Map {
            key: Box::new(RuntimeTy::String {
                attr: TyAttr::default(),
            }),
            value: Box::new(RuntimeTy::Int {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        });
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer in JSON using this schema:\n{ \"<string>\": int }".to_string())
        );
    }

    #[test]
    fn test_render_map_type_parameters_opt_in() {
        // `map_style='type_parameters'` stays available as an opt-in escape hatch
        // that renders the literal BAML type syntax.
        let content = OutputFormatContent::new(RuntimeTy::Map {
            key: Box::new(RuntimeTy::String {
                attr: TyAttr::default(),
            }),
            value: Box::new(RuntimeTy::Int {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        });
        let rendered = content
            .render(&RenderOptions {
                map_style: MapStyle::TypeParameters,
                ..RenderOptions::default()
            })
            .unwrap();
        assert_eq!(
            rendered,
            Some("Answer in JSON using this schema:\nmap<string, int>".to_string())
        );
    }

    #[test]
    fn test_render_class_with_map_field_object_shape() {
        // B-630 repro: a `map<string, T>` field must render as a JSON object
        // shape so the model sees the object it has to emit, rather than leaking
        // the literal BAML type syntax `map<string, int>`.
        let content = OutputFormatContent::new(ty_class("Review")).with_class(mk_class(
            "Review",
            vec![("scores", ty_map(ty_string(), ty_int()))],
        ));

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Answer in JSON using this schema:
{
  scores: { "<string>": int },
}"#
            ))
        );
    }

    #[test]
    fn test_render_class() {
        let cls = Class {
            name: "Person".to_string(),
            alias: None,
            description: Some("A person".to_string()),
            fields: vec![
                ClassField {
                    name: "name".to_string(),
                    alias: None,
                    field_type: RuntimeTy::String {
                        attr: TyAttr::default(),
                    },
                    description: None,
                },
                ClassField {
                    name: "age".to_string(),
                    alias: None,
                    field_type: RuntimeTy::Int {
                        attr: TyAttr::default(),
                    },
                    description: Some("Age in years".to_string()),
                },
            ],
        };

        let content = OutputFormatContent::new(RuntimeTy::Class(
            baml_type::TypeName::local("Person".into()),
            Vec::new(),
            TyAttr::default(),
        ))
        .with_class(cls);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer in JSON using this schema:\n\
                 {\n  \
                   /// A person\n\
                 \n  \
                   name: string,\n  \
                   /// Age in years\n  \
                   age: int,\n\
                 }"
                .to_string()
            )
        );
    }

    #[test]
    fn test_render_class_no_description() {
        let cls = Class {
            name: "Point".to_string(),
            alias: None,
            description: None,
            fields: vec![
                ClassField {
                    name: "x".to_string(),
                    alias: None,
                    field_type: RuntimeTy::Int {
                        attr: TyAttr::default(),
                    },
                    description: None,
                },
                ClassField {
                    name: "y".to_string(),
                    alias: None,
                    field_type: RuntimeTy::Int {
                        attr: TyAttr::default(),
                    },
                    description: None,
                },
            ],
        };

        let content = OutputFormatContent::new(RuntimeTy::Class(
            baml_type::TypeName::local("Point".into()),
            Vec::new(),
            TyAttr::default(),
        ))
        .with_class(cls);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer in JSON using this schema:\n\
                 {\n  \
                   x: int,\n  \
                   y: int,\n\
                 }"
                .to_string()
            )
        );
    }

    #[test]
    fn test_render_enum() {
        let enm = Enum {
            name: "Color".to_string(),
            alias: None,
            description: None,
            values: vec![
                EnumValue {
                    name: "Red".to_string(),
                    alias: None,
                    description: None,
                },
                EnumValue {
                    name: "Green".to_string(),
                    alias: None,
                    description: Some("Like grass".to_string()),
                },
                EnumValue {
                    name: "Blue".to_string(),
                    alias: None,
                    description: None,
                },
            ],
        };

        let content = OutputFormatContent::new(RuntimeTy::Enum(
            baml_type::TypeName::local("Color".into()),
            TyAttr::default(),
        ))
        .with_enum(enm);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer with any of the categories:\n\
                 Color\n\
                 ----\n\
                 - Red\n\
                 - Green: Like grass\n\
                 - Blue"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_render_union() {
        let content = OutputFormatContent::new(RuntimeTy::Union(
            vec![
                RuntimeTy::String {
                    attr: TyAttr::default(),
                },
                RuntimeTy::Int {
                    attr: TyAttr::default(),
                },
                RuntimeTy::Bool {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        ));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer in JSON using any of these schemas:\nstring or int or bool".to_string())
        );
    }

    #[test]
    fn test_render_with_custom_or_splitter() {
        let content = OutputFormatContent::new(RuntimeTy::Union(
            vec![
                RuntimeTy::String {
                    attr: TyAttr::default(),
                },
                RuntimeTy::Int {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        ));
        let options = RenderOptions {
            or_splitter: RenderSetting::Always(" | ".to_string()),
            ..Default::default()
        };
        let rendered = content.render(&options).unwrap();
        assert_eq!(
            rendered,
            Some("Answer in JSON using any of these schemas:\nstring | int".to_string())
        );
    }

    #[test]
    fn test_render_literal_string() {
        let content = OutputFormatContent::new(RuntimeTy::Literal(
            LiteralValue::String("hello".to_string()),
            Freshness::Regular,
            TyAttr::default(),
        ));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer using this specific value:\n\"hello\"".to_string())
        );
    }

    #[test]
    fn test_render_literal_int() {
        let content = OutputFormatContent::new(RuntimeTy::Literal(
            LiteralValue::Int(42),
            Freshness::Regular,
            TyAttr::default(),
        ));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer using this specific value:\n42".to_string())
        );
    }

    #[test]
    fn test_render_literal_bool() {
        let content = OutputFormatContent::new(RuntimeTy::Literal(
            LiteralValue::Bool(true),
            Freshness::Regular,
            TyAttr::default(),
        ));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer using this specific value:\ntrue".to_string())
        );
    }

    #[test]
    fn test_render_opaque_unsupported() {
        let content = OutputFormatContent::new(RuntimeTy::type_type());
        let err = content.render(&RenderOptions::default()).unwrap_err();
        assert!(matches!(err, RenderError::UnsupportedType(s) if s == "reflect.Type"));
    }

    #[test]
    fn test_render_non_data_type_returns_error_instead_of_panicking() {
        let content = OutputFormatContent::new(RuntimeTy::Never {
            attr: TyAttr::default(),
        });
        let err = content.render(&RenderOptions::default()).unwrap_err();
        assert!(matches!(err, RenderError::UnsupportedType(s) if s == "never"));
    }

    // ========================================================================
    // Helper functions for creating types (used by recursive type tests)
    // ========================================================================

    fn ty_int() -> RuntimeTy {
        RuntimeTy::Int {
            attr: TyAttr::default(),
        }
    }
    fn ty_bool() -> RuntimeTy {
        RuntimeTy::Bool {
            attr: TyAttr::default(),
        }
    }
    fn ty_string() -> RuntimeTy {
        RuntimeTy::String {
            attr: TyAttr::default(),
        }
    }
    fn ty_float() -> RuntimeTy {
        RuntimeTy::Float {
            attr: TyAttr::default(),
        }
    }
    fn ty_class(name: &str) -> RuntimeTy {
        RuntimeTy::Class(
            baml_type::TypeName::local(name.into()),
            Vec::new(),
            TyAttr::default(),
        )
    }
    fn ty_class_with_args(name: &str, args: Vec<RuntimeTy>) -> RuntimeTy {
        RuntimeTy::Class(
            baml_type::TypeName::local(name.into()),
            args,
            TyAttr::default(),
        )
    }
    fn ty_optional(inner: RuntimeTy) -> RuntimeTy {
        RuntimeTy::optional(inner)
    }
    fn ty_list(inner: RuntimeTy) -> RuntimeTy {
        RuntimeTy::List(Box::new(inner), TyAttr::default())
    }
    fn ty_map(key: RuntimeTy, value: RuntimeTy) -> RuntimeTy {
        RuntimeTy::Map {
            key: Box::new(key),
            value: Box::new(value),
            attr: TyAttr::default(),
        }
    }
    fn ty_union(variants: Vec<RuntimeTy>) -> RuntimeTy {
        RuntimeTy::Union(variants, TyAttr::default())
    }

    fn ty_enum(name: &str) -> RuntimeTy {
        RuntimeTy::Enum(baml_type::TypeName::local(name.into()), TyAttr::default())
    }

    fn mk_class(name: &str, fields: Vec<(&str, RuntimeTy)>) -> Class {
        Class {
            name: name.to_string(),
            alias: None,
            description: None,
            fields: fields
                .into_iter()
                .map(|(n, t)| ClassField {
                    name: n.to_string(),
                    alias: None,
                    field_type: t,
                    description: None,
                })
                .collect(),
        }
    }

    fn mk_class_desc(name: &str, desc: &str, fields: Vec<(&str, RuntimeTy)>) -> Class {
        Class {
            name: name.to_string(),
            alias: None,
            description: Some(desc.to_string()),
            fields: fields
                .into_iter()
                .map(|(n, t)| ClassField {
                    name: n.to_string(),
                    alias: None,
                    field_type: t,
                    description: None,
                })
                .collect(),
        }
    }

    fn mk_enum(name: &str, values: Vec<&str>) -> Enum {
        Enum {
            name: name.to_string(),
            alias: None,
            description: None,
            values: values
                .into_iter()
                .map(|v| EnumValue {
                    name: v.to_string(),
                    alias: None,
                    description: None,
                })
                .collect(),
        }
    }

    fn mk_recursive(names: &[&str]) -> indexmap::IndexSet<String> {
        names.iter().map(std::string::ToString::to_string).collect()
    }

    fn ctx_class_field(
        name: &str,
        field_type: RuntimeTy,
        field_template: Option<baml_type::TyTemplate>,
    ) -> sys_types::ClassFieldDefinition {
        sys_types::ClassFieldDefinition {
            name: name.to_string(),
            field_type,
            field_template,
            description: None,
            alias: None,
            skip: false,
        }
    }

    fn ctx_class_definition(
        name: &baml_type::TypeName,
        fields: Vec<sys_types::ClassFieldDefinition>,
    ) -> sys_types::ClassDefinition {
        sys_types::ClassDefinition {
            name: name.display_name().to_string(),
            description: None,
            alias: None,
            fields,
        }
    }

    // ========================================================================
    // Recursive class tests (ported from engine)
    // ========================================================================

    #[test]
    fn test_render_top_level_simple_recursive_class() {
        let mut content = OutputFormatContent::new(ty_class("Node")).with_class(mk_class(
            "Node",
            vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
        ));
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                "Node {\n  data: int,\n  next: Node or null,\n}\n\n\
                 Answer in JSON using this schema: Node"
            ))
        );
    }

    #[test]
    fn test_render_nested_simple_recursive_class() {
        let mut content = OutputFormatContent::new(ty_class("LinkedList"))
            .with_class(mk_class(
                "Node",
                vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
            ))
            .with_class(mk_class(
                "LinkedList",
                vec![("head", ty_optional(ty_class("Node"))), ("len", ty_int())],
            ));
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
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
    fn test_top_level_recursive_cycle() {
        let mut content = OutputFormatContent::new(ty_class("A"))
            .with_class(mk_class("A", vec![("pointer", ty_class("B"))]))
            .with_class(mk_class("B", vec![("pointer", ty_class("C"))]))
            .with_class(mk_class("C", vec![("pointer", ty_optional(ty_class("A")))]));
        content.recursive_classes = mk_recursive(&["A", "B", "C"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
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
    fn test_nested_recursive_cycle() {
        let mut content = OutputFormatContent::new(ty_class("NonRecursive"))
            .with_class(mk_class("A", vec![("pointer", ty_class("B"))]))
            .with_class(mk_class("B", vec![("pointer", ty_class("C"))]))
            .with_class(mk_class("C", vec![("pointer", ty_optional(ty_class("A")))]))
            .with_class(mk_class(
                "NonRecursive",
                vec![
                    ("pointer", ty_class("A")),
                    ("data", ty_int()),
                    ("field", ty_bool()),
                ],
            ));
        content.recursive_classes = mk_recursive(&["A", "B", "C"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
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
    fn test_nested_class_in_hoisted_recursive_class() {
        let mut content = OutputFormatContent::new(ty_class("NonRecursive"))
            .with_class(mk_class(
                "A",
                vec![("pointer", ty_class("B")), ("nested", ty_class("Nested"))],
            ))
            .with_class(mk_class("B", vec![("pointer", ty_class("C"))]))
            .with_class(mk_class("C", vec![("pointer", ty_optional(ty_class("A")))]))
            .with_class(mk_class(
                "NonRecursive",
                vec![
                    ("pointer", ty_class("A")),
                    ("data", ty_int()),
                    ("field", ty_bool()),
                ],
            ))
            .with_class(mk_class(
                "Nested",
                vec![("data", ty_int()), ("field", ty_bool())],
            ));
        content.recursive_classes = mk_recursive(&["A", "B", "C"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
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
    fn test_mutually_recursive_list() {
        let mut content = OutputFormatContent::new(ty_class("Tree"))
            .with_class(mk_class(
                "Tree",
                vec![("data", ty_int()), ("children", ty_class("Forest"))],
            ))
            .with_class(mk_class(
                "Forest",
                vec![("trees", ty_list(ty_class("Tree")))],
            ));
        content.recursive_classes = mk_recursive(&["Tree", "Forest"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
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

    // ========================================================================
    // Recursive class with description
    // ========================================================================

    #[test]
    fn test_hoisted_class_with_description() {
        let mut content = OutputFormatContent::new(ty_class("Node")).with_class(mk_class_desc(
            "Node",
            "A node in a linked list",
            vec![("value", ty_int()), ("next", ty_optional(ty_class("Node")))],
        ));
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                "/// A node in a linked list\nNode {\n  value: int,\n  next: Node or null,\n}\n\n\
                 Answer in JSON using this schema: Node"
            ))
        );
    }

    // ========================================================================
    // Recursive union tests
    // ========================================================================

    #[test]
    fn test_top_level_recursive_union() {
        let mut content =
            OutputFormatContent::new(ty_union(vec![ty_class("Node"), ty_class("Tree")]))
                .with_class(mk_class(
                    "Node",
                    vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
                ))
                .with_class(mk_class(
                    "Tree",
                    vec![("data", ty_int()), ("children", ty_list(ty_class("Tree")))],
                ));
        content.recursive_classes = mk_recursive(&["Node", "Tree"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
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
    fn test_nested_recursive_union() {
        let mut content = OutputFormatContent::new(ty_class("DataType"))
            .with_class(mk_class(
                "DataType",
                vec![
                    (
                        "data_type",
                        ty_union(vec![ty_class("Node"), ty_class("Tree")]),
                    ),
                    ("len", ty_int()),
                    ("description", ty_string()),
                ],
            ))
            .with_class(mk_class(
                "Node",
                vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
            ))
            .with_class(mk_class(
                "Tree",
                vec![("data", ty_int()), ("children", ty_list(ty_class("Tree")))],
            ));
        content.recursive_classes = mk_recursive(&["Node", "Tree"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
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
    fn test_top_level_recursive_union_with_non_recursive_class() {
        let mut content = OutputFormatContent::new(ty_union(vec![
            ty_class("Node"),
            ty_class("Tree"),
            ty_class("NonRecursive"),
        ]))
        .with_class(mk_class(
            "Node",
            vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
        ))
        .with_class(mk_class(
            "Tree",
            vec![("data", ty_int()), ("children", ty_list(ty_class("Tree")))],
        ))
        .with_class(mk_class(
            "NonRecursive",
            vec![("data", ty_int()), ("tag", ty_string())],
        ));
        content.recursive_classes = mk_recursive(&["Node", "Tree"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
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
    fn test_nested_recursive_union_with_non_recursive_class() {
        let mut content = OutputFormatContent::new(ty_class("DataType"))
            .with_class(mk_class(
                "DataType",
                vec![
                    (
                        "data_type",
                        ty_union(vec![
                            ty_class("Node"),
                            ty_class("Tree"),
                            ty_class("NonRecursive"),
                        ]),
                    ),
                    ("len", ty_int()),
                    ("description", ty_string()),
                ],
            ))
            .with_class(mk_class(
                "Node",
                vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
            ))
            .with_class(mk_class(
                "Tree",
                vec![("data", ty_int()), ("children", ty_list(ty_class("Tree")))],
            ))
            .with_class(mk_class(
                "NonRecursive",
                vec![("data", ty_int()), ("tag", ty_string())],
            ));
        content.recursive_classes = mk_recursive(&["Node", "Tree"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
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
    fn test_top_level_union_of_unions_pointing_to_recursive_class() {
        let mut content = OutputFormatContent::new(ty_union(vec![
            ty_union(vec![ty_class("Node"), ty_int()]),
            ty_union(vec![ty_string(), ty_class("Tree")]),
        ]))
        .with_class(mk_class(
            "Node",
            vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
        ))
        .with_class(mk_class(
            "Tree",
            vec![("data", ty_int()), ("children", ty_list(ty_class("Tree")))],
        ));
        content.recursive_classes = mk_recursive(&["Node", "Tree"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
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
    fn test_nested_union_of_unions_pointing_to_recursive_class() {
        let mut content = OutputFormatContent::new(ty_class("NonRecursive"))
            .with_class(mk_class(
                "NonRecursive",
                vec![
                    (
                        "the_union",
                        ty_union(vec![
                            ty_union(vec![ty_class("Node"), ty_int()]),
                            ty_union(vec![ty_string(), ty_class("Tree")]),
                        ]),
                    ),
                    ("data", ty_int()),
                    ("field", ty_bool()),
                ],
            ))
            .with_class(mk_class(
                "Node",
                vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
            ))
            .with_class(mk_class(
                "Tree",
                vec![("data", ty_int()), ("children", ty_list(ty_class("Tree")))],
            ));
        content.recursive_classes = mk_recursive(&["Node", "Tree"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
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

    // ========================================================================
    // Collection types (list/map) with recursion
    // ========================================================================

    #[test]
    fn test_render_top_level_list_with_recursive_items() {
        let mut content = OutputFormatContent::new(ty_list(ty_class("Node"))).with_class(mk_class(
            "Node",
            vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
        ));
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
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
    fn test_render_top_level_class_with_self_referential_map() {
        let mut content = OutputFormatContent::new(ty_class("RecursiveMap")).with_class(mk_class(
            "RecursiveMap",
            vec![("data", ty_map(ty_string(), ty_class("RecursiveMap")))],
        ));
        content.recursive_classes = mk_recursive(&["RecursiveMap"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"RecursiveMap {
  data: { "<string>": RecursiveMap },
}

Answer in JSON using this schema: RecursiveMap"#
            ))
        );
    }

    #[test]
    fn test_render_nested_self_referential_map() {
        let mut content = OutputFormatContent::new(ty_class("NonRecursive"))
            .with_class(mk_class(
                "RecursiveMap",
                vec![("data", ty_map(ty_string(), ty_class("RecursiveMap")))],
            ))
            .with_class(mk_class(
                "NonRecursive",
                vec![("rec_map", ty_class("RecursiveMap"))],
            ));
        content.recursive_classes = mk_recursive(&["RecursiveMap"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"RecursiveMap {
  data: { "<string>": RecursiveMap },
}

Answer in JSON using this schema:
{
  rec_map: RecursiveMap,
}"#
            ))
        );
    }

    #[test]
    fn test_render_top_level_map_pointing_to_another_recursive_class() {
        let mut content = OutputFormatContent::new(ty_map(ty_string(), ty_class("Node")))
            .with_class(mk_class(
                "Node",
                vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
            ));
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this schema:
{ "<string>": Node }"#
            ))
        );
    }

    #[test]
    fn test_render_nested_map_pointing_to_another_recursive_class() {
        let mut content = OutputFormatContent::new(ty_class("MapWithRecValue"))
            .with_class(mk_class(
                "MapWithRecValue",
                vec![("data", ty_map(ty_string(), ty_class("Node")))],
            ))
            .with_class(mk_class(
                "Node",
                vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
            ));
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this schema:
{
  data: { "<string>": Node },
}"#
            ))
        );
    }

    #[test]
    fn test_render_nested_map_pointing_to_another_optional_recursive_class() {
        let mut content = OutputFormatContent::new(ty_class("MapWithRecValue"))
            .with_class(mk_class(
                "MapWithRecValue",
                vec![("data", ty_map(ty_string(), ty_optional(ty_class("Node"))))],
            ))
            .with_class(mk_class(
                "Node",
                vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
            ));
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this schema:
{
  data: { "<string>": Node or null },
}"#
            ))
        );
    }

    #[test]
    fn test_render_top_level_map_pointing_to_recursive_union() {
        let mut content = OutputFormatContent::new(ty_map(
            ty_string(),
            ty_union(vec![ty_class("Node"), ty_int(), ty_class("NonRecursive")]),
        ))
        .with_class(mk_class(
            "Node",
            vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
        ))
        .with_class(mk_class(
            "NonRecursive",
            vec![("field", ty_string()), ("data", ty_int())],
        ));
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this schema:
{ "<string>": Node or int or {
  field: string,
  data: int,
} }"#
            ))
        );
    }

    #[test]
    fn test_render_nested_map_pointing_to_recursive_union() {
        let mut content = OutputFormatContent::new(ty_class("MapWithRecUnion"))
            .with_class(mk_class(
                "MapWithRecUnion",
                vec![(
                    "data",
                    ty_map(
                        ty_string(),
                        ty_union(vec![ty_class("Node"), ty_int(), ty_class("NonRecursive")]),
                    ),
                )],
            ))
            .with_class(mk_class(
                "Node",
                vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
            ))
            .with_class(mk_class(
                "NonRecursive",
                vec![("field", ty_string()), ("data", ty_int())],
            ));
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this schema:
{
  data: { "<string>": Node or int or {
    field: string,
    data: int,
  } },
}"#
            ))
        );
    }

    // ========================================================================
    // Hoisting options tests
    // ========================================================================

    #[test]
    fn test_render_hoisted_classes_with_prefix() {
        let mut content = OutputFormatContent::new(ty_class("NonRecursive"))
            .with_class(mk_class("A", vec![("pointer", ty_class("B"))]))
            .with_class(mk_class("B", vec![("pointer", ty_class("C"))]))
            .with_class(mk_class("C", vec![("pointer", ty_optional(ty_class("A")))]))
            .with_class(mk_class(
                "NonRecursive",
                vec![
                    ("pointer", ty_class("A")),
                    ("data", ty_int()),
                    ("field", ty_bool()),
                ],
            ));
        content.recursive_classes = mk_recursive(&["A", "B", "C"]);

        let options = RenderOptions {
            hoisted_class_prefix: RenderSetting::Always("interface".to_string()),
            ..Default::default()
        };
        let rendered = content.render(&options).unwrap();
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
    fn test_render_hoisted_classes_subset() {
        let content = OutputFormatContent::new(ty_class("Ret"))
            .with_class(mk_class("A", vec![("prop", ty_int())]))
            .with_class(mk_class("B", vec![("prop", ty_string())]))
            .with_class(mk_class("C", vec![("prop", ty_float())]))
            .with_class(mk_class(
                "Ret",
                vec![
                    ("a", ty_class("A")),
                    ("b", ty_class("B")),
                    ("c", ty_class("C")),
                ],
            ));

        let options = RenderOptions {
            hoist_classes: HoistClasses::Subset(vec!["A".to_string(), "B".to_string()]),
            ..Default::default()
        };
        let rendered = content.render(&options).unwrap();
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
    fn test_render_hoisted_generic_class_family_subset() {
        let box_int = ty_class_with_args("Box", vec![ty_int()]);
        let box_string = ty_class_with_args("Box", vec![ty_string()]);
        let mut content = OutputFormatContent::new(ty_class("Ret")).with_class(mk_class(
            "Ret",
            vec![
                ("int_box", box_int.clone()),
                ("string_box", box_string.clone()),
            ],
        ));
        content.classes.insert(
            class_instantiation_key(&box_int),
            mk_class("Box", vec![("value", ty_int())]),
        );
        content.classes.insert(
            class_instantiation_key(&box_string),
            mk_class("Box", vec![("value", ty_string())]),
        );

        let options = RenderOptions {
            hoist_classes: HoistClasses::Subset(vec!["Box".to_string()]),
            ..Default::default()
        };
        let rendered = content.render(&options).unwrap().unwrap();

        assert_eq!(rendered.matches("Box<int> {").count(), 1, "{rendered}");
        assert_eq!(rendered.matches("Box<string> {").count(), 1, "{rendered}");
        assert!(rendered.contains("int_box: Box<int>,"), "{rendered}");
        assert!(rendered.contains("string_box: Box<string>,"), "{rendered}");
    }

    #[test]
    fn test_render_hoist_all_classes() {
        let content = OutputFormatContent::new(ty_class("Ret"))
            .with_class(mk_class("A", vec![("prop", ty_int())]))
            .with_class(mk_class("B", vec![("prop", ty_string())]))
            .with_class(mk_class("C", vec![("prop", ty_float())]))
            .with_class(mk_class(
                "Ret",
                vec![
                    ("a", ty_class("A")),
                    ("b", ty_class("B")),
                    ("c", ty_class("C")),
                ],
            ));

        let options = RenderOptions {
            hoist_classes: HoistClasses::All,
            ..Default::default()
        };
        let rendered = content.render(&options).unwrap();
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

    // ========================================================================
    // Attribute handling tests (ported from old engine + new)
    // ========================================================================

    /// Ported from old engine: `skipped_variants_are_not_rendered`
    #[test]
    fn skipped_variants_are_not_rendered() {
        // Enum Foo with @skip on Baz variant — only Bar should render
        let enm = Enum {
            name: "Foo".to_string(),
            alias: None,
            description: None,
            values: vec![EnumValue {
                name: "Bar".to_string(),
                alias: None,
                description: None,
            }],
            // Baz is already filtered out by the extraction layer
        };

        let content = OutputFormatContent::new(ty_enum("Foo")).with_enum(enm);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer with any of the categories:\n\
                 Foo\n\
                 ----\n\
                 - Bar"
                    .to_string()
            )
        );
    }

    /// Ported from old engine: `skipped_class_fields_are_not_rendered`
    #[test]
    fn skipped_class_fields_are_not_rendered() {
        // Class with @skip optional field — only `keep` field rendered
        let cls = Class {
            name: "MyClass".to_string(),
            alias: None,
            description: None,
            fields: vec![ClassField {
                name: "keep".to_string(),
                alias: None,
                field_type: ty_string(),
                description: None,
            }],
            // hidden field is already filtered out by the extraction layer
        };

        let content = OutputFormatContent::new(ty_class("MyClass")).with_class(cls);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer in JSON using this schema:\n\
                 {\n\
                 \x20 keep: string,\n\
                 }"
                .to_string()
            )
        );
    }

    /// Ported from old engine: `test_render_output_format_aliases` (recursive Date → hoisted)
    /// Note: Enum hoisting and list wrapping format differ from old engine.
    /// This test verifies alias/description/skip work correctly with the current renderer.
    #[test]
    fn test_render_output_format_aliases() {
        // Recursive Date class (self-referencing via year: Date?)
        let month_enum = mk_enum(
            "Month",
            vec![
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ],
        );

        let date_cls = Class {
            name: "Date".to_string(),
            alias: None,
            description: None,
            fields: vec![
                ClassField {
                    name: "day".to_string(),
                    alias: None,
                    field_type: ty_int(),
                    description: None,
                },
                ClassField {
                    name: "month".to_string(),
                    alias: None,
                    field_type: ty_enum("Month"),
                    description: None,
                },
                ClassField {
                    name: "year".to_string(),
                    alias: None,
                    field_type: ty_optional(ty_class("Date")),
                    description: None,
                },
            ],
        };

        let education_cls = Class {
            name: "Education".to_string(),
            alias: None,
            description: None,
            fields: vec![
                ClassField {
                    name: "from_date".to_string(),
                    alias: None,
                    field_type: ty_class("Date"),
                    description: None,
                },
                ClassField {
                    name: "to_date".to_string(),
                    alias: None,
                    field_type: ty_union(vec![
                        ty_class("Date"),
                        RuntimeTy::Literal(
                            LiteralValue::String("current".to_string()),
                            Freshness::Regular,
                            TyAttr::default(),
                        ),
                    ]),
                    description: None,
                },
                ClassField {
                    name: "school".to_string(),
                    alias: None,
                    field_type: ty_string(),
                    description: None,
                },
                ClassField {
                    name: "description".to_string(),
                    alias: None,
                    field_type: ty_string(),
                    description: None,
                },
            ],
        };

        let resume_cls = Class {
            name: "Resume".to_string(),
            alias: None,
            description: None,
            fields: vec![ClassField {
                name: "education".to_string(),
                alias: None,
                field_type: ty_list(ty_class("Education")),
                description: None,
            }],
        };

        let mut content = OutputFormatContent::new(ty_class("Resume"))
            .with_enum(month_enum)
            .with_class(date_cls)
            .with_class(education_cls)
            .with_class(resume_cls);
        content.recursive_classes = mk_recursive(&["Date"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        // Verify Date is hoisted, enum renders inline with header, fields present
        let r = rendered.unwrap();
        assert!(r.contains("Date {"), "Date should be hoisted");
        assert!(r.contains("day: int,"), "Date should have day field");
        assert!(
            r.contains("year: Date or null,"),
            "Date should self-reference"
        );
        assert!(
            r.contains("from_date: Date,"),
            "Education should reference Date"
        );
        assert!(r.contains("to_date: Date or \"current\","), "to_date union");
        assert!(r.contains("school: string,"), "school field");
    }

    /// Ported from old engine: `test_render_output_format` (non-recursive → inline)
    #[test]
    fn test_render_output_format_inline() {
        // Non-recursive version — Date has year: int (not Date?)
        let month_enum = mk_enum(
            "Month",
            vec![
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ],
        );

        let date_cls = Class {
            name: "Date".to_string(),
            alias: None,
            description: None,
            fields: vec![
                ClassField {
                    name: "day".to_string(),
                    alias: None,
                    field_type: ty_int(),
                    description: None,
                },
                ClassField {
                    name: "month".to_string(),
                    alias: None,
                    field_type: ty_enum("Month"),
                    description: None,
                },
                ClassField {
                    name: "year".to_string(),
                    alias: None,
                    field_type: ty_int(),
                    description: None,
                },
            ],
        };

        let education_cls = Class {
            name: "Education".to_string(),
            alias: None,
            description: None,
            fields: vec![
                ClassField {
                    name: "from_date".to_string(),
                    alias: None,
                    field_type: ty_class("Date"),
                    description: None,
                },
                ClassField {
                    name: "to_date".to_string(),
                    alias: None,
                    field_type: ty_union(vec![
                        ty_class("Date"),
                        RuntimeTy::Literal(
                            LiteralValue::String("current".to_string()),
                            Freshness::Regular,
                            TyAttr::default(),
                        ),
                    ]),
                    description: None,
                },
                ClassField {
                    name: "school".to_string(),
                    alias: None,
                    field_type: ty_string(),
                    description: None,
                },
                ClassField {
                    name: "description".to_string(),
                    alias: None,
                    field_type: ty_string(),
                    description: None,
                },
            ],
        };

        let resume_cls = Class {
            name: "Resume".to_string(),
            alias: None,
            description: None,
            fields: vec![ClassField {
                name: "education".to_string(),
                alias: None,
                field_type: ty_list(ty_class("Education")),
                description: None,
            }],
        };

        let content = OutputFormatContent::new(ty_class("Resume"))
            .with_enum(month_enum)
            .with_class(date_cls)
            .with_class(education_cls)
            .with_class(resume_cls);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        // Verify inline rendering (no hoisting since not recursive)
        let r = rendered.unwrap();
        assert!(
            !r.contains("Date {"),
            "Date should NOT be hoisted (not recursive)"
        );
        assert!(r.contains("day: int,"), "Date fields should be inline");
        assert!(r.contains("year: int,"), "year is int, not self-ref");
        assert!(r.contains("from_date:"), "Education from_date");
        assert!(r.contains("school: string,"), "school field");
    }

    /// Ported from old engine: `test_render_output_format_description_and_alias`
    #[test]
    fn test_render_output_format_description_and_alias() {
        let cls = Class {
            name: "MyClass".to_string(),
            alias: None,
            description: None,
            fields: vec![ClassField {
                name: "Name".to_string(),
                alias: Some("a".to_string()),
                field_type: ty_string(),
                description: Some("d".to_string()),
            }],
        };

        let content = OutputFormatContent::new(ty_class("MyClass")).with_class(cls);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer in JSON using this schema:\n\
                 {\n\
                 \x20 /// d\n\
                 \x20 a: string,\n\
                 }"
                .to_string()
            )
        );
    }

    /// New test: class with @alias on fields, no description
    #[test]
    fn test_render_class_with_field_alias() {
        let cls = Class {
            name: "MyClass".to_string(),
            alias: None,
            description: None,
            fields: vec![
                ClassField {
                    name: "my_field".to_string(),
                    alias: Some("myField".to_string()),
                    field_type: ty_string(),
                    description: None,
                },
                ClassField {
                    name: "other".to_string(),
                    alias: None,
                    field_type: ty_int(),
                    description: None,
                },
            ],
        };

        let content = OutputFormatContent::new(ty_class("MyClass")).with_class(cls);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer in JSON using this schema:\n\
                 {\n\
                 \x20 myField: string,\n\
                 \x20 other: int,\n\
                 }"
                .to_string()
            )
        );
    }

    /// New test: enum with @alias on variants
    #[test]
    fn test_render_enum_with_variant_alias() {
        let enm = Enum {
            name: "Color".to_string(),
            alias: None,
            description: None,
            values: vec![
                EnumValue {
                    name: "Red".to_string(),
                    alias: Some("r".to_string()),
                    description: None,
                },
                EnumValue {
                    name: "Green".to_string(),
                    alias: Some("g".to_string()),
                    description: Some("Like grass".to_string()),
                },
                EnumValue {
                    name: "Blue".to_string(),
                    alias: None,
                    description: None,
                },
            ],
        };

        let content = OutputFormatContent::new(ty_enum("Color")).with_enum(enm);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer with any of the categories:\n\
                 Color\n\
                 ----\n\
                 - r\n\
                 - g: Like grass\n\
                 - Blue"
                    .to_string()
            )
        );
    }

    /// New test: enum with @@alias on the enum itself
    #[test]
    fn test_render_enum_with_block_alias() {
        let enm = Enum {
            name: "TestEnum".to_string(),
            alias: Some("Category".to_string()),
            description: None,
            values: vec![
                EnumValue {
                    name: "A".to_string(),
                    alias: None,
                    description: None,
                },
                EnumValue {
                    name: "B".to_string(),
                    alias: None,
                    description: None,
                },
            ],
        };

        let content = OutputFormatContent::new(ty_enum("TestEnum")).with_enum(enm);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer with any of the categories:\n\
                 Category\n\
                 ----\n\
                 - A\n\
                 - B"
                .to_string()
            )
        );
    }

    /// New test: recursive class with @@alias, verify hoisted definition and references use alias
    #[test]
    fn test_render_hoisted_class_with_alias() {
        let cls = Class {
            name: "Node".to_string(),
            alias: Some("GraphNode".to_string()),
            description: None,
            fields: vec![
                ClassField {
                    name: "data".to_string(),
                    alias: None,
                    field_type: ty_int(),
                    description: None,
                },
                ClassField {
                    name: "next".to_string(),
                    alias: None,
                    field_type: ty_optional(ty_class("Node")),
                    description: None,
                },
            ],
        };

        let mut content = OutputFormatContent::new(ty_class("Node")).with_class(cls);
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                "GraphNode {\n\
                 \x20 data: int,\n\
                 \x20 next: GraphNode or null,\n\
                 }\n\
                 \n\
                 Answer in JSON using this schema: GraphNode"
            ))
        );
    }

    #[test]
    fn test_hoisted_generic_class_alias_preserves_type_arguments() {
        let generic_key = "Box<int>";
        let mut content = OutputFormatContent::new(ty_class("Wrapper"))
            .with_class(mk_class(
                "Wrapper",
                vec![("value", ty_class_with_args("Box", vec![ty_int()]))],
            ))
            .with_class(Class {
                name: "Box".to_string(),
                alias: None,
                description: None,
                fields: vec![ClassField {
                    name: "value".to_string(),
                    alias: None,
                    field_type: ty_int(),
                    description: None,
                }],
            });
        content.classes.swap_remove("Box");
        content.classes.insert(
            generic_key.to_string(),
            Class {
                name: "Box".to_string(),
                alias: Some("Container".to_string()),
                description: None,
                fields: vec![ClassField {
                    name: "value".to_string(),
                    alias: None,
                    field_type: ty_int(),
                    description: None,
                }],
            },
        );
        content.recursive_classes = mk_recursive(&[generic_key]);

        let rendered = content.render(&RenderOptions::default()).unwrap().unwrap();
        assert!(
            rendered.contains("Container<int> {\n  value: int,\n}"),
            "{rendered}"
        );
        assert!(rendered.contains("value: Container<int>,"), "{rendered}");
    }

    #[test]
    fn test_hoisted_generic_class_alias_collision_is_rejected() {
        let box_int = ty_class_with_args("Box", vec![ty_int()]);
        let crate_int = ty_class_with_args("Crate", vec![ty_int()]);
        let mut content = OutputFormatContent::new(ty_class("Wrapper")).with_class(mk_class(
            "Wrapper",
            vec![("box", box_int.clone()), ("crate", crate_int.clone())],
        ));
        for (ty, name) in [(box_int, "Box"), (crate_int, "Crate")] {
            content.classes.insert(
                class_instantiation_key(&ty),
                Class {
                    name: name.to_string(),
                    alias: Some("Container".to_string()),
                    description: None,
                    fields: vec![ClassField {
                        name: "value".to_string(),
                        alias: None,
                        field_type: ty_int(),
                        description: None,
                    }],
                },
            );
        }

        let error = content
            .render(&RenderOptions {
                hoist_classes: HoistClasses::All,
                ..Default::default()
            })
            .unwrap_err();
        assert!(matches!(
            error,
            RenderError::RenderedClassNameCollision {
                rendered_name,
                first,
                second,
            } if rendered_name == "Container<int>"
                && first == "Box<int>"
                && second == "Crate<int>"
        ));
    }

    /// New test: class with @@description, verify comment rendered
    #[test]
    fn test_render_class_with_class_description() {
        let cls = Class {
            name: "Foo".to_string(),
            alias: None,
            description: Some("A foo object".to_string()),
            fields: vec![
                ClassField {
                    name: "bar".to_string(),
                    alias: None,
                    field_type: ty_string(),
                    description: None,
                },
                ClassField {
                    name: "baz".to_string(),
                    alias: None,
                    field_type: ty_int(),
                    description: Some("A baz field".to_string()),
                },
            ],
        };

        let content = OutputFormatContent::new(ty_class("Foo")).with_class(cls);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer in JSON using this schema:\n\
                 {\n  \
                   /// A foo object\n\
                 \n  \
                   bar: string,\n  \
                   /// A baz field\n  \
                   baz: int,\n\
                 }"
                .to_string()
            )
        );
    }

    // ========================================================================
    // Phase 5: Additional test coverage
    // ========================================================================

    fn ty_alias(name: &str) -> RuntimeTy {
        RuntimeTy::TypeAlias(baml_type::TypeName::local(name.into()), TyAttr::default())
    }

    #[test]
    fn test_self_referential_union() {
        let mut content =
            OutputFormatContent::new(ty_class("SelfReferential")).with_class(mk_class(
                "SelfReferential",
                vec![(
                    "recursion",
                    ty_union(vec![
                        ty_int(),
                        ty_string(),
                        ty_optional(ty_class("SelfReferential")),
                    ]),
                )],
            ));
        content.recursive_classes = mk_recursive(&["SelfReferential"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
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
    fn test_render_simple_recursive_alias() {
        let mut content = OutputFormatContent::new(ty_alias("RecursiveMapAlias"));
        content.recursive_type_aliases.insert(
            "RecursiveMapAlias".to_string(),
            ty_map(ty_string(), ty_alias("RecursiveMapAlias")),
        );

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"RecursiveMapAlias = { "<string>": RecursiveMapAlias }

Answer in JSON using this schema: RecursiveMapAlias"#
            ))
        );
    }

    #[test]
    fn test_render_recursive_alias_cycle() {
        // A = B[], B = C, C = A[]
        let mut content = OutputFormatContent::new(ty_alias("A"));
        content
            .recursive_type_aliases
            .insert("A".to_string(), ty_list(ty_alias("B")));
        content
            .recursive_type_aliases
            .insert("B".to_string(), ty_alias("C"));
        content
            .recursive_type_aliases
            .insert("C".to_string(), ty_list(ty_alias("A")));

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"A = B[]
B = C
C = A[]

Answer in JSON using this schema: A"#
            ))
        );
    }

    #[test]
    fn test_render_recursive_alias_cycle_with_hoist_prefix() {
        let mut content = OutputFormatContent::new(ty_alias("A"));
        content
            .recursive_type_aliases
            .insert("A".to_string(), ty_list(ty_alias("B")));
        content
            .recursive_type_aliases
            .insert("B".to_string(), ty_alias("C"));
        content
            .recursive_type_aliases
            .insert("C".to_string(), ty_list(ty_alias("A")));

        let options = RenderOptions {
            hoisted_class_prefix: RenderSetting::Always("type".to_string()),
            ..Default::default()
        };
        let rendered = content.render(&options).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"type A = B[]
type B = C
type C = A[]

Answer in JSON using this type: A"#
            ))
        );
    }

    #[test]
    fn test_build_output_format_preserves_exact_recursive_generic() {
        let chain = baml_type::TypeName::local("Chain".into());
        let target = RuntimeTy::Class(chain.clone(), vec![ty_int()], TyAttr::default());
        let next_template =
            baml_type::TyTemplate::class(chain.clone(), vec![baml_type::TyTemplate::TypeArgRef(0)]);

        let mut classes = indexmap::IndexMap::new();
        classes.insert(
            chain.clone(),
            ctx_class_definition(
                &chain,
                vec![ctx_class_field("next", target.clone(), Some(next_template))],
            ),
        );

        let mut ctx = sys_types::SysOpContext::empty();
        ctx.class_definitions = Arc::new(classes);

        let content = build_output_format_content(&target, &ctx);
        let rendered = content.render(&RenderOptions::default()).unwrap().unwrap();
        assert!(rendered.contains("Chain<int> {\n  next: Chain<int>,\n}"));
    }

    #[test]
    fn test_build_output_format_preserves_finite_nested_generic() {
        let boxed = baml_type::TypeName::local("Box".into());
        let box_int = RuntimeTy::Class(boxed.clone(), vec![ty_int()], TyAttr::default());
        let target = RuntimeTy::Class(boxed.clone(), vec![box_int], TyAttr::default());

        let mut classes = indexmap::IndexMap::new();
        classes.insert(
            boxed.clone(),
            ctx_class_definition(
                &boxed,
                vec![ctx_class_field(
                    "value",
                    ty_int(),
                    Some(baml_type::TyTemplate::TypeArgRef(0)),
                )],
            ),
        );

        let mut ctx = sys_types::SysOpContext::empty();
        ctx.class_definitions = Arc::new(classes);

        let content = build_output_format_content(&target, &ctx);
        assert!(content.classes.contains_key("Box<Box<int>>"));
        assert!(content.classes.contains_key("Box<int>"));
        assert!(content.render(&RenderOptions::default()).is_ok());
    }

    #[test]
    fn test_build_output_format_preserves_finite_transformed_recursion() {
        let step = baml_type::TypeName::local("Step".into());
        let target = RuntimeTy::Class(
            step.clone(),
            vec![ty_string(), ty_bool()],
            TyAttr::default(),
        );
        let next_template = baml_type::TyTemplate::class(
            step.clone(),
            vec![
                baml_type::TyTemplate::list(baml_type::TyTemplate::TypeArgRef(1)),
                baml_type::TyTemplate::from(baml_type::RealizedTy::int()),
            ],
        );
        let next_realized = RuntimeTy::Class(
            step.clone(),
            vec![ty_list(ty_bool()), ty_int()],
            TyAttr::default(),
        );

        let mut classes = indexmap::IndexMap::new();
        classes.insert(
            step.clone(),
            ctx_class_definition(
                &step,
                vec![ctx_class_field("next", next_realized, Some(next_template))],
            ),
        );

        let mut ctx = sys_types::SysOpContext::empty();
        ctx.class_definitions = Arc::new(classes);

        let content = build_output_format_content(&target, &ctx);
        assert!(content.classes.contains_key("Step<string, bool>"));
        assert!(content.classes.contains_key("Step<bool[], int>"));
        assert!(content.classes.contains_key("Step<int[], int>"));
        assert!(content.render(&RenderOptions::default()).is_ok());
    }

    #[test]
    fn test_render_output_format_content_rejects_non_regular_recursive_generic() {
        let chain = baml_type::TypeName::local("Chain".into());
        let target = RuntimeTy::Class(chain.clone(), vec![ty_int()], TyAttr::default());
        let next_template = baml_type::TyTemplate::class(
            chain.clone(),
            vec![baml_type::TyTemplate::class(
                chain.clone(),
                vec![baml_type::TyTemplate::TypeArgRef(0)],
            )],
        );
        let next_realized = RuntimeTy::Class(
            chain.clone(),
            vec![RuntimeTy::Class(
                chain.clone(),
                vec![ty_int()],
                TyAttr::default(),
            )],
            TyAttr::default(),
        );

        let mut classes = indexmap::IndexMap::new();
        classes.insert(
            chain.clone(),
            ctx_class_definition(
                &chain,
                vec![ctx_class_field("next", next_realized, Some(next_template))],
            ),
        );

        let mut ctx = sys_types::SysOpContext::empty();
        ctx.class_definitions = Arc::new(classes);

        let content = build_output_format_content(&target, &ctx);
        let error = render_output_format_content(
            &content, None, None, None, None, None, None, None, None, None,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            RenderError::NonRegularRecursiveGeneric {
                class,
                ancestor,
                instantiation,
            } if class == "Chain"
                && ancestor == "Chain<int>"
                && instantiation == "Chain<Chain<int>>"
        ));
    }

    #[test]
    fn test_build_output_format_rejects_mutually_expansive_recursive_generic() {
        let a = baml_type::TypeName::local("A".into());
        let b = baml_type::TypeName::local("B".into());
        let target = RuntimeTy::Class(a.clone(), vec![ty_int()], TyAttr::default());
        let b_int = RuntimeTy::Class(b.clone(), vec![ty_int()], TyAttr::default());
        let a_a_int = RuntimeTy::Class(
            a.clone(),
            vec![RuntimeTy::Class(
                a.clone(),
                vec![ty_int()],
                TyAttr::default(),
            )],
            TyAttr::default(),
        );

        let mut classes = indexmap::IndexMap::new();
        classes.insert(
            a.clone(),
            ctx_class_definition(
                &a,
                vec![ctx_class_field(
                    "b",
                    b_int,
                    Some(baml_type::TyTemplate::class(
                        b.clone(),
                        vec![baml_type::TyTemplate::TypeArgRef(0)],
                    )),
                )],
            ),
        );
        classes.insert(
            b.clone(),
            ctx_class_definition(
                &b,
                vec![ctx_class_field(
                    "a",
                    a_a_int,
                    Some(baml_type::TyTemplate::class(
                        a.clone(),
                        vec![baml_type::TyTemplate::class(
                            a,
                            vec![baml_type::TyTemplate::TypeArgRef(0)],
                        )],
                    )),
                )],
            ),
        );

        let mut ctx = sys_types::SysOpContext::empty();
        ctx.class_definitions = Arc::new(classes);

        let content = build_output_format_content(&target, &ctx);
        let error = content.render(&RenderOptions::default()).unwrap_err();
        assert!(matches!(
            error,
            RenderError::NonRegularRecursiveGeneric {
                class,
                ancestor,
                instantiation,
            } if class == "A" && ancestor == "A<int>" && instantiation == "A<A<int>>"
        ));
    }

    // ========================================================================
    // Enum hoisting and inline rendering
    // ========================================================================

    #[test]
    fn inline_enum_in_class_field() {
        // Enum with <=6 values and no descriptions → renders inline in field type
        let enm = mk_enum("Color", vec!["Red", "Green", "Blue"]);
        let cls = mk_class(
            "Item",
            vec![("name", ty_string()), ("color", ty_enum("Color"))],
        );
        let content = OutputFormatContent::new(ty_class("Item"))
            .with_class(cls)
            .with_enum(enm);
        let rendered = content.render(&RenderOptions::default()).unwrap().unwrap();
        assert!(
            rendered.contains("color: 'Red' or 'Green' or 'Blue',"),
            "Expected inline enum rendering, got:\n{rendered}"
        );
    }

    #[test]
    fn hoisted_enum_in_class_field_renders_as_name() {
        // Enum with >6 values → hoisted, field renders just the name
        let enm = Enum {
            name: "BigEnum".to_string(),
            alias: None,
            description: None,
            values: (0..8)
                .map(|i| EnumValue {
                    name: format!("V{i}"),
                    alias: None,
                    description: None,
                })
                .collect(),
        };
        let cls = mk_class("Item", vec![("val", ty_enum("BigEnum"))]);
        let content = OutputFormatContent::new(ty_class("Item"))
            .with_class(cls)
            .with_enum(enm);
        let rendered = content.render(&RenderOptions::default()).unwrap().unwrap();
        // Field should use just the name, not inline values
        assert!(
            rendered.contains("val: BigEnum,"),
            "Expected hoisted enum name in field, got:\n{rendered}"
        );
        // Enum definition should appear above
        assert!(
            rendered.contains("BigEnum\n----"),
            "Expected hoisted enum block, got:\n{rendered}"
        );
    }

    #[test]
    fn enum_description_triggers_hoisting() {
        // Enum with @@description but <=6 values → hoisted because of description
        let enm = Enum {
            name: "Status".to_string(),
            alias: None,
            description: Some("The status of an order".to_string()),
            values: vec![
                EnumValue {
                    name: "Pending".to_string(),
                    alias: None,
                    description: None,
                },
                EnumValue {
                    name: "Done".to_string(),
                    alias: None,
                    description: None,
                },
            ],
        };
        let cls = mk_class("Order", vec![("status", ty_enum("Status"))]);
        let content = OutputFormatContent::new(ty_class("Order"))
            .with_class(cls)
            .with_enum(enm);
        let rendered = content.render(&RenderOptions::default()).unwrap().unwrap();
        // Should be hoisted and description rendered above
        assert!(
            rendered.contains("/// The status of an order\nStatus\n----"),
            "Expected hoisted enum with description, got:\n{rendered}"
        );
        // Field should reference by name
        assert!(
            rendered.contains("status: Status,"),
            "Expected enum name in field, got:\n{rendered}"
        );
    }

    #[test]
    fn field_alias_in_class() {
        let cls = Class {
            name: "User".to_string(),
            alias: None,
            description: None,
            fields: vec![
                ClassField {
                    name: "user_name".to_string(),
                    alias: Some("username".to_string()),
                    field_type: ty_string(),
                    description: None,
                },
                ClassField {
                    name: "email_addr".to_string(),
                    alias: Some("email".to_string()),
                    field_type: ty_string(),
                    description: None,
                },
            ],
        };
        let content = OutputFormatContent::new(ty_class("User")).with_class(cls);
        let rendered = content.render(&RenderOptions::default()).unwrap().unwrap();
        assert!(
            rendered.contains("username: string,"),
            "Expected field alias 'username', got:\n{rendered}"
        );
        assert!(
            rendered.contains("email: string,"),
            "Expected field alias 'email', got:\n{rendered}"
        );
        assert!(
            !rendered.contains("user_name"),
            "Original field name should not appear"
        );
    }

    #[test]
    fn enum_variant_alias_inline() {
        // Inline enum with variant aliases
        let enm = Enum {
            name: "Color".to_string(),
            alias: None,
            description: None,
            values: vec![
                EnumValue {
                    name: "Red".to_string(),
                    alias: Some("r".to_string()),
                    description: None,
                },
                EnumValue {
                    name: "Green".to_string(),
                    alias: Some("g".to_string()),
                    description: None,
                },
            ],
        };
        let cls = mk_class("Item", vec![("color", ty_enum("Color"))]);
        let content = OutputFormatContent::new(ty_class("Item"))
            .with_class(cls)
            .with_enum(enm);
        let rendered = content.render(&RenderOptions::default()).unwrap().unwrap();
        assert!(
            rendered.contains("color: 'r' or 'g',"),
            "Expected aliased variant names in inline enum, got:\n{rendered}"
        );
    }

    #[test]
    fn multiline_list_of_class() {
        // List<Class> should render multiline when class is not hoisted
        let cls = mk_class("Point", vec![("x", ty_int()), ("y", ty_int())]);
        let content = OutputFormatContent::new(ty_list(ty_class("Point"))).with_class(cls);
        let rendered = content.render(&RenderOptions::default()).unwrap().unwrap();
        assert!(
            rendered.contains("[\n  {\n"),
            "Expected multiline list rendering, got:\n{rendered}"
        );
    }

    #[test]
    fn target_enum_renders_block_format() {
        // When target IS an enum, it should render full block format, not inline
        let enm = mk_enum("Sentiment", vec!["Positive", "Negative", "Neutral"]);
        let content = OutputFormatContent::new(ty_enum("Sentiment")).with_enum(enm);
        let rendered = content.render(&RenderOptions::default()).unwrap().unwrap();
        assert!(
            rendered.contains("Sentiment\n----\n- Positive\n- Negative\n- Neutral"),
            "Target enum should render in block format, got:\n{rendered}"
        );
    }

    #[test]
    fn field_description_above_field() {
        // Field descriptions should render as /// comment above the field line
        let cls = Class {
            name: "User".to_string(),
            alias: None,
            description: None,
            fields: vec![
                ClassField {
                    name: "name".to_string(),
                    alias: None,
                    field_type: ty_string(),
                    description: Some("The user's full name".to_string()),
                },
                ClassField {
                    name: "age".to_string(),
                    alias: None,
                    field_type: ty_int(),
                    description: None,
                },
            ],
        };
        let content = OutputFormatContent::new(ty_class("User")).with_class(cls);
        let rendered = content.render(&RenderOptions::default()).unwrap().unwrap();
        // Description on line before, field on next line
        assert!(
            rendered.contains("  /// The user's full name\n  name: string,"),
            "Expected description above field, got:\n{rendered}"
        );
        // Field without description: no /// line
        assert!(
            !rendered.contains("/// \n  age"),
            "Field without description should have no comment"
        );
    }
}
