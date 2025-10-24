use std::str::FromStr;

use baml_types::{LiteralValue, TypeValue};
use internal_baml_diagnostics::{DatamodelError, Diagnostics};

use super::{helpers::Pair, parse_attribute::parse_attribute, Rule};
use crate::{
    ast::*,
    parser::{
        helpers::{assert_correct_parser, parsing_catch_all, unreachable_rule},
        parse_field::parse_field_type_with_attr,
        parse_identifier::parse_identifier,
    },
};

pub fn parse_field_type(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser(
        &pair,
        &[Rule::field_type, Rule::openParen, Rule::closeParen],
        diagnostics,
    );

    let mut arity = FieldArity::Required;
    let mut ftype = None;
    let mut attributes = Vec::new();
    let span = diagnostics.span(pair.as_span());

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::union => {
                let result = parse_union(current, diagnostics);
                ftype = result;
            }
            Rule::non_union => {
                let result = parse_base_type(current, diagnostics);

                ftype = result;
            }
            Rule::field_attribute => {
                attributes.push(parse_attribute(current, false, diagnostics));
            }
            Rule::optional_token => arity = FieldArity::Optional,
            _ => {
                parsing_catch_all(current, "field_type", diagnostics);
            }
        }
    }

    match ftype {
        Some(ftype) => {
            if arity.is_optional() {
                Some(ftype.to_nullable())
            } else {
                Some(ftype)
            }
        }
        None => {
            diagnostics.push_error(DatamodelError::new_parser_error(
                "Field type must be defined".to_string(),
                span,
            ));
            None
        }
    }
}

fn parse_union(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser(&pair, &[Rule::union], diagnostics);

    let span = diagnostics.span(pair.as_span());
    let mut types = Vec::new();
    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::base_type => {
                if let Some(f) = parse_base_type(current, diagnostics) {
                    types.push(f)
                }
            }
            Rule::base_type_with_attr => {
                if let Some(f) = parse_base_type_with_attr(current, diagnostics) {
                    types.push(f)
                }
            }
            Rule::field_operator => {}

            _ => unreachable_rule(&current, "union", diagnostics),
        }
    }

    let mut union = match types.len() {
        0 => {
            diagnostics.push_error(DatamodelError::new_parser_error(
                "A union must have at least 1 type".to_string(),
                span,
            ));
            None
        }
        1 => Some(types[0].to_owned()),
        _ => Some(FieldType::Union(FieldArity::Required, types, span, None)),
    };

    // Match statement above gets rid of the union if there's only one type.
    // In that case attributes should already be associated to that type.
    if matches!(union, Some(FieldType::Union(_, _, _, _))) {
        union.as_mut().map(reassociate_union_attributes);
    }

    union
}

fn parse_base_type_with_attr(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    let mut attributes = Vec::new();
    let mut base_type = None;

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::base_type => {
                base_type = parse_base_type(current, diagnostics);
            }
            Rule::field_attribute => {
                let att = parse_attribute(current, false, diagnostics);
                attributes.push(att);
            }
            _ => unreachable_rule(&current, "base_type_with_attr", diagnostics),
        }
    }

    match base_type {
        Some(mut ft) => {
            ft.extend_attributes(attributes);
            Some(ft)
        }
        None => None,
    }
}

