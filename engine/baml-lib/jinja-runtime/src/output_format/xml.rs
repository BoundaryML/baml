use std::collections::HashSet;

use baml_types::{TypeIR, TypeValue};
use indexmap::IndexSet;

use super::types::{HoistClasses, OutputFormatContent, RenderOptions, RenderSetting};

struct XmlRenderContext {
    hoisted_enums: HashSet<String>,
    hoisted_classes: IndexSet<String>,
    visiting_classes: HashSet<String>,
    visiting_aliases: HashSet<String>,
}

pub(super) fn render(
    content: &OutputFormatContent,
    options: &RenderOptions,
) -> Result<Option<String>, minijinja::Error> {
    let mut ctx = render_context(content, options)?;
    let body = render_root_type(content, &content.target, options, &mut ctx)?;
    let prefix = match &options.prefix {
        RenderSetting::Always(prefix) => prefix.clone(),
        RenderSetting::Never => String::new(),
        RenderSetting::Auto => default_prefix(&content.target).to_string(),
    };

    let mut definitions = Vec::new();
    for enm in content.enums.values() {
        if ctx.hoisted_enums.contains(enm.name.real_name()) {
            definitions.push(render_enum_definition(enm, options));
        }
    }

    let root_class = match &content.target {
        TypeIR::Class { name, .. } => Some(name.as_str()),
        _ => None,
    };
    for class_name in ctx.hoisted_classes.clone() {
        // A root class is already its own complete XML structure. Its recursive
        // occurrences are references, but duplicating the root definition above
        // the instruction would make the requested output ambiguous.
        if root_class == Some(class_name.as_str()) {
            continue;
        }

        let class = content
            .classes
            .iter()
            .find_map(|((name, _), class)| (name == &class_name).then_some(class))
            .ok_or_else(|| serialization_error(anyhow::anyhow!("Class {class_name} not found")))?;
        let definition = render_class_block(
            content,
            &class_name,
            &class.namespace,
            0,
            true,
            options,
            &mut ctx,
        )?;
        definitions.push(match &options.hoisted_class_prefix {
            RenderSetting::Always(label) if !label.is_empty() => {
                format!("{label} {}\n{definition}", class.name.rendered_name())
            }
            RenderSetting::Auto | RenderSetting::Never | RenderSetting::Always(_) => definition,
        });
    }

    let rendered = if definitions.is_empty() {
        format!("{prefix}{body}")
    } else {
        format!("{}\n\n{prefix}{body}", definitions.join("\n\n"))
    };

    Ok((!rendered.is_empty()).then_some(rendered))
}

fn render_context(
    content: &OutputFormatContent,
    options: &RenderOptions,
) -> Result<XmlRenderContext, minijinja::Error> {
    let hoisted_enums = if matches!(options.always_hoist_enums, RenderSetting::Always(true)) {
        content.enums.keys().cloned().collect()
    } else {
        HashSet::new()
    };

    let mut hoisted_classes = content
        .recursive_classes
        .iter()
        .cloned()
        .collect::<IndexSet<_>>();
    match &options.hoist_classes {
        HoistClasses::Auto => {}
        HoistClasses::All => {
            hoisted_classes.extend(content.classes.keys().map(|(name, _)| name.clone()));
        }
        HoistClasses::Subset(classes) => {
            let mut not_found = IndexSet::new();
            for class_name in classes {
                if content.classes.keys().any(|(name, _)| name == class_name) {
                    hoisted_classes.insert(class_name.clone());
                } else {
                    not_found.insert(class_name.clone());
                }
            }
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
                            .map(|class| format!("\"{class}\""))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
            }
        }
    }

    Ok(XmlRenderContext {
        hoisted_enums,
        hoisted_classes,
        visiting_classes: HashSet::new(),
        visiting_aliases: HashSet::new(),
    })
}

fn default_prefix(target: &TypeIR) -> &'static str {
    match target {
        TypeIR::Class { .. } | TypeIR::List(_, _) | TypeIR::Map(_, _, _) => {
            "Answer in XML using this structure:\n"
        }
        TypeIR::Union(items, _) if has_null(items.iter_include_null().iter().copied()) => {
            "Answer in XML using this structure:\n"
        }
        TypeIR::Union(_, _) => "Answer in XML using any of these structures:\n",
        TypeIR::Enum { .. } => "Answer in XML using any of these values:\n",
        TypeIR::RecursiveTypeAlias { .. } => "Answer in XML using this structure:\n",
        TypeIR::Primitive(_, _) | TypeIR::Literal(_, _) => "Answer as XML:",
        TypeIR::Tuple(_, _) | TypeIR::Arrow(_, _) => "",
        TypeIR::Top(_) => panic!(
            "TypeIR::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the type resolution phase."
        ),
    }
}

