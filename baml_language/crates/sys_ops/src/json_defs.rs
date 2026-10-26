//! A [`SysOpContext`]'s definitions as the JSON inhabitance analysis reads
//! them ([`baml_type::json_inhabit`]).

use baml_type::json_inhabit::{JsonClass, JsonDefs, JsonField, JsonStatus, JsonVariant};
use sys_types::{DefKey, SapTy, SysOpContext};

/// How a reader takes the types whose JSON form is its own call.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Reader {
    /// `baml.json.schema` and `ai.internal.reaches_media`: every type must
    /// have a schema. `unknown`, an interface, a media type that does not
    /// name its kind, and the forms the schema has no rule for are rejected.
    Schema,
    /// The prompt renderer: takes every type as it is. A type it has no form
    /// for fails when it is rendered.
    Render,
}

/// `ctx`'s definitions, as `reader` reads them.
pub(crate) struct ContextDefs<'a> {
    pub ctx: &'a SysOpContext,
    pub reader: Reader,
}

impl ContextDefs<'_> {
    fn enum_definition(&self, head: &DefKey) -> Result<&sys_types::EnumDefinition, String> {
        self.ctx
            .enum_definitions
            .get(head)
            .ok_or_else(|| format!("json_schema: unknown enum `{}`", head.display_name()))
    }
}

impl JsonDefs<DefKey> for ContextDefs<'_> {
    type Error = String;

    fn class(&self, head: &DefKey, args: &[SapTy]) -> Result<JsonClass<DefKey>, String> {
        let class_def = self
            .ctx
            .class_definitions
            .get(head)
            .ok_or_else(|| format!("json_schema: unknown class `{}`", head.display_name()))?;
        let fields = class_def
            .fields
            .iter()
            .map(|field| {
                // The renderer walks a field through its template, so a
                // generic class reads at its arguments; the schema takes no
                // generic class, so the declared type is the one to read.
                let ty = match (&field.field_template, self.reader) {
                    (Some(template), Reader::Render) => template.substitute_symbolic(args),
                    (Some(template), Reader::Schema) if !args.is_empty() => {
                        template.substitute_symbolic(args)
                    }
                    _ => field.field_type.clone(),
                };
                JsonField {
                    name: field.name.clone(),
                    alias: field.alias.clone(),
                    skip: field.skip,
                    ty,
                }
            })
            .collect();
        Ok(JsonClass { fields })
    }

    fn variants(&self, head: &DefKey) -> Result<Vec<JsonVariant>, String> {
        Ok(self
            .enum_definition(head)?
            .declared_variants
            .iter()
            .map(|variant| JsonVariant {
                name: variant.name.clone(),
                skip: variant.skip,
            })
            .collect())
    }

    fn shape(&self, ty: &SapTy) -> Result<Option<SapTy>, String> {
        Ok(llm_shape(self.ctx, ty).cloned())
    }

    fn alias_target(&self, head: &DefKey) -> Result<SapTy, String> {
        self.ctx
            .type_alias_definitions
            .get(head)
            .cloned()
            .ok_or_else(|| format!("json_schema: unknown type alias `{}`", head.display_name()))
    }

    fn opaque(&self, ty: &SapTy) -> Result<JsonStatus, String> {
        if self.reader == Reader::Render {
            return Ok(JsonStatus::Json);
        }
        match ty {
            SapTy::Media(baml_type::MediaKind::Generic) => Err(format!(
                "json_schema: `{ty}` does not say which kind of media it is; \
                 a JSON media value is one of image, audio, video or pdf"
            )),
            SapTy::Media(_) => Ok(JsonStatus::Json),
            SapTy::Unknown => Err(
                "json_schema: `unknown` admits every value, which no schema describes".to_string(),
            ),
            SapTy::Interface(..) => Err(format!(
                "json_schema: interface type `{ty}` admits values of types not known \
                 here, which no schema describes"
            )),
            _ => Err(format!("json_schema: `{ty}` is not a resolved type")),
        }
    }

    fn check(&self, ty: &SapTy) -> Result<(), String> {
        if self.reader == Reader::Render {
            return Ok(());
        }
        match ty {
            SapTy::Literal(baml_base::Literal::Float(_), _) => Err(format!(
                "json_schema: `{ty}` is a float literal type, which BAML does not have"
            )),
            SapTy::Map { key, .. } => map_keys(key).map(|_| ()),
            SapTy::Class(_, args) if !args.is_empty() => Err(format!(
                "json_schema: generic class instance `{ty}` is not supported yet"
            )),
            SapTy::EnumVariant(head, name) => {
                let enum_def = self.enum_definition(head)?;
                if enum_def
                    .declared_variants
                    .iter()
                    .any(|variant| variant.name == name.as_str())
                {
                    Ok(())
                } else {
                    Err(format!(
                        "json_schema: enum `{}` has no variant `{name}`",
                        head.display_name()
                    ))
                }
            }
            _ => Ok(()),
        }
    }
}

/// What a model writes in place of `ty`, a class or enum type: the `Shape` of
/// its `baml.FromLlmOutput` implementation, if it has one. Every reader of
/// [`SysOpContext`] describes a model's answer, so each describes the shape.
pub(crate) fn llm_shape<'a>(ctx: &'a SysOpContext, ty: &SapTy) -> Option<&'a SapTy> {
    if ctx.llm_shapes.is_empty() {
        return None;
    }
    match ty {
        // The engine keys an instantiation with each argument's union flat.
        SapTy::Class(head, args) => {
            let key = SapTy::Class(
                head.clone(),
                args.iter().cloned().map(SapTy::flatten_union).collect(),
            );
            ctx.llm_shapes.get(&key)
        }
        SapTy::Enum(_) => ctx.llm_shapes.get(ty),
        _ => None,
    }
}

/// The hint a model is shown for a type it writes as its `Shape`: the type's
/// `@@description`, never its `///` docs, which are written for code.
pub(crate) fn llm_shape_hint(ctx: &SysOpContext, ty: &SapTy) -> Option<String> {
    let description = match ty {
        SapTy::Class(head, _) => ctx.class_definitions.get(head)?.description.as_deref(),
        SapTy::Enum(head) => ctx.enum_definitions.get(head)?.description.as_deref(),
        _ => None,
    }?;
    let description = description.trim();
    (!description.is_empty()).then(|| description.to_string())
}

/// The keys a map's key type allows in a JSON object: `None` for every
/// string, or the string literals it is a union of. Any other key type has no
/// JSON object form.
pub(crate) fn map_keys(key: &SapTy) -> Result<Option<Vec<String>>, String> {
    let mut members = Vec::new();
    match key {
        SapTy::Union(union) => baml_type::json_inhabit::flatten_union(union, &mut members),
        _ => members.push(key),
    }
    let mut literals = Vec::new();
    let mut any_string = false;
    for member in members {
        match member {
            SapTy::String => any_string = true,
            SapTy::Literal(baml_base::Literal::String(literal), _) => {
                literals.push(literal.clone());
            }
            _ => {
                return Err(format!(
                    "json_schema: map key type `{key}` has no JSON object key form; \
                     a key type must be `string` or string literals"
                ));
            }
        }
    }
    Ok((!any_string).then_some(literals))
}
