//! Lightweight CST walk to extract @stream.* annotations from class fields.

use baml_base::Name;
use baml_compiler_syntax::SyntaxKind;
use rowan::ast::AstNode as _;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::ty::PpirTypeRef;

/// Stream annotations extracted from CST for a single field.
#[derive(Debug, Clone, Default)]
pub(crate) struct StreamAttrs {
    pub stream_type: Option<PpirTypeRef>,
    /// Raw CST value expression from @stream.starts_as(...). Passed through to HIR.
    pub stream_starts_as: Option<String>,
    pub stream_with_state: bool,
    pub stream_done: bool,
    pub stream_not_null: bool,
}

/// Extract stream annotations for all fields of all classes in a file's CST.
///
/// Returns a map: `class_name` → [(`field_name`, `StreamAttrs`)].
/// This is a lightweight walk — only looks at class definitions and their
/// field attributes, skipping all validation.
pub(crate) fn extract_stream_attrs_from_cst(
    cst: &baml_compiler_syntax::SyntaxNode,
) -> FxHashMap<SmolStr, Vec<(Name, StreamAttrs)>> {
    let mut result = FxHashMap::default();

    for child in cst.children() {
        if child.kind() != SyntaxKind::CLASS_DEF {
            continue;
        }
        let Some(class_def) = baml_compiler_syntax::ast::ClassDef::cast(child) else {
            continue;
        };
        let Some(class_name_tok) = class_def.name() else {
            continue;
        };
        let class_name = SmolStr::new(class_name_tok.text());

        // Check for class-level @@stream.* block attributes
        let mut class_stream_done = false;
        let mut class_stream_not_null = false;
        for block_attr in class_def.block_attributes() {
            if let Some(attr_name) = block_attr.full_name() {
                match attr_name.as_str() {
                    "stream.done" => class_stream_done = true,
                    "stream.not_null" => class_stream_not_null = true,
                    _ => {}
                }
            }
        }

        let mut field_attrs = Vec::new();
        for field_node in class_def.fields() {
            let Some(field_name_tok) = field_node.name() else {
                continue;
            };
            let field_name: Name = SmolStr::new(field_name_tok.text());

            let mut attrs = StreamAttrs {
                stream_done: class_stream_done,
                stream_not_null: class_stream_not_null,
                ..Default::default()
            };

            // Search for ATTRIBUTE nodes in all descendant positions.
            // The parser places dotted attributes (e.g., @stream.done) inside
            // TYPE_EXPR, while simple attributes (@alias, @skip) are direct
            // children of FIELD. Using descendants() finds both.
            for node_or_token in field_node.syntax().descendants() {
                if node_or_token.kind() != SyntaxKind::ATTRIBUTE {
                    continue;
                }
                let Some(attr) =
                    baml_compiler_syntax::ast::Attribute::cast(node_or_token)
                else {
                    continue;
                };
                if let Some(attr_name) = attr.full_name() {
                    match attr_name.as_str() {
                        "stream.type" => {
                            attrs.stream_type = parse_type_from_attr_args(&attr);
                        }
                        "stream.starts_as" => {
                            attrs.stream_starts_as = attr.string_arg();
                        }
                        "stream.with_state" => {
                            attrs.stream_with_state = true;
                        }
                        "stream.done" => {
                            attrs.stream_done = true;
                        }
                        "stream.not_null" => {
                            attrs.stream_not_null = true;
                        }
                        _ => {}
                    }
                }
            }

            field_attrs.push((field_name, attrs));
        }

        result.insert(class_name, field_attrs);
    }

    result
}

/// Parse a type expression from attribute arguments.
///
/// Handles: simple names (Person, int, never), the basic type keywords.
/// Returns None if parsing fails.
fn parse_type_from_attr_args(attr: &baml_compiler_syntax::ast::Attribute) -> Option<PpirTypeRef> {
    let arg_text = attr.string_arg()?;
    Some(PpirTypeRef::from_type_name(&arg_text))
}

