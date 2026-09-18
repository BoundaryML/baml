//! Attribute lowering: the one pass that reads a declaration's attributes.
//!
//! Attributes are a fixed vocabulary — [`baml_base::SCHEMA_ATTRIBUTE_SPECS`],
//! each entry with an argument shape and the declaration positions it has
//! meaning at. Lowering reads a declaration's raw `@`/`@@` list ONCE and
//! produces two things together: the position's typed carrier (a field's
//! [`ClassFieldAttrs`], a class's [`ClassAttrs`], …) and every attribute
//! diagnostic. Nothing downstream sees a raw attribute, so what was checked
//! and what was stored cannot disagree, and a carrier cannot hold an
//! attribute its position rejects — it has no field for one.

use baml_base::{AttributePosition, Name, SchemaAttributeArguments, SchemaAttributeSpec};
use baml_compiler_diagnostics::{diagnostic::DiagnosticId, runtime_type::SerializedKeyContainer};
use baml_compiler2_ast::{ast, parse_string_attr_value};
use rustc_hash::FxHashMap;
use text_size::TextRange;

use crate::{
    diagnostic::Hir2Diagnostic,
    item_tree::{ClassAttrs, ClassFieldAttrs, EnumAttrs, EnumVariantAttrs, SchemaAttrs},
};

/// One attribute's argument, in the shape its spec declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttributeValue {
    /// A flag attribute (`@skip`): present or not.
    Flag,
    /// A string attribute (`@alias("k")`): the decoded literal.
    Text(String),
}

/// A declaration position's typed attribute carrier.
pub(crate) trait AttributeCarrier: Default {
    const POSITION: AttributePosition;

    /// Store one attribute. Called only for a spec the table allows at
    /// [`Self::POSITION`], with a value of the spec's declared shape — so a
    /// name this cannot store is a drift between the table and the carrier,
    /// which `tests::every_carrier_stores_what_the_table_allows` rules out.
    fn record(&mut self, spec: &SchemaAttributeSpec, value: AttributeValue);
}

/// Lower a declaration's attributes into its position's carrier, reporting
/// unknown names, names without meaning at the position, malformed
/// arguments, and repeats of a single-valued attribute.
pub(crate) fn lower_attributes<C: AttributeCarrier>(
    raw: &[ast::RawAttribute],
    diagnostics: &mut Vec<Hir2Diagnostic>,
) -> C {
    let mut carrier = C::default();
    lower_into(raw, C::POSITION, diagnostics, &mut |spec, value| {
        carrier.record(spec, value);
    });
    carrier
}

/// Lower the attributes of a declaration whose position admits none: every
/// attribute written there is diagnosed and nothing is stored.
pub(crate) fn reject_attributes(
    raw: &[ast::RawAttribute],
    position: AttributePosition,
    diagnostics: &mut Vec<Hir2Diagnostic>,
) {
    lower_into(raw, position, diagnostics, &mut |spec, _| {
        unreachable!(
            "the attribute table allows `{}` on {} but the position has no carrier",
            spec.name,
            position.describe()
        )
    });
}