fn render_root_type(
    content: &OutputFormatContent,
    target: &TypeIR,
    options: &RenderOptions,
    ctx: &mut XmlRenderContext,
) -> Result<String, minijinja::Error> {
    Ok(match target {
        TypeIR::Primitive(_, _) | TypeIR::Literal(_, _) => leaf(
            "",
            "value",
            &scalar_text(content, target, options, ctx)?.expect("scalar"),
        ),
        TypeIR::Enum { name, .. } => {
            let enm = content.find_enum(name).map_err(serialization_error)?;
            leaf(
                "",
                enm.name.rendered_name(),
                &scalar_text(content, target, options, ctx)?.expect("enum"),
            )
        }
        TypeIR::Class { name, mode, .. } => {
            render_class_block(content, name, mode, 0, true, options, ctx)?
        }
        TypeIR::List(item, _) => render_top_level_list(content, item, options, ctx)?,
        TypeIR::Map(key, value, _) => render_map(content, key, value, "map", 0, options, ctx)?,
        TypeIR::Union(items, _) => {
            let variants = items.iter_include_null();
            render_root_union(content, &variants, options, ctx)?
        }
        TypeIR::RecursiveTypeAlias { name, .. } => {
            if !ctx.visiting_aliases.insert(name.clone()) {
                leaf("", "value", name)
            } else {
                let alias_target = content
                    .find_recursive_alias_target(name)
                    .map_err(serialization_error)?;
                let rendered = render_root_type(content, alias_target, options, ctx)?;
                ctx.visiting_aliases.remove(name);
                rendered
            }
        }
        TypeIR::Tuple(_, _) => return Err(unsupported("Tuple type is not supported in outputs")),
        TypeIR::Arrow(_, _) => {
            return Err(unsupported(
                "Arrow type is not supported in LLM function outputs",
            ));
        }
        TypeIR::Top(_) => panic!(
            "TypeIR::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the type resolution phase."
        ),
    })
}

fn render_class_block(
    content: &OutputFormatContent,
    name: &str,
    mode: &baml_types::StreamingMode,
    depth: usize,
    force_expand: bool,
    options: &RenderOptions,
    ctx: &mut XmlRenderContext,
) -> Result<String, minijinja::Error> {
    let class = content
        .find_class(mode, name)
        .map_err(serialization_error)?;
    let class_tag = class.name.rendered_name();
    let indent = "  ".repeat(depth);

    if !force_expand && ctx.hoisted_classes.contains(name) {
        return Ok(leaf(&indent, class_tag, class_tag));
    }
    if !ctx.visiting_classes.insert(name.to_string()) {
        return Ok(leaf(&indent, class_tag, class_tag));
    }

    let mut field_tags = HashSet::with_capacity(class.fields.len());
    for (field_name, _, _, _) in &class.fields {
        let field_tag = xml_tag_name(field_name.rendered_name());
        if !field_tags.insert(field_tag.clone()) {
            return Err(unsupported(&format!(
                "Class {name} cannot be rendered as XML because multiple fields use the <{field_tag}> tag"
            )));
        }
    }

    let mut fields = class
        .description
        .as_deref()
        .map(|description| xml_comments(description, depth + 1))
        .unwrap_or_default();
    for (field_name, field_type, description, _) in &class.fields {
        if let Some(description) = description {
            fields.extend(xml_comments(description, depth + 1));
        }
        fields.push(render_field(
            content,
            field_type,
            field_name.rendered_name(),
            depth + 1,
            options,
            ctx,
        )?);
    }
    ctx.visiting_classes.remove(name);

    Ok(container(&indent, class_tag, &fields.join("\n")))
}

