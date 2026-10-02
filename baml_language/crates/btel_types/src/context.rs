//! Owned execution context, independent of whether telemetry is enabled.

use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub enum ContextValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ContextData {
    metadata: BTreeMap<String, ContextValue>,
    distinct_id: Option<String>,
}

static EMPTY: ContextData = ContextData {
    metadata: BTreeMap::new(),
    distinct_id: None,
};

static PROCESS: std::sync::OnceLock<Context> = std::sync::OnceLock::new();

/// Immutable launch context shared by all engines in this process.
pub struct ProcessContext;

impl ProcessContext {
    /// Set before creating engines. Reading the context freezes an empty default.
    /// A second initialization is rejected, even when the values match.
    pub fn initialize(context: Context) -> Result<(), Context> {
        PROCESS.set(context)
    }

    pub fn get() -> &'static Context {
        PROCESS.get_or_init(Context::default)
    }
}

/// A frame owns one pointer, not a copy of its ancestor's metadata.
/// Empty roots need no allocation. Published versions are immutable.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Context(Option<Arc<ContextData>>);

/// Missing keys inherit; null values remove keys. A null identity inherits.
/// Tombstones must survive builder composition until invocation entry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContextPatch {
    pub metadata: BTreeMap<String, Option<ContextValue>>,
    pub distinct_id: Option<String>,
}

impl ContextPatch {
    pub fn is_empty(&self) -> bool {
        self.metadata.is_empty() && self.distinct_id.is_none()
    }

    pub fn compose(&mut self, later: Self) {
        self.metadata.extend(later.metadata);
        if later.distinct_id.is_some() {
            self.distinct_id = later.distinct_id;
        }
    }
}

impl Context {
    /// Identity of an immutable version, not value equality (notably for NaN).
    pub fn same_version(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Some(left), Some(right)) => Arc::ptr_eq(left, right),
            (None, None) => true,
            _ => false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    pub fn metadata(&self) -> &BTreeMap<String, ContextValue> {
        &self.data().metadata
    }

    pub fn distinct_id(&self) -> Option<&str> {
        self.data().distinct_id.as_deref()
    }

    #[must_use]
    pub fn with_patch(&self, patch: &ContextPatch) -> Self {
        if patch.is_empty() {
            return self.clone();
        }
        let mut data = self.data().clone();
        for (key, value) in &patch.metadata {
            if let Some(value) = value {
                data.metadata.insert(key.clone(), value.clone());
            } else {
                data.metadata.remove(key);
            }
        }
        if let Some(id) = &patch.distinct_id {
            data.distinct_id = Some(id.clone());
        }
        if data.metadata.is_empty() && data.distinct_id.is_none() {
            Self::default()
        } else {
            Self(Some(Arc::new(data)))
        }
    }

    fn data(&self) -> &ContextData {
        self.0.as_deref().unwrap_or(&EMPTY)
    }
}

const _: () = assert!(std::mem::size_of::<Context>() == std::mem::size_of::<usize>());

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(key: &str, value: Option<ContextValue>) -> ContextPatch {
        ContextPatch {
            metadata: [(key.to_owned(), value)].into(),
            distinct_id: None,
        }
    }

    #[test]
    fn versions_share_until_updated() {
        let root = Context::default();
        let parent = root.with_patch(&patch("count", Some(ContextValue::Int(1))));
        let child = parent.with_patch(&ContextPatch::default());
        assert!(Arc::ptr_eq(
            parent.0.as_ref().unwrap(),
            child.0.as_ref().unwrap()
        ));
        let updated = child.with_patch(&patch("count", Some(ContextValue::Int(2))));
        assert_eq!(parent.metadata()["count"], ContextValue::Int(1));
        assert_eq!(child.metadata()["count"], ContextValue::Int(1));
        assert_eq!(updated.metadata()["count"], ContextValue::Int(2));
        assert!(root.is_empty());
    }

    #[test]
    fn composition_preserves_deletions_and_inherited_identity() {
        let parent = Context::default().with_patch(&ContextPatch {
            metadata: [("remove".into(), Some(ContextValue::Bool(true)))].into(),
            distinct_id: Some("parent".into()),
        });
        let mut changes = patch("count", Some(ContextValue::Int(1)));
        changes.compose(patch("remove", None));
        changes.compose(patch("count", Some(ContextValue::Int(2))));
        let child = parent.with_patch(&changes);
        assert!(!child.metadata().contains_key("remove"));
        assert_eq!(child.metadata()["count"], ContextValue::Int(2));
        assert_eq!(child.distinct_id(), Some("parent"));
        assert!(parent.metadata().contains_key("remove"));
        changes.compose(ContextPatch {
            distinct_id: Some("child".into()),
            ..ContextPatch::default()
        });
        assert_eq!(parent.with_patch(&changes).distinct_id(), Some("child"));
        assert_eq!(parent.distinct_id(), Some("parent"));
    }

    #[test]
    fn removing_the_last_key_restores_allocation_free_empty_context() {
        let root = Context::default();
        assert!(root.with_patch(&patch("missing", None)).is_empty());
        let nonempty = root.with_patch(&patch("last", Some(ContextValue::Bool(false))));
        assert!(nonempty.with_patch(&patch("last", None)).is_empty());
    }

    #[test]
    fn captured_version_can_cross_threads_without_observing_later_updates() {
        let parent = Context::default().with_patch(&patch("count", Some(ContextValue::Int(1))));
        let launched = parent.clone();
        let parent = parent.with_patch(&patch("count", Some(ContextValue::Int(2))));
        let child = std::thread::spawn(move || {
            assert_eq!(launched.metadata()["count"], ContextValue::Int(1));
            launched.with_patch(&patch("count", Some(ContextValue::Int(3))))
        })
        .join()
        .unwrap();
        assert_eq!(parent.metadata()["count"], ContextValue::Int(2));
        assert_eq!(child.metadata()["count"], ContextValue::Int(3));
    }
}
