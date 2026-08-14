use baml_base::Name;
use baml_compiler2_hir::{item_tree::Attribute, loc::EnumLoc};
use text_size::TextRange;

/// Span-free semantic data for an `enum` declaration.
///
/// Enums carry no type expressions, so there is no `TypeRefStore` here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumData {
    pub name: Name,
    pub variants: Vec<EnumVariantData>,
    pub attributes: Vec<Attribute>,
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariantData {
    pub name: Name,
    pub attributes: Vec<Attribute>,
    pub docstring: Option<String>,
}

/// Spans for an `Enum`, parallel to [`EnumData`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumSourceMap {
    /// Full source span of the declaration.
    pub span: TextRange,
    /// Span of the enum's name token.
    pub name_span: TextRange,
    /// Name span per variant, parallel to [`EnumData::variants`].
    pub variant_name_spans: Vec<TextRange>,
}

/// Semantic data for one enum. Span-free — see the module docs.
#[salsa::tracked(returns(ref))]
pub fn enum_data<'db>(db: &'db dyn crate::Db, item: EnumLoc<'db>) -> EnumData {
    lower(db, item).0
}

/// Spans for one enum. Kept separate from [`enum_data`] so that a
/// whitespace-only edit invalidates this but not the semantic data.
#[salsa::tracked(returns(ref))]
pub fn enum_source_map<'db>(db: &'db dyn crate::Db, item: EnumLoc<'db>) -> EnumSourceMap {
    lower(db, item).1
}

fn lower<'db>(db: &'db dyn crate::Db, item: EnumLoc<'db>) -> (EnumData, EnumSourceMap) {
    let file = item.file(db);
    let item_tree = crate::file_item_tree(db, file);
    let item_source_map = crate::file_item_tree_source_map(db, file);
    let data = &item_tree[item.id(db)];

    (
        EnumData {
            name: data.name.clone(),
            variants: data
                .variants
                .iter()
                .map(|variant| EnumVariantData {
                    name: variant.name.clone(),
                    attributes: variant.attributes.clone(),
                    docstring: variant.docstring.clone(),
                })
                .collect(),
            attributes: data.attributes.clone(),
            docstring: data.docstring.clone(),
        },
        EnumSourceMap {
            span: data.span,
            name_span: item_source_map
                .enum_name_spans
                .get(&item.id(db))
                .copied()
                .unwrap_or_else(|| unreachable!("name span recorded at allocation")),
            variant_name_spans: item_source_map
                .enum_variant_spans
                .get(&item.id(db))
                .cloned()
                .unwrap_or_default(),
        },
    )
}