fn parse_base_type(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser(
        &pair,
        &[
            Rule::base_type,
            Rule::non_union,
            Rule::base_type_without_array,
        ],
        diagnostics,
    );

    let span = diagnostics.span(pair.as_span());
    if let Some(current) = pair.into_inner().next() {
        return match current.as_rule() {
            Rule::path_identifier => {
                diagnostics.push_error(DatamodelError::new_validation_error(
                    "Path identifiers are not supported in base types",
                    diagnostics.span(current.as_span()),
                ));
                // Return a Symbol type to allow type validation to continue
                // This will trigger the "type not found" error in the validation pipeline
                Some(FieldType::Symbol(
                    FieldArity::Required,
                    Identifier::Local(
                        current.as_str().to_string(),
                        diagnostics.span(current.as_span()),
                    ),
                    None,
                ))
            }
            Rule::identifier => {
                let identifier = parse_identifier(current.clone(), diagnostics);
                let field_type = match current.as_str() {
                    "string" | "int" | "float" | "bool" | "image" | "audio" | "pdf" | "video" => {
                        FieldType::Primitive(
                            FieldArity::Required,
                            TypeValue::from_str(identifier.name()).expect("Invalid type value"),
                            diagnostics.span(current.as_span()),
                            None,
                        )
                    }
                    "null" => FieldType::Primitive(
                        FieldArity::Optional,
                        TypeValue::Null,
                        diagnostics.span(current.as_span()),
                        None,
                    ),
                    "true" => FieldType::Literal(
                        FieldArity::Required,
                        LiteralValue::Bool(true),
                        diagnostics.span(current.as_span()),
                        None,
                    ),
                    "false" => FieldType::Literal(
                        FieldArity::Required,
                        LiteralValue::Bool(false),
                        diagnostics.span(current.as_span()),
                        None,
                    ),
                    _ => FieldType::Symbol(
                        FieldArity::Required,
                        Identifier::Local(
                            identifier.name().to_string(),
                            diagnostics.span(current.as_span()),
                        ),
                        None,
                    ),
                };
                Some(field_type)
            }
            Rule::array_notation => parse_array(current, diagnostics),
            Rule::map => parse_map(current, diagnostics),
            Rule::group => parse_group(current, diagnostics),
            Rule::tuple => parse_tuple(current, diagnostics),
            Rule::parenthesized_type => parse_parenthesized_type(current, diagnostics),
            Rule::literal_type => parse_literal_type(current, diagnostics),
            _ => {
                unreachable_rule(&current, "base_type", diagnostics);
                None
            }
        };
    }

    diagnostics.push_error(DatamodelError::new_parser_error(
        "A base type must be defined".to_string(),
        span,
    ));
    None
}

fn parse_parenthesized_type(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser(&pair, &[Rule::parenthesized_type], diagnostics);

    let span = diagnostics.span(pair.as_span());
    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::openParen | Rule::closeParen => continue,
            Rule::field_type_with_attr => {
                return parse_field_type_with_attr(current, true, diagnostics);
            }
            _ => unreachable_rule(&current, "parenthesized_type", diagnostics),
        }
    }

    diagnostics.push_error(DatamodelError::new_parser_error(
        "Parenthesized type must contain a field type".to_string(),
        span,
    ));
    None
}

fn parse_literal_type(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser(&pair, &[Rule::literal_type], diagnostics);

    let span = diagnostics.span(pair.as_span());

    let Some(literal_type) = pair.into_inner().next() else {
        diagnostics.push_error(DatamodelError::new_parser_error(
            "Literal type must contain a value".to_string(),
            span,
        ));
        return None;
    };

    let literal_value = match literal_type.as_rule() {
        Rule::quoted_string_literal => match literal_type.into_inner().next() {
            Some(string_content) => LiteralValue::String(string_content.as_str().into()),
            None => {
                diagnostics.push_error(DatamodelError::new_parser_error(
                    "Quoted string literal has no string content".to_string(),
                    span,
                ));
                return None;
            }
        },

        Rule::numeric_literal => match literal_type.as_str().parse::<i64>() {
            Ok(int) => LiteralValue::Int(int),

            // This should only be a float because of how the pest grammar is defined.
            Err(_e) => {
                diagnostics.push_error(DatamodelError::new_validation_error(
                    format!(
                        "Float literal values are not supported: {}",
                        literal_type.as_str()
                    )
                    .as_str(),
                    span,
                ));

                return None;
            }
        },
        _ => {
            unreachable_rule(&literal_type, "literal_type", diagnostics);
            LiteralValue::String(String::new())
        }
    };

    Some(FieldType::Literal(
        FieldArity::Required,
        literal_value,
        span,
        None,
    ))
}