fn lower_into(
    raw: &[ast::RawAttribute],
    position: AttributePosition,
    diagnostics: &mut Vec<Hir2Diagnostic>,
    record: &mut dyn FnMut(&SchemaAttributeSpec, AttributeValue),
) {
    // Sites of each single-valued attribute, in first-seen order, so the
    // E0014 diagnostics are deterministic.
    let mut sites: Vec<(&'static str, Vec<TextRange>)> = Vec::new();
    for attr in raw {
        let Some(spec) = baml_base::schema_attribute_spec(attr.name.as_str()) else {
            diagnostics.push(Hir2Diagnostic::UnknownAttribute {
                attr_name: attr.name.clone(),
                position,
                span: attr.span,
            });
            continue;
        };
        if !spec.allows(position) {
            diagnostics.push(Hir2Diagnostic::AttributeNotAllowedHere {
                attr_name: attr.name.clone(),
                position,
                allowed: spec.positions,
                span: attr.span,
            });
            continue;
        }
        if !spec.repeatable {
            match sites.iter_mut().find(|(name, _)| *name == spec.name) {
                Some((_, spans)) => spans.push(attr.span),
                None => sites.push((spec.name, vec![attr.span])),
            }
        }
        // A repeat still records: the last write wins, as E0014 says.
        if let Some(value) = parse_argument(spec, attr, position, diagnostics) {
            record(spec, value);
        }
    }
    for (attr_name, sites) in sites {
        if sites.len() >= 2 {
            diagnostics.push(Hir2Diagnostic::DuplicateAttribute {
                attr_name: attr_name.to_string(),
                sites,
            });
        }
    }
}

/// The attribute's argument in its spec's shape, or `None` after reporting a
/// malformed one — a malformed attribute stores nothing rather than a guess.
fn parse_argument(
    spec: &SchemaAttributeSpec,
    attr: &ast::RawAttribute,
    position: AttributePosition,
    diagnostics: &mut Vec<Hir2Diagnostic>,
) -> Option<AttributeValue> {
    let spelled = format!("{}{}", position.sigil(), spec.name);
    match spec.arguments {
        SchemaAttributeArguments::None => {
            if attr.args.is_empty() {
                return Some(AttributeValue::Flag);
            }
            diagnostics.push(Hir2Diagnostic::DiagnosticMessage {
                diagnostic_id: DiagnosticId::UnexpectedAttributeArg,
                message: format!("`{spelled}` does not take any arguments"),
                span: attr.span,
            });
            None
        }
        SchemaAttributeArguments::String { .. } => {
            let [arg] = attr.args.as_slice() else {
                diagnostics.push(Hir2Diagnostic::DiagnosticMessage {
                    diagnostic_id: DiagnosticId::InvalidAttributeArg,
                    message: format!("`{spelled}` expects exactly one string argument"),
                    span: attr.span,
                });
                return None;
            };
            match parse_string_attr_value(&arg.value) {
                Some(text) => Some(AttributeValue::Text(text)),
                None => {
                    diagnostics.push(Hir2Diagnostic::DiagnosticMessage {
                        diagnostic_id: DiagnosticId::InvalidAttributeArg,
                        message: format!(
                            "`{spelled}` argument must be a string literal, got `{}`",
                            arg.value
                        ),
                        span: arg.span,
                    });
                    None
                }
            }
        }
    }
}

/// Store a schema-wide attribute on `schema`, handing back a value that is
/// not one so the carrier can store it itself.
fn record_schema(
    schema: &mut SchemaAttrs,
    spec: &SchemaAttributeSpec,
    value: AttributeValue,
) -> Option<AttributeValue> {
    match (spec.name, value) {
        ("description", AttributeValue::Text(text)) => {
            schema.description = Some(text);
            None
        }
        ("alias", AttributeValue::Text(text)) => {
            schema.alias = Some(text);
            None
        }
        (_, value) => Some(value),
    }
}

fn drift(spec: &SchemaAttributeSpec, position: AttributePosition) -> ! {
    unreachable!(
        "the attribute table allows `{}` on {} but its carrier has no field for it",
        spec.name,
        position.describe()
    )
}

impl AttributeCarrier for ClassFieldAttrs {
    const POSITION: AttributePosition = AttributePosition::ClassField;

    fn record(&mut self, spec: &SchemaAttributeSpec, value: AttributeValue) {
        let Some(value) = record_schema(&mut self.schema, spec, value) else {
            return;
        };
        match (spec.name, value) {
            ("skip", AttributeValue::Flag) => self.skip = true,
            ("stream.done", AttributeValue::Flag) => self.stream_done = true,
            ("stream.must_exist", AttributeValue::Flag) => self.must_exist = true,
            _ => drift(spec, Self::POSITION),
        }
    }
}

impl AttributeCarrier for EnumVariantAttrs {
    const POSITION: AttributePosition = AttributePosition::EnumVariant;

    fn record(&mut self, spec: &SchemaAttributeSpec, value: AttributeValue) {
        let Some(value) = record_schema(&mut self.schema, spec, value) else {
            return;
        };
        match (spec.name, value) {
            ("skip", AttributeValue::Flag) => self.skip = true,
            _ => drift(spec, Self::POSITION),
        }
    }
}

impl AttributeCarrier for ClassAttrs {
    const POSITION: AttributePosition = AttributePosition::Class;

    fn record(&mut self, spec: &SchemaAttributeSpec, value: AttributeValue) {
        let Some(value) = record_schema(&mut self.schema, spec, value) else {
            return;
        };
        match (spec.name, value) {
            ("stream.done", AttributeValue::Flag) => self.stream_done = true,
            _ => drift(spec, Self::POSITION),
        }
    }
}

impl AttributeCarrier for EnumAttrs {
    const POSITION: AttributePosition = AttributePosition::Enum;

    fn record(&mut self, spec: &SchemaAttributeSpec, value: AttributeValue) {
        if let Some(_unstored) = record_schema(&mut self.schema, spec, value) {
            drift(spec, Self::POSITION);
        }
    }
}

/// A class field or enum variant as the serialized-key check sees it.
pub(crate) struct SerializedMember<'a> {
    pub(crate) name: &'a Name,
    pub(crate) name_span: TextRange,
    pub(crate) alias: Option<&'a str>,
    pub(crate) skip: bool,
}

