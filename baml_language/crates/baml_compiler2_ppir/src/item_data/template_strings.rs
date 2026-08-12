use baml_base::Name;
use baml_compiler2_hir::{
    loc::TemplateStringLoc,
    type_ref::{TypeRefBuilder, TypeRefSourceMap, TypeRefStore},
};
use text_size::TextRange;

use crate::item_data::common::FunctionParamData;

/// Span-free semantic data for a `template_string` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateStringData {
    pub name: Name,
    /// Type references in this template's parameter list. Scoped to the item.
    pub type_refs: TypeRefStore,
    pub params: Vec<FunctionParamData>,
    /// Legacy template body text retained for parser recovery.
    pub body: Option<String>,
}

/// Spans for a `TemplateString`, parallel to [`TemplateStringData`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateStringSourceMap {
    /// Full source span of the declaration.
    pub span: TextRange,
    /// Span of the template string's name token.
    pub name_span: TextRange,
    /// Spans for every node in [`TemplateStringData::type_refs`].
    pub type_refs: TypeRefSourceMap,
    /// One span per parameter, parallel to [`TemplateStringData::params`].
    pub param_spans: Vec<TextRange>,
}

/// Semantic data for one template string. Span-free — see the module docs.
#[salsa::tracked(returns(ref))]
pub fn template_string_data<'db>(
    db: &'db dyn crate::Db,
    template: TemplateStringLoc<'db>,
) -> TemplateStringData {
    lower(db, template).0
}

/// Spans for one template string. Kept separate from [`template_string_data`] so
/// that a whitespace-only edit invalidates this but not the semantic data.
#[salsa::tracked(returns(ref))]
pub fn template_string_source_map<'db>(
    db: &'db dyn crate::Db,
    template: TemplateStringLoc<'db>,
) -> TemplateStringSourceMap {
    lower(db, template).1
}

fn lower<'db>(
    db: &'db dyn crate::Db,
    template: TemplateStringLoc<'db>,
) -> (TemplateStringData, TemplateStringSourceMap) {
    let file = template.file(db);
    let item_tree = crate::file_item_tree(db, file);
    let item_source_map = crate::file_item_tree_source_map(db, file);
    let data = &item_tree[template.id(db)];

    let mut type_refs = TypeRefBuilder::new();
    let params = data
        .params
        .iter()
        .map(|param| FunctionParamData {
            name: param.name.clone(),
            type_ref: param.type_expr.as_ref().map(|te| type_refs.lower(te)),
            has_default: param.default.is_some(),
        })
        .collect();
    let (store, spans) = type_refs.finish();

    (
        TemplateStringData {
            name: data.name.clone(),
            type_refs: store,
            params,
            body: data.body.clone(),
        },
        TemplateStringSourceMap {
            span: data.span,
            name_span: item_source_map
                .template_string_name_spans
                .get(&template.id(db))
                .copied()
                .unwrap_or_else(|| unreachable!("name span recorded at allocation")),
            type_refs: spans,
            param_spans: data.params.iter().map(|param| param.span).collect(),
        },
    )
}
