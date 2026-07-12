use std::collections::HashSet;

use baml_types::{TypeIR, TypeValue};

use super::types::{OutputFormatContent, RenderOptions, RenderSetting};

pub(super) fn render(
    content: &OutputFormatContent,
    options: &RenderOptions,
) -> Result<Option<String>, minijinja::Error> {
    // Keep the long-standing behavior for a bare string return. There is no
    // structure to describe, and jsonish deliberately returns the raw model
    // response for string targets.
    if matches!(content.target, TypeIR::Primitive(TypeValue::String, _)) {
        return Ok(None);
    }

    let null_value = match &options.render_null_as {
        RenderSetting::Always(value) => value.as_str(),
        RenderSetting::Auto | RenderSetting::Never => "null",
    };
    let tag = root_tag(content, &content.target)?;
    let body = render_named_type(
        content,
        &content.target,
        &tag,
        0,
        &mut HashSet::new(),
        null_value,
    )?;

    let prefix = match &options.prefix {
        RenderSetting::Always(prefix) => prefix.clone(),
        RenderSetting::Never => String::new(),
        RenderSetting::Auto if matches!(content.target, TypeIR::Union(_, _)) => {
            "Answer in XML using exactly one of these templates:\n".to_string()
        }
        RenderSetting::Auto => "Answer in XML using this template:\n".to_string(),
    };

    Ok(Some(format!("{prefix}{body}")))
}

fn root_tag(content: &OutputFormatContent, target: &TypeIR) -> Result<String, minijinja::Error> {
    Ok(match target {
        TypeIR::Class { name, mode, .. } => content
            .find_class(mode, name)
            .map(|class| xml_tag_name(class.name.rendered_name()))
            .map_err(serialization_error)?,
        TypeIR::Enum { name, .. } => content
            .find_enum(name)
            .map(|enm| xml_tag_name(enm.name.rendered_name()))
            .map_err(serialization_error)?,
        TypeIR::List(_, _) => "items".to_string(),
        TypeIR::Map(_, _, _) => "map".to_string(),
        _ => "value".to_string(),
    })
}

fn render_named_type(
    content: &OutputFormatContent,
    target: &TypeIR,
    tag: &str,
    depth: usize,
    visiting: &mut HashSet<String>,
    null_value: &str,
) -> Result<String, minijinja::Error> {
    let indent = "  ".repeat(depth);
    let tag = xml_tag_name(tag);

    Ok(match target {
        TypeIR::Primitive(value, _) => match value {
            TypeValue::String => leaf(&indent, &tag, "string"),
            TypeValue::Int => leaf(&indent, &tag, "int"),
            TypeValue::Float => leaf(&indent, &tag, "float"),
            TypeValue::Bool => leaf(&indent, &tag, "bool"),
            TypeValue::Null => leaf(&indent, &tag, null_value),
            TypeValue::Media(media_type) => {
                return Err(minijinja::Error::new(
                    minijinja::ErrorKind::BadSerialization,
                    format!("type '{media_type}' is not supported in outputs"),
                ))
            }
        },
        TypeIR::Literal(value, _) => leaf(&indent, &tag, &value.to_string()),
        TypeIR::Enum { name, .. } => {
            let enm = content.find_enum(name).map_err(serialization_error)?;
            let values = enm
                .values
                .iter()
                .map(|(name, _)| name.rendered_name())
                .collect::<Vec<_>>()
                .join(" | ");
            leaf(&indent, &tag, &values)
        }
        TypeIR::Class { name, mode, .. } => {
            if !visiting.insert(name.clone()) {
                format!(
                    "{indent}<!-- Repeat the {name} shape recursively here. -->\n{}",
                    leaf(&indent, &tag, name)
                )
            } else {
                let class = content
                    .find_class(mode, name)
                    .map_err(serialization_error)?;
                let mut fields = Vec::new();
                for (field_name, field_type, description, _) in &class.fields {
                    if let Some(description) = description {
                        fields.extend(xml_comments(description, depth + 1));
                    }
                    if is_optional(field_type) {
                        fields.push(format!(
                            "{}<!-- Optional: omit this element when its value is null. -->",
                            "  ".repeat(depth + 1)
                        ));
                    }
                    fields.push(render_named_type(
                        content,
                        field_type,
                        field_name.rendered_name(),
                        depth + 1,
                        visiting,
                        null_value,
                    )?);
                }
                visiting.remove(name);

                if fields.is_empty() {
                    format!("{indent}<{tag}></{tag}>")
                } else {
                    format!("{indent}<{tag}>\n{}\n{indent}</{tag}>", fields.join("\n"))
                }
            }
        }
        TypeIR::RecursiveTypeAlias { name, .. } => {
            if !visiting.insert(name.clone()) {
                leaf(&indent, &tag, name)
            } else {
                let alias_target = content
                    .find_recursive_alias_target(name)
                    .map_err(serialization_error)?;
                let rendered =
                    render_named_type(content, alias_target, &tag, depth, visiting, null_value)?;
                visiting.remove(name);
                rendered
            }
        }
        TypeIR::List(item, _) => format!(
            "{indent}<{tag}>\n{}\n{indent}</{tag}>",
            render_named_type(content, item, "item", depth + 1, visiting, null_value)?
        ),
        TypeIR::Map(key, value, _) => {
            let child_indent = "  ".repeat(depth + 1);
            format!(
                "{indent}<{tag}>\n{child_indent}<entry>\n{}\n{}\n{child_indent}</entry>\n{indent}</{tag}>",
                render_named_type(content, key, "key", depth + 2, visiting, null_value)?,
                render_named_type(content, value, "value", depth + 2, visiting, null_value)?,
            )
        }
        TypeIR::Union(items, _) => {
            let variants = items
                .iter_include_null()
                .iter()
                .map(|variant| {
                    render_named_type(content, variant, &tag, depth, visiting, null_value)
                })
                .collect::<Result<Vec<_>, _>>()?;
            variants.join(&format!("\n{indent}<!-- OR -->\n"))
        }
        TypeIR::Tuple(_, _) => {
            return Err(minijinja::Error::new(
                minijinja::ErrorKind::BadSerialization,
                "Tuple type is not supported in outputs",
            ))
        }
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

fn leaf(indent: &str, tag: &str, value: &str) -> String {
    format!(
        "{indent}<{tag}>{}</{tag}>",
        escape_xml_text(value.trim_matches('"'))
    )
}

fn is_optional(target: &TypeIR) -> bool {
    match target {
        TypeIR::Union(items, _) => items
            .iter_include_null()
            .iter()
            .any(|item| matches!(item, TypeIR::Primitive(TypeValue::Null, _))),
        _ => false,
    }
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

fn serialization_error(error: anyhow::Error) -> minijinja::Error {
    minijinja::Error::new(minijinja::ErrorKind::BadSerialization, error.to_string())
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