/// Parses array type notation from input pair.
///
/// Handles both required and optional arrays like `string[]` and `string[]?`.
/// Returns `Some(FieldType::List)` if the array type was successfully parsed
/// with arity or [`None`] if parsing fails.
///
/// # Arguments
///
/// * `pair` - Input pair with array notation tokens.
/// * `diagnostics` - Mutable reference to diagnostics collector for error
///   reporting.
///
/// # Implementation Details
///
/// * Supports multiple dimensions like `string[][]`.
/// * Handles optional arrays with `?` suffix.
/// * Preserves source span info for errors.
/// * Valid inputs: `string[]`, `int[]?`, `MyClass[][]?`.
fn parse_array(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser(&pair, &[Rule::array_notation], diagnostics);

    let mut dims = 0_u32;
    let mut field = None;
    // Track whether this array is optional (e.g., string[]?)
    // default to Required, will be updated to Optional if ? token is found
    let mut arity = FieldArity::Required;
    let span = diagnostics.span(pair.as_span());

    for current in pair.into_inner() {
        match current.as_rule() {
            // Parse the base type of the array (e.g., 'string' in string[])
            Rule::base_type_without_array => field = parse_base_type(current, diagnostics),
            // Count array dimensions (number of [] pairs)
            Rule::array_suffix => dims += 1,
            // Handle optional marker (?) for arrays like string[]?
            // This makes the entire array optional, not its elements
            Rule::optional_token => arity = FieldArity::Optional,
            _ => {
                unreachable_rule(&current, "map", diagnostics);
            }
        }
    }

    match field {
        Some(field) => Some(FieldType::List(
            arity,           // Whether the array itself is optional
            Box::new(field), // The type of elements in the array
            dims,            // Number of dimensions (e.g., 2 for string[][])
            span,            // Source location for error reporting
            None,            // No attributes initially
        )),
        None => {
            diagnostics.push_error(DatamodelError::new_parser_error(
                "Array type must have a base type defined".to_string(),
                span,
            ));
            None
        }
    }
}

/// Parses a map type notation from the input pair.
///
/// Handles both required and optional maps (e.g., `map<string, int>` and
/// `map<string, int>?`).
///
/// # Arguments
///
/// * `pair` - The input pair containing map notation tokens.
/// * `diagnostics` - Mutable reference to the diagnostics collector for error
///   reporting.
///
/// # Returns
///
/// * `Some(FieldType::Map)` - Successfully parsed map type with appropriate
///   arity.
/// * [`None`] - If parsing fails.
///
/// # Implementation Details
///
/// - Supports optional maps with the `?` suffix.
/// - Preserves source span information for error reporting.
/// - Example valid inputs: `map<string, int>`, `map<string, myclass>?`.
fn parse_map(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser(&pair, &[Rule::map], diagnostics);

    let mut fields = Vec::new();
    // Track whether this map is optional (e.g., map<string, int>?)
    // Default to Required, will be updated to Optional if ? token is found
    let mut arity = FieldArity::Required;
    let span = diagnostics.span(pair.as_span());

    for current in pair.into_inner() {
        match current.as_rule() {
            // Parse both key and value types of the map
            Rule::field_type => {
                if let Some(f) = parse_field_type(current, diagnostics) {
                    fields.push(f)
                }
            }
            // Handle optional marker (?) for maps like map<string, int>?
            // This makes the entire map optional, not its values
            Rule::optional_token => arity = FieldArity::Optional,
            _ => {
                unreachable_rule(&current, "map", diagnostics);
            }
        }
    }

    match fields.len() {
        0 => None, // Invalid: no types specified
        1 => None, // Invalid: only key type specified
        2 => Some(FieldType::Map(
            arity,                                                  // Whether the map itself is optional
            Box::new((fields[0].to_owned(), fields[1].to_owned())), // Key and value types
            span, // Source location for error reporting
            None, // No attributes initially
        )),
        _ => {
            diagnostics.push_error(DatamodelError::new_parser_error(
                "Maps must specify exactly a key type and a value type".to_string(),
                span,
            ));
            None
        }
    }
}

fn parse_group(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser(&pair, &[Rule::group], diagnostics);
    let mut attributes = Vec::new();
    let mut field_type = None;

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::openParen | Rule::closeParen => continue,
            Rule::field_type => {
                field_type = parse_field_type(current, diagnostics);
            }
            Rule::field_attribute => {
                let attr = parse_attribute(current, true, diagnostics);
                attributes.push(attr);
            }
            _ => {
                unreachable_rule(&current, "group", diagnostics);
            }
        }
    }

    if let Some(ft) = field_type.as_mut() {
        ft.extend_attributes(attributes)
    };

    field_type
}