/// Reject a class or enum whose members don't all serialize to distinct
/// JSON keys.
///
/// A member's *effective serialized key* is its `@alias` value if it carries
/// one, otherwise its declared name. When an `@alias` is present the real
/// member name is never used for matching (see `bex_sap`'s
/// `AnnotatedField::key_matches`), so two members with the same effective key
/// are indistinguishable in the serialized schema: `ctx.output_format()`
/// renders duplicate keys and only the first can ever be satisfied. This
/// catches both `a @alias("x")` + `b @alias("x")` and a plain member `x`
/// colliding with another member's `@alias("x")`.
///
/// Repeated identical names are already reported by the duplicate-definition
/// checks, so this fires only when at least two *distinct* member names share
/// a key. A `@skip`ped member has no key.
pub(crate) fn check_serialized_keys<'a>(
    members: impl Iterator<Item = SerializedMember<'a>>,
    container: SerializedKeyContainer,
    diagnostics: &mut Vec<Hir2Diagnostic>,
) {
    let mut buckets: FxHashMap<String, Vec<(Name, TextRange)>> = FxHashMap::default();
    for member in members {
        if member.skip {
            continue;
        }
        let key = member
            .alias
            .map_or_else(|| member.name.as_str().to_string(), str::to_string);
        buckets
            .entry(key)
            .or_default()
            .push((member.name.clone(), member.name_span));
    }

    for (key, members) in buckets {
        let distinct = members.iter().any(|(name, _)| name != &members[0].0);
        if members.len() >= 2 && distinct {
            let sites = members.into_iter().map(|(_, span)| span).collect();
            diagnostics.push(Hir2Diagnostic::DuplicateFieldAlias {
                key,
                sites,
                container,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value_for(spec: &SchemaAttributeSpec) -> AttributeValue {
        match spec.arguments {
            SchemaAttributeArguments::None => AttributeValue::Flag,
            SchemaAttributeArguments::String { .. } => AttributeValue::Text("x".to_string()),
        }
    }

    fn exercise<C: AttributeCarrier>() {
        let mut carrier = C::default();
        for spec in baml_base::schema_attribute_specs_at(C::POSITION) {
            carrier.record(spec, value_for(spec));
        }
    }

    /// The table and the carriers are two spellings of one vocabulary: every
    /// attribute the table allows at a position has a field on that
    /// position's carrier, so `record` never reaches its drift arm.
    #[test]
    fn every_carrier_stores_what_the_table_allows() {
        exercise::<ClassFieldAttrs>();
        exercise::<EnumVariantAttrs>();
        exercise::<ClassAttrs>();
        exercise::<EnumAttrs>();
    }

    /// The positions lowered with [`reject_attributes`] have no carrier, so
    /// the table must allow nothing there.
    #[test]
    fn carrier_less_positions_admit_nothing() {
        for position in [
            AttributePosition::InterfaceField,
            AttributePosition::Interface,
            AttributePosition::Function,
        ] {
            assert!(
                baml_base::schema_attribute_specs_at(position)
                    .next()
                    .is_none(),
                "{position:?} admits an attribute but has no carrier"
            );
        }
    }
}
