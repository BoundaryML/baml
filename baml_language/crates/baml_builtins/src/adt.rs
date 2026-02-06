use std::{collections::VecDeque, sync::Arc};

// Do not clone. only clone as arc.
#[derive(Debug)]
pub struct MediaValue {
    pub random_id: usize,
    pub kind: baml_base::MediaKind,
    pub mime_type: Option<String>,
    // This is unsafe because technically this is writeable
    // so we need to use the unsafe_rw_lock to protect it
    // which then needs to be accessed using the unsafe_cell.get()
    content: std::cell::UnsafeCell<MediaContent>,
    content_rw_lock: std::sync::RwLock<()>,
}

// this is necessary to allow the MediaValue to be usable via async and arc.
// UnsafeCell is not Sync, so we need to make it Sync manually.
// This is safe because every access to content is only exposed via explict
// read_content/write_content methods.
#[allow(unsafe_code)]
unsafe impl Sync for MediaValue {}

impl PartialEq for MediaValue {
    fn eq(&self, other: &Self) -> bool {
        self.random_id == other.random_id
    }
}

impl Eq for MediaValue {}

impl std::hash::Hash for MediaValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.random_id.hash(state);
    }
}

static GLOBAL_MEDIA_VALUE_ID: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

impl MediaValue {
    pub fn new(
        kind: baml_base::MediaKind,
        content: MediaContent,
        mime_type: Option<String>,
    ) -> Self {
        Self {
            content_rw_lock: std::sync::RwLock::new(()),
            random_id: GLOBAL_MEDIA_VALUE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            kind,
            content: std::cell::UnsafeCell::new(content),
            mime_type,
        }
    }

    /// Read content without taking the lock. Use only when no concurrent writes can occur.
    ///
    /// # Safety
    ///
    /// The caller must ensure no other thread is calling `write_content` or
    /// `read_content_unguarded` concurrently. Safe to use from a single thread
    /// or when external synchronization guarantees no writers.
    #[allow(unsafe_code)]
    pub unsafe fn read_content_unguarded<T>(&self, f: impl FnOnce(&MediaContent) -> T) -> T {
        let content = unsafe { &*self.content.get() };
        f(content)
    }

    pub fn read_content<T>(&self, f: impl FnOnce(&MediaContent) -> T) -> T {
        let _guard = self.content_rw_lock.read().unwrap();
        #[allow(unsafe_code)]
        let content = unsafe { &*self.content.get() };
        f(content)
    }

    pub fn write_content<T>(&self, f: impl FnOnce(&mut MediaContent) -> T) -> T {
        let _guard = self.content_rw_lock.write().unwrap();
        #[allow(unsafe_code)]
        let content = unsafe { &mut *self.content.get() };
        f(content)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MediaContent {
    Url {
        url: String,
        base64_data: Option<String>,
    },
    Base64 {
        base64_data: String,
    },
    File {
        file: String,
        base64_data: Option<String>,
    },
}

impl std::fmt::Display for MediaValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.read_content(|content| write!(f, "{}::{}", self.kind, content))
    }
}

impl std::fmt::Display for MediaContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaContent::Url { url, base64_data } => {
                write!(f, "url({url}, loaded={})", base64_data.is_some())
            }
            MediaContent::Base64 { base64_data, .. } => {
                // Show first 5, last 5, and total length for context
                let len = base64_data.len();
                if len <= 10 {
                    write!(f, "base64({base64_data}, len={len})")
                } else {
                    let start = &base64_data[..5];
                    let end = &base64_data[len.saturating_sub(5)..];
                    write!(f, "base64({start}...{end}, len={len})")
                }
            }
            MediaContent::File { file, base64_data } => {
                write!(f, "file({file}, loaded={})", base64_data.is_some())
            }
        }
    }
}

// Do not clone. only clone as arc.
/// A node in the prompt AST tree.
#[derive(Debug, PartialEq, Clone)]
pub enum PromptAst {
    Simple(std::sync::Arc<PromptAstSimple>),