fn render_field(
    content: &OutputFormatContent,
    target: &TypeIR,
    field_name: &str,
    depth: usize,
    options: &RenderOptions,
    ctx: &mut XmlRenderContext,
) -> Result<String, minijinja::Error> {
    if let TypeIR::Union(items, _) = target {
        let variants = items.iter_include_null();
        if has_null(variants.iter().copied()) {
            return render_optional_field(content, &variants, field_name, depth, options, ctx);
        }
        if let Some(text) = scalar_union_text(content, &variants, options, ctx)? {
            return Ok(leaf(&"  ".repeat(depth), field_name, &text));
        }
        let body = render_union_blocks(content, &variants, depth + 1, "value", options, ctx)?;
        return Ok(container(&"  ".repeat(depth), field_name, &body));
    }

    if let Some(text) = scalar_text(content, target, options, ctx)? {
        return Ok(leaf(&"  ".repeat(depth), field_name, &text));
    }

    Ok(match target {
        TypeIR::Class { name, mode, .. } => {
            let block = render_class_block(content, name, mode, depth + 1, false, options, ctx)?;
            container(&"  ".repeat(depth), field_name, &block)
        }
        TypeIR::List(item, _) => render_field_list(content, item, field_name, depth, options, ctx)?,
        TypeIR::Map(key, value, _) => {
            render_map(content, key, value, field_name, depth, options, ctx)?
        }
        TypeIR::RecursiveTypeAlias { name, .. } => {
            if !ctx.visiting_aliases.insert(name.clone()) {
                leaf(&"  ".repeat(depth), field_name, name)
            } else {
                let alias_target = content
                    .find_recursive_alias_target(name)
                    .map_err(serialization_error)?;
                let rendered =
                    render_field(content, alias_target, field_name, depth, options, ctx)?;
                ctx.visiting_aliases.remove(name);
                rendered
            }
        }
        TypeIR::Union(_, _) => unreachable!("unions handled above"),
        TypeIR::Tuple(_, _) => return Err(unsupported("Tuple type is not supported in outputs")),
        TypeIR::Arrow(_, _) => {
            return Err(unsupported(
                "Arrow type is not supported in LLM function outputs",
            ));
        }
        TypeIR::Top(_) => panic!(
            "TypeIR::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the type resolution phase."
        ),
        TypeIR::Primitive(_, _) | TypeIR::Literal(_, _) | TypeIR::Enum { .. } => {
            unreachable!("scalar types handled above")
        }
    })
}

fn render_optional_field(
    content: &OutputFormatContent,
    variants: &[&TypeIR],
    field_name: &str,
    depth: usize,
    options: &RenderOptions,
    ctx: &mut XmlRenderContext,
) -> Result<String, minijinja::Error> {
    let non_null = variants
        .iter()
        .copied()
        .filter(|variant| !matches!(variant, TypeIR::Primitive(TypeValue::Null, _)))
        .collect::<Vec<_>>();
    let null = null_text(options);

    if non_null.is_empty() {
        return Ok(leaf(&"  ".repeat(depth), field_name, null));
    }
    if let Some(mut text) = scalar_union_text(content, &non_null, options, ctx)? {
        text.push_str(&options.or_splitter);
        text.push_str(null);
        return Ok(leaf(&"  ".repeat(depth), field_name, &text));
    }

    let rendered = if non_null.len() == 1 {
        if matches!(non_null[0], TypeIR::Map(_, _, _)) {
            let body =
                render_union_variant(content, non_null[0], depth + 1, "value", options, ctx)?;
            container(&"  ".repeat(depth), field_name, &body)
        } else {
            render_field(content, non_null[0], field_name, depth, options, ctx)?
        }
    } else {
        let body = render_union_blocks(content, &non_null, depth + 1, "value", options, ctx)?;
        container(&"  ".repeat(depth), field_name, &body)
    };
    Ok(append_to_field_content(
        rendered,
        field_name,
        depth,
        &format!("{}{null}", options.or_splitter),
    ))
}

fn render_root_union(
    content: &OutputFormatContent,
    variants: &[&TypeIR],
    options: &RenderOptions,
    ctx: &mut XmlRenderContext,
) -> Result<String, minijinja::Error> {
    let has_null_variant = has_null(variants.iter().copied());
    let non_null = variants
        .iter()
        .copied()
        .filter(|variant| !matches!(variant, TypeIR::Primitive(TypeValue::Null, _)))
        .collect::<Vec<_>>();

    if let Some(mut text) = scalar_union_text(content, &non_null, options, ctx)? {
        if has_null_variant {
            text.push_str(&options.or_splitter);
            text.push_str(null_text(options));
        }
        return Ok(leaf("", "value", &text));
    }

    let mut rendered = render_union_blocks(content, &non_null, 0, "value", options, ctx)?;
    if has_null_variant {
        rendered.push_str(&options.or_splitter);
        rendered.push_str(null_text(options));
    }
    Ok(rendered)
}

