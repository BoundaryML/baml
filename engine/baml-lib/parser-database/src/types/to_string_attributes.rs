use std::collections::HashMap;

use crate::interner::StringId;

///
#[derive(Debug)]
pub enum ToStringAttributes {
    ///
    Static(StaticStringAttributes),
    ///
    Dynamic(DynamicStringAttributes),
}

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
    skip: Option<bool>,
    alias: Option<StringId>,
    meta: HashMap<StringId, StringId>,
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
    pub fn add_meta(&mut self, meta_name: StringId, value: StringId) -> bool {
        if self.meta.contains_key(&meta_name) {
            return false;
        }
        self.meta.insert(meta_name, value);
        true
    }

    ///
    pub fn meta(&self) -> &HashMap<StringId, StringId> {
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