    /// A message with a role, content, and optional metadata.
    Message {
        role: String,
        content: std::sync::Arc<PromptAstSimple>,
        metadata: serde_json::Value,
    },

    /// A sequence of prompt nodes.
    Vec(Vec<std::sync::Arc<PromptAst>>),
}

#[derive(Debug, PartialEq, Clone)]
pub enum PromptAstSimple {
    String(String),
    Media(std::sync::Arc<MediaValue>),
    Multiple(Vec<std::sync::Arc<PromptAstSimple>>),
}

impl PromptAst {
    // ensures no vec of vecs; preserves document order when flattening nested Vec.
    pub fn merge_adjacent(self: std::sync::Arc<Self>) -> std::sync::Arc<Self> {
        let mut result = Vec::new();
        let mut queue = VecDeque::from([self]);
        while let Some(current) = queue.pop_front() {
            match &*current {
                PromptAst::Simple(_) => result.push(current),
                PromptAst::Message {
                    role,
                    content,
                    metadata,
                } => {
                    let content = content.clone().merge_adjacent();
                    result.push(Arc::new(PromptAst::Message {
                        role: role.clone(),
                        content,
                        metadata: metadata.clone(),
                    }));
                }
                PromptAst::Vec(vec) => {
                    for item in vec.iter().rev() {
                        queue.push_front(item.clone());
                    }
                }
            }
        }

        let mut final_result = Vec::new();
        for item in result {
            let Some(last) = final_result.pop() else {
                final_result.push(item);
                continue;
            };
            if let (PromptAst::Simple(self_simple), PromptAst::Simple(other_simple)) =
                (last.as_ref(), item.as_ref())
            {
                let merged = Arc::new(PromptAstSimple::Multiple(vec![
                    self_simple.clone(),
                    other_simple.clone(),
                ]))
                .merge_adjacent();
                final_result.push(Arc::new(PromptAst::Simple(merged)));
            } else {
                final_result.push(last);
                final_result.push(item);
            }
        }

        if final_result.len() == 1 {
            final_result.pop().unwrap()
        } else {
            Arc::new(PromptAst::Vec(final_result))
        }
    }
}

impl PromptAstSimple {
    pub fn join(self: std::sync::Arc<Self>, other: std::sync::Arc<Self>) -> std::sync::Arc<Self> {
        Arc::new(PromptAstSimple::Multiple(vec![self, other])).merge_adjacent()
    }

    /// Merge adjacent strings, media, and multiple nodes. Preserves document order when flattening nested Multiple.
    fn merge_adjacent(self: std::sync::Arc<Self>) -> std::sync::Arc<Self> {
        let mut result = Vec::new();
        let mut queue = VecDeque::from([self]);
        while let Some(current) = queue.pop_front() {
            match &*current {
                PromptAstSimple::String(_) | PromptAstSimple::Media(_) => {
                    result.push(current);
                }
                PromptAstSimple::Multiple(multiple) => {
                    for item in multiple.iter().rev() {
                        queue.push_front(item.clone());
                    }
                }
            }
        }

        let mut final_result = Vec::new();
        // merge adjacent strings
        for item in result {
            let Some(last) = final_result.pop() else {
                final_result.push(item);
                continue;
            };
            if let (PromptAstSimple::String(self_string), PromptAstSimple::String(other_string)) =
                (last.as_ref(), item.as_ref())
            {
                final_result.push(Arc::new(PromptAstSimple::String(
                    self_string.clone() + other_string.as_str(),
                )));
            } else {
                final_result.push(last);
                final_result.push(item);
            }
        }

        if final_result.len() == 1 {
            final_result.pop().unwrap()
        } else {
            Arc::new(PromptAstSimple::Multiple(final_result))
        }
    }
}

impl From<String> for PromptAstSimple {
    fn from(value: String) -> Self {
        PromptAstSimple::String(value)
    }
}

impl From<std::sync::Arc<MediaValue>> for PromptAstSimple {
    fn from(value: std::sync::Arc<MediaValue>) -> Self {
        PromptAstSimple::Media(value)
    }
}