fn render_union_blocks(
    content: &OutputFormatContent,
    variants: &[&TypeIR],
    depth: usize,
    fallback_tag: &str,
    options: &RenderOptions,
    ctx: &mut XmlRenderContext,
) -> Result<String, minijinja::Error> {
    let indent = "  ".repeat(depth);
    variants
        .iter()
        .map(|variant| render_union_variant(content, variant, depth, fallback_tag, options, ctx))
        .collect::<Result<Vec<_>, _>>()
        .map(|variants| variants.join(&format!("\n\n{indent}OR\n\n")))
}

fn render_union_variant(
    content: &OutputFormatContent,
    variant: &TypeIR,
    depth: usize,
    fallback_tag: &str,
    options: &RenderOptions,
    ctx: &mut XmlRenderContext,
) -> Result<String, minijinja::Error> {
    Ok(match variant {
        TypeIR::Class { name, mode, .. } => {
            render_class_block(content, name, mode, depth, false, options, ctx)?
        }
        TypeIR::Enum { name, .. } => {
            let enm = content.find_enum(name).map_err(serialization_error)?;
            leaf(
                &"  ".repeat(depth),
                enm.name.rendered_name(),
                &scalar_text(content, variant, options, ctx)?.expect("enum"),
            )
        }
        TypeIR::List(item, _) => {
            let rendered = render_top_level_list(content, item, options, ctx)?;
            indent_block(&rendered, depth)
        }
        TypeIR::Map(key, value, _) => render_map(content, key, value, "map", depth, options, ctx)?,
        TypeIR::RecursiveTypeAlias { name, .. } => {
            if !ctx.visiting_aliases.insert(name.clone()) {
                leaf(&"  ".repeat(depth), fallback_tag, name)
            } else {
                let alias_target = content
                    .find_recursive_alias_target(name)
                    .map_err(serialization_error)?;
                let rendered =
                    render_union_variant(content, alias_target, depth, fallback_tag, options, ctx)?;
                ctx.visiting_aliases.remove(name);
                rendered
            }
        }
        TypeIR::Union(items, _) => {
            let variants = items.iter_include_null();
            if let Some(text) = scalar_union_text(content, &variants, options, ctx)? {
                leaf(&"  ".repeat(depth), fallback_tag, &text)
            } else {
                render_union_blocks(content, &variants, depth, fallback_tag, options, ctx)?
            }
        }
        TypeIR::Tuple(_, _) => return Err(unsupported("Tuple type is not supported in outputs")),
        TypeIR::Arrow(_, _) => {
            return Err(unsupported(
                "Arrow type is not supported in LLM function outputs",
            ));
        }
        TypeIR::Top(_) => {
            return Err(unsupported("Top type is not supported in outputs"));
        }
        TypeIR::Primitive(_, _) | TypeIR::Literal(_, _) => leaf(
            &"  ".repeat(depth),
            fallback_tag,
            &scalar_text(content, variant, options, ctx)?.expect("scalar"),
        ),
    })
}

fn render_top_level_list(
    content: &OutputFormatContent,
    item: &TypeIR,
    options: &RenderOptions,
    ctx: &mut XmlRenderContext,
) -> Result<String, minijinja::Error> {
    if let TypeIR::Class { name, mode, .. } = item {
        let class = content
            .find_class(mode, name)
            .map_err(serialization_error)?;
        let wrapper = format!("{}List", class.name.rendered_name());
        let element = render_class_block(content, name, mode, 1, false, options, ctx)?;
        return Ok(container("", &wrapper, &element));
    }

    let item = render_list_item(content, item, 1, options, ctx)?;
    Ok(container("", "list", &item))
}

fn render_field_list(
    content: &OutputFormatContent,
    item: &TypeIR,
    field_name: &str,
    depth: usize,
    options: &RenderOptions,
    ctx: &mut XmlRenderContext,
) -> Result<String, minijinja::Error> {
    let indent = "  ".repeat(depth);
    if let TypeIR::Class { name, mode, .. } = item {
        let class = content
            .find_class(mode, name)
            .map_err(serialization_error)?;
        let list_indent = "  ".repeat(depth + 1);
        let wrapper = format!("{}List", class.name.rendered_name());
        let element = render_class_block(content, name, mode, depth + 2, false, options, ctx)?;
        return Ok(container(
            &indent,
            field_name,
            &container(&list_indent, &wrapper, &element),
        ));
    }

    let item = render_list_item(content, item, depth + 1, options, ctx)?;
    Ok(container(&indent, field_name, &item))
}

