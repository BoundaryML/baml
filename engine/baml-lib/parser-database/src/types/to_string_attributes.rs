use std::collections::HashMap;

use crate::interner::StringId;
use internal_baml_jinja::LazyExpression;

///
#[derive(Debug)]
pub enum ToStringAttributes {
    ///
    Static(StaticStringAttributes),
    ///
    Dynamic(DynamicStringAttributes),
}

// TODO: This type should be removed (deleted).
///
#[derive(Debug, Default)]
pub struct DynamicStringAttributes {
    ///
    pub code: HashMap<StringId, StringId>,
}

impl DynamicStringAttributes {
    ///
    pub fn add_code(&mut self, language: StringId, code: StringId) -> bool {
        if self.code.contains_key(&language) {
            return false;
        }
        self.code.insert(language, code);
        true
    }
}

///
#[derive(Debug, Default)]
pub struct StaticStringAttributes {
    dynamic_type: Option<bool>, // TODO: This should be a LazyExpression?
    skip: Option<bool>, // TODO: This should be a LazyExpression.
    alias: Option<StringId>, // TODO: This should be a LazyExpression.
    meta: HashMap<StringId, LazyExpression<String>>,
}

impl StaticStringAttributes {
    ///
    pub fn skip(&self) -> &Option<bool> {
        &self.skip
    }

    ///
    pub fn set_skip(&mut self, skip: bool) {
        self.skip.replace(skip);
    }

    ///
    pub fn dynamic_type(&self) -> &Option<bool> {
        &self.dynamic_type
    }

    ///
    pub fn set_dynamic_type(&mut self) {
        self.dynamic_type.replace(true);
    }

    ///
    pub fn add_meta(&mut self, meta_name: StringId, value: LazyExpression<String>) -> bool {
        if self.meta.contains_key(&meta_name) {
            return false;
        }
        self.meta.insert(meta_name, value);
        true
    }

    ///
    pub fn meta(&self) -> &HashMap<StringId, LazyExpression<String>> {
        &self.meta
    }

    ///
    pub fn add_alias(&mut self, alias: StringId) {
        self.alias.replace(alias);
    }

    ///
    pub fn alias(&self) -> &Option<StringId> {
        &self.alias
    }
}