impl From<Vec<std::sync::Arc<PromptAstSimple>>> for PromptAstSimple {
    fn from(value: Vec<std::sync::Arc<PromptAstSimple>>) -> Self {
        PromptAstSimple::Multiple(value)
    }
}

impl From<std::sync::Arc<PromptAstSimple>> for PromptAst {
    fn from(value: std::sync::Arc<PromptAstSimple>) -> Self {
        PromptAst::Simple(value)
    }
}

impl<T: Into<PromptAstSimple>> From<T> for PromptAst {
    fn from(value: T) -> Self {
        PromptAst::Simple(std::sync::Arc::new(value.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple(s: &str) -> Arc<PromptAst> {
        Arc::new(PromptAst::Simple(Arc::new(PromptAstSimple::String(
            s.to_string(),
        ))))
    }

    #[test]
    fn test_prompt_ast_merge_adjacent_single_simple() {
        let ast = simple("hello");
        let merged = ast.merge_adjacent();
        assert!(matches!(&*merged, PromptAst::Simple(_)));
        if let PromptAst::Simple(s) = &*merged {
            assert!(matches!(&**s, PromptAstSimple::String(t) if t == "hello"));
        }
    }

    #[test]
    fn test_prompt_ast_merge_adjacent_two_simples_merged() {
        let a = simple("hello");
        let b = simple(" world");
        let vec_ast = Arc::new(PromptAst::Vec(vec![a, b]));
        let merged = vec_ast.merge_adjacent();
        assert!(matches!(&*merged, PromptAst::Simple(_)));
        if let PromptAst::Simple(s) = &*merged {
            assert!(matches!(&**s, PromptAstSimple::String(t) if t == "hello world"));
        }
    }

    #[test]
    fn test_prompt_ast_merge_adjacent_nested_vec_preserves_order() {
        // [A, Vec([B, C]), D] should flatten in order to A,B,C,D then adjacent Simples merge to one string "abcd".
        let a = simple("a");
        let b = simple("b");
        let c = simple("c");
        let d = simple("d");
        let inner = Arc::new(PromptAst::Vec(vec![b, c]));
        let outer = Arc::new(PromptAst::Vec(vec![a, inner, d]));
        let merged = outer.merge_adjacent();
        // All four adjacent Simple(string) nodes merge into one Simple("abcd")
        assert!(matches!(&*merged, PromptAst::Simple(_)));
        if let PromptAst::Simple(s) = &*merged {
            assert!(matches!(&**s, PromptAstSimple::String(t) if t == "abcd"));
        }
    }

    #[test]
    fn test_prompt_ast_simple_join_merges_strings() {
        let a = Arc::new(PromptAstSimple::String("foo".to_string()));
        let b = Arc::new(PromptAstSimple::String("bar".to_string()));
        let joined = a.join(b);
        assert!(matches!(&*joined, PromptAstSimple::String(s) if s == "foobar"));
    }

    #[test]
    fn test_prompt_ast_simple_merge_adjacent_multiple_preserves_order() {
        // Two adjacent Simples: first has nested Multiple([a, Multiple([b]), c]), second is "d".
        // Flattening preserves order; adjacent strings merge, so we get one Simple("abcd").
        let a = Arc::new(PromptAstSimple::String("a".to_string()));
        let b = Arc::new(PromptAstSimple::String("b".to_string()));
        let c = Arc::new(PromptAstSimple::String("c".to_string()));
        let d = Arc::new(PromptAstSimple::String("d".to_string()));
        let inner = Arc::new(PromptAstSimple::Multiple(vec![b]));
        let multi = Arc::new(PromptAstSimple::Multiple(vec![a, inner, c]));
        let first = Arc::new(PromptAst::Simple(multi));
        let second = Arc::new(PromptAst::Simple(d));
        let vec_ast = Arc::new(PromptAst::Vec(vec![first, second]));
        let merged = vec_ast.merge_adjacent();
        assert!(matches!(&*merged, PromptAst::Simple(_)));
        if let PromptAst::Simple(s) = &*merged {
            assert!(matches!(&**s, PromptAstSimple::String(t) if t == "abcd"));
        }
    }
}