fn render_list_item(
    content: &OutputFormatContent,
    item: &TypeIR,
    depth: usize,
    options: &RenderOptions,
    ctx: &mut XmlRenderContext,
) -> Result<String, minijinja::Error> {
    if let TypeIR::Union(items, _) = item {
        let variants = items.iter_include_null();
        if let Some(text) = scalar_union_text(content, &variants, options, ctx)? {
            return Ok(leaf(&"  ".repeat(depth), "item", &text));
        }
        return render_union_blocks(content, &variants, depth, "item", options, ctx);
    }
    if let Some(text) = scalar_text(content, item, options, ctx)? {
        return Ok(leaf(&"  ".repeat(depth), "item", &text));
    }

    Ok(match item {
        TypeIR::Class { name, mode, .. } => {
            render_class_block(content, name, mode, depth, false, options, ctx)?
        }
        TypeIR::List(inner, _) => {
            let child = if let TypeIR::Class { name, mode, .. } = inner.as_ref() {
                let class = content
                    .find_class(mode, name)
                    .map_err(serialization_error)?;
                let wrapper = format!("{}List", class.name.rendered_name());
                let element =
                    render_class_block(content, name, mode, depth + 2, false, options, ctx)?;
                container(&"  ".repeat(depth + 1), &wrapper, &element)
            } else {
                render_list_item(content, inner, depth + 1, options, ctx)?
            };
            container(&"  ".repeat(depth), "item", &child)
        }
        TypeIR::Map(key, value, _) => render_map(content, key, value, "item", depth, options, ctx)?,
        TypeIR::RecursiveTypeAlias { name, .. } => {
            if !ctx.visiting_aliases.insert(name.clone()) {
                leaf(&"  ".repeat(depth), "item", name)
            } else {
                let target = content
                    .find_recursive_alias_target(name)
                    .map_err(serialization_error)?;
                let rendered = render_list_item(content, target, depth, options, ctx)?;
                ctx.visiting_aliases.remove(name);
                rendered
            }
        }
        TypeIR::Union(_, _) => unreachable!("unions handled above"),
        TypeIR::Tuple(_, _) => return Err(unsupported("Tuple type is not supported in outputs")),
        TypeIR::Arrow(_, _) => {
            return Err(unsupported(
                "Arrow type is not supported in LLM function outputs",
            ));
        }
        TypeIR::Top(_) => panic!(
            "TypeIR::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the type resolution phase."
        ),
        TypeIR::Primitive(_, _) | TypeIR::Literal(_, _) | TypeIR::Enum { .. } => {
            unreachable!("scalar types handled above")
        }
    })
}

fn render_map(
    content: &OutputFormatContent,
    key: &TypeIR,
    value: &TypeIR,
    tag: &str,
    depth: usize,
    options: &RenderOptions,
    ctx: &mut XmlRenderContext,
) -> Result<String, minijinja::Error> {
    let indent = "  ".repeat(depth);
    let entry_indent = "  ".repeat(depth + 1);
    let key = render_field(content, key, "key", depth + 2, options, ctx)?;
    let value = render_field(content, value, "value", depth + 2, options, ctx)?;
    let entry = container(&entry_indent, "entry", &format!("{key}\n{value}"));
    Ok(container(&indent, tag, &entry))
}

fn scalar_union_text(
    content: &OutputFormatContent,
    variants: &[&TypeIR],
    options: &RenderOptions,
    ctx: &XmlRenderContext,
) -> Result<Option<String>, minijinja::Error> {
    variants
        .iter()
        .map(|variant| scalar_text(content, variant, options, ctx))
        .collect::<Result<Option<Vec<_>>, _>>()
        .map(|values| values.map(|values| values.join(&options.or_splitter)))
}

