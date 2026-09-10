use std::collections::BTreeMap;

/// Owned primitive values permitted in execution scope metadata.
#[derive(Clone, Debug, PartialEq)]
pub enum ScopeValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

/// An effective execution scope. Metadata never contains null.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScopeContext {
    pub metadata: BTreeMap<String, ScopeValue>,
    pub distinct_id: Option<String>,
}

/// Edits owned by a single-use local id. `None` metadata values remove keys.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScopeContextPatch {
    pub metadata: BTreeMap<String, Option<ScopeValue>>,
    pub distinct_id: Option<String>,
}

impl ScopeContextPatch {
    pub fn is_empty(&self) -> bool {
        self.metadata.is_empty() && self.distinct_id.is_none()
    }

    pub fn compose(&mut self, later: Self) {
        self.metadata.extend(later.metadata);
        if later.distinct_id.is_some() {
            self.distinct_id = later.distinct_id;
        }
    }

    pub fn apply(&self, parent: &ScopeContext) -> ScopeContext {
        let mut context = parent.clone();
        for (key, value) in &self.metadata {
            if let Some(value) = value {
                context.metadata.insert(key.clone(), value.clone());
            } else {
                context.metadata.remove(key);
            }
        }
        if let Some(distinct_id) = &self.distinct_id {
            context.distinct_id = Some(distinct_id.clone());
        }
        context
    }
}