fn parse_tuple(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser(&pair, &[Rule::tuple], diagnostics);

    let span = diagnostics.span(pair.as_span());

    let mut fields = Vec::new();

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::openParen | Rule::closeParen => continue,

            Rule::field_type_with_attr => {
                if let Some(f) = parse_field_type_with_attr(current, false, diagnostics) {
                    fields.push(f)
                }
            }
            Rule::field_type => {
                if let Some(f) = parse_field_type(current, diagnostics) {
                    fields.push(f)
                }
            }
            _ => {
                unreachable_rule(&current, "tuple", diagnostics);
            }
        }
    }

    match fields.len() {
        0 => None,
        1 => Some(fields[0].to_owned()),
        _ => Some(FieldType::Tuple(FieldArity::Required, fields, span, None)),
    }
}

/// For the last variant of a [`FieldType::Union`], here we remove the
/// attributes from that variant and attach them to the union, unless the
/// attribute was tagged with the `parenthesized` field.
///
/// This is done because `field_foo int | string @description("d")` is naturally
/// parsed as a field with a union whose secord variant has a description. But
/// the correct Baml interpretation is a union with a description.
pub fn reassociate_union_attributes(field_type: &mut FieldType) {
    match field_type {
        FieldType::Union(_arity, ref mut variants, _, _) => {
            if let Some(last_variant) = variants.last_mut() {
                let last_variant_attributes = last_variant.attributes().to_owned();
                let (attrs_for_variant, attrs_for_union): (Vec<Attribute>, Vec<Attribute>) =
                    last_variant_attributes
                        .into_iter()
                        .partition(|attr| attr.parenthesized);
                last_variant.set_attributes(attrs_for_variant);
                field_type.extend_attributes(attrs_for_union);
            }
        }
        _ => {
            // This is an internal error - the function should only be called on unions
            // Since we don't have diagnostics here, we silently return
            // The validation pipeline will catch any type errors later
        }
    }
}

#[cfg(test)]
mod tests {
    use pest::{consumes_to, parses_to};

    use super::super::{BAMLParser, Rule};

    /// Tests the parsing of optional array and map types.
    /// This test ensures that the parser correctly handles the optional token (?)
    /// when applied to arrays and maps.
    ///
    /// # Test Cases
    /// 1. Optional Arrays:
    ///    - Tests `string[]?` syntax
    ///    - Verifies correct token positions and nesting
    ///    - Ensures optional token is properly associated with array type
    ///
    /// 2. Optional Maps:
    ///    - Tests `map<string, int>?` syntax
    ///    - Verifies correct token positions and nesting
    ///    - Ensures optional token is properly associated with map type
    ///
    /// These test cases verify the implementation of issue #948,
    /// which requested support for optional lists and maps in BAML.
    #[test]
    fn optional_types() {
        // Test Case 1: Optional Arrays
        parses_to! {
            parser: BAMLParser,
            input: r#"string[]?"#,
            rule: Rule::field_type,
            tokens: [field_type(0,9,[
                non_union(0,9,[
                    array_notation(0,9,[
                        base_type_without_array(0,6,[
                            identifier(0,6,[
                                single_word(0,6)
                            ])
                        ]),
                        array_suffix(6,8),
                        optional_token(8,9)
                    ])
                ])
            ])]
        };

        // Test Case 2: Optional Maps
        parses_to! {
            parser: BAMLParser,
            input: r#"map<string, int>?"#,
            rule: Rule::field_type,
            tokens: [field_type(0,17,[
                non_union(0,17,[
                    map(0,17,[
                        field_type(4,10,[
                            non_union(4,10,[
                                identifier(4,10,[
                                    single_word(4,10)
                                ])
                            ])
                        ]),
                        field_type(12,15,[
                            non_union(12,15,[
                                identifier(12,15,[
                                    single_word(12,15)
                                ])
                            ])
                        ]),
                        optional_token(16,17)
                    ])
                ])
            ])]
        }
    }
}