fn scalar_text(
    content: &OutputFormatContent,
    target: &TypeIR,
    options: &RenderOptions,
    ctx: &XmlRenderContext,
) -> Result<Option<String>, minijinja::Error> {
    Ok(match target {
        TypeIR::Primitive(value, _) => Some(match value {
            TypeValue::String => "string".to_string(),
            TypeValue::Int => "int".to_string(),
            TypeValue::Float => "float".to_string(),
            TypeValue::Bool => "bool".to_string(),
            TypeValue::Null => null_text(options).to_string(),
            TypeValue::Media(media_type) => {
                return Err(unsupported(&format!(
                    "type '{media_type}' is not supported in outputs"
                )));
            }
        }),
        TypeIR::Literal(value, _) => Some(value.to_string().trim_matches('"').to_string()),
        TypeIR::Enum { name, .. } => {
            let enm = content.find_enum(name).map_err(serialization_error)?;
            Some(if ctx.hoisted_enums.contains(name) {
                enm.name.rendered_name().to_string()
            } else {
                enm.values
                    .iter()
                    .map(|(name, _)| name.rendered_name())
                    .collect::<Vec<_>>()
                    .join(" | ")
            })
        }
        TypeIR::Class { name, mode, .. } if ctx.hoisted_classes.contains(name) => Some(
            content
                .find_class(mode, name)
                .map_err(serialization_error)?
                .name
                .rendered_name()
                .to_string(),
        ),
        TypeIR::RecursiveTypeAlias { name, .. } if ctx.visiting_aliases.contains(name) => {
            Some(name.clone())
        }
        _ => None,
    })
}

fn render_enum_definition(enm: &super::types::Enum, options: &RenderOptions) -> String {
    let value_prefix = match &options.enum_value_prefix {
        RenderSetting::Auto => "- ",
        RenderSetting::Always(prefix) => prefix,
        RenderSetting::Never => "",
    };
    let values = enm
        .values
        .iter()
        .map(|(name, description)| match description {
            Some(description) => {
                format!("{value_prefix}{}: {description}", name.rendered_name())
            }
            None => format!("{value_prefix}{}", name.rendered_name()),
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}\n----\n{values}", enm.name.rendered_name())
}

fn null_text(options: &RenderOptions) -> &str {
    match &options.render_null_as {
        RenderSetting::Always(value) => value,
        RenderSetting::Auto | RenderSetting::Never => "null",
    }
}

fn has_null<'a>(mut variants: impl Iterator<Item = &'a TypeIR>) -> bool {
    variants.any(|variant| matches!(variant, TypeIR::Primitive(TypeValue::Null, _)))
}

fn container(indent: &str, tag: &str, body: &str) -> String {
    let tag = xml_tag_name(tag);
    if body.is_empty() {
        format!("{indent}<{tag}></{tag}>")
    } else {
        format!("{indent}<{tag}>\n{body}\n{indent}</{tag}>")
    }
}

fn leaf(indent: &str, tag: &str, value: &str) -> String {
    let tag = xml_tag_name(tag);
    format!("{indent}<{tag}>{}</{tag}>", escape_xml_text(value))
}

fn append_to_field_content(
    mut rendered: String,
    field_name: &str,
    depth: usize,
    suffix: &str,
) -> String {
    let suffix = escape_xml_text(suffix);
    let closing = format!("\n{}</{}>", "  ".repeat(depth), xml_tag_name(field_name));
    if let Some(closing_start) = rendered.rfind(&closing) {
        rendered.insert_str(closing_start, &suffix);
        return rendered;
    }

    let closing = format!("</{}>", xml_tag_name(field_name));
    if let Some(closing_start) = rendered.rfind(&closing) {
        rendered.insert_str(closing_start, &suffix);
    } else {
        rendered.push_str(&suffix);
    }
    rendered
}

fn indent_block(value: &str, depth: usize) -> String {
    let indent = "  ".repeat(depth);
    value
        .lines()
        .map(|line| format!("{indent}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn xml_comments(description: &str, depth: usize) -> Vec<String> {
    let indent = "  ".repeat(depth);
    description
        .trim()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let safe = line.trim().replace("--", "—");
            format!("{indent}<!-- {safe} -->")
        })
        .collect()
}

fn xml_tag_name(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    for (index, ch) in name.chars().enumerate() {
        let valid = if index == 0 {
            ch.is_alphabetic() || ch == '_'
        } else {
            ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.')
        };
        result.push(if valid { ch } else { '_' });
    }

    if result.is_empty() {
        "value".to_string()
    } else {
        result
    }
}

fn escape_xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn unsupported(message: &str) -> minijinja::Error {
    minijinja::Error::new(minijinja::ErrorKind::BadSerialization, message.to_string())
}

fn serialization_error(error: anyhow::Error) -> minijinja::Error {
    unsupported(&error.to_string())
}

#[cfg(test)]
mod tests {
    use super::xml_tag_name;

    #[test]
    fn preserves_unicode_xml_names() {
        assert_eq!(xml_tag_name("prénom"), "prénom");
        assert_eq!(xml_tag_name("first name"), "first_name");
    }
}
