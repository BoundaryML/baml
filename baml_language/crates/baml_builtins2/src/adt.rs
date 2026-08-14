use std::{collections::VecDeque, sync::Arc};

use crate::media::MediaValue;

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

/// One flattened chat message: role, structural content, and the per-message
/// metadata the `${role(...)}` marker carried (`serde_json::Value::Null` when
/// the node had none). Produced by [`PromptAst::to_structured_messages`].
pub type StructuredMessage = (String, Arc<PromptAstSimple>, serde_json::Value);

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

    /// Flatten this prompt into an ordered list of structural chat messages.
    /// A `Message` node contributes its role and its per-message metadata; a
    /// role-less `Simple` node contributes an empty role and null metadata;
    /// nested `Vec` nodes are flattened in document order. The content stays
    /// structural so stdlib clients can lower media to their provider-specific
    /// wire representation, and the metadata stays attached so they can lower
    /// per-message directives (Anthropic `cache_control`, for example).
    pub fn to_structured_messages(&self) -> Vec<StructuredMessage> {
        let mut out = Vec::new();
        self.collect_structured_messages(&mut out);
        out
    }

    fn collect_structured_messages(&self, out: &mut Vec<StructuredMessage>) {
        match self {
            PromptAst::Simple(content) => out.push((
                String::new(),
                content.clone(),
                serde_json::Value::Null,
            )),
            PromptAst::Message {
                role,
                content,
                metadata,
            } => {
                out.push((role.clone(), content.clone(), metadata.clone()));
            }
            PromptAst::Vec(items) => {
                for item in items {
                    item.collect_structured_messages(out);
                }
            }
        }
    }

    /// Readable projection of [`Self::to_structured_messages`]. Media becomes
    /// a placeholder here, but remains structural in the underlying prompt.
    pub fn to_messages(&self) -> Vec<(String, String)> {
        self.to_structured_messages()
            .into_iter()
            .map(|(role, content, _)| (role, content.to_text()))
            .collect()
    }

    /// Render this prompt as readable plain text: each chat message as a
    /// `[role]` header line followed by its content, messages separated by a
    /// blank line. Role-less content is rendered with no header. Backs the
    /// stdlib `PromptAst.text()` accessor, its `baml.ToString` conversion, and
    /// the CLI's readable value print.
    pub fn render_text(&self) -> String {
        self.to_messages()
            .into_iter()
            .map(|(role, content)| {
                if role.is_empty() {
                    content
                } else {
                    format!("[{role}]\n{content}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

impl PromptAstSimple {
    /// Best-effort readable text for a content chunk. Strings render verbatim;
    /// media renders via its `Display` placeholder (e.g. `image::url(...)`);
    /// `Multiple` concatenates its parts in document order.
    pub fn to_text(&self) -> String {
        match self {
            PromptAstSimple::String(s) => s.clone(),
            PromptAstSimple::Media(media) => media.to_string(),
            PromptAstSimple::Multiple(items) => items.iter().map(|item| item.to_text()).collect(),
        }
    }

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
    #[allow(clippy::many_single_char_names)]
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
    #[allow(clippy::many_single_char_names)]
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

    fn message(role: &str, text: &str) -> Arc<PromptAst> {
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(PromptAstSimple::String(text.to_string())),
            metadata: serde_json::Value::Null,
        })
    }

    #[test]
    fn to_messages_flattens_role_messages_in_order() {
        let ast = PromptAst::Vec(vec![
            message("system", "You are helpful."),
            message("user", "Hi World!"),
        ]);
        assert_eq!(
            ast.to_messages(),
            vec![
                ("system".to_string(), "You are helpful.".to_string()),
                ("user".to_string(), "Hi World!".to_string()),
            ]
        );
    }

    #[test]
    fn to_structured_messages_keeps_per_message_metadata() {
        let ast = PromptAst::Vec(vec![
            Arc::new(PromptAst::Message {
                role: "user".to_string(),
                content: Arc::new(PromptAstSimple::String("Hi World!".to_string())),
                metadata: serde_json::json!({ "cache_control": { "type": "ephemeral" } }),
            }),
            Arc::new(PromptAst::Simple(Arc::new(PromptAstSimple::String(
                "trailing".to_string(),
            )))),
        ]);
        let messages = ast.to_structured_messages();
        assert_eq!(
            messages
                .iter()
                .map(|(role, _, metadata)| (role.as_str(), metadata.clone()))
                .collect::<Vec<_>>(),
            vec![
                (
                    "user",
                    serde_json::json!({ "cache_control": { "type": "ephemeral" } })
                ),
                ("", serde_json::Value::Null),
            ]
        );
    }

    #[test]
    fn to_messages_roleless_simple_has_empty_role() {
        let ast = PromptAst::Simple(Arc::new(PromptAstSimple::String("just text".to_string())));
        assert_eq!(
            ast.to_messages(),
            vec![(String::new(), "just text".to_string())]
        );
    }

    #[test]
    fn render_text_uses_role_headers_and_blank_line_separators() {
        let ast = PromptAst::Vec(vec![
            message("system", "You are helpful."),
            message("user", "Hi World!"),
        ]);
        assert_eq!(
            ast.render_text(),
            "[system]\nYou are helpful.\n\n[user]\nHi World!"
        );
    }

    #[test]
    fn render_text_roleless_simple_has_no_header() {
        let ast = PromptAst::Simple(Arc::new(PromptAstSimple::String("plain".to_string())));
        assert_eq!(ast.render_text(), "plain");
    }

    #[test]
    fn render_text_is_free_of_rust_debug_noise() {
        let ast = message("system", "hello");
        let rendered = ast.render_text();
        for noise in [
            "Adt(",
            "String(",
            "$rust_type",
            "_data",
            "Message {",
            "Null",
        ] {
            assert!(
                !rendered.contains(noise),
                "render_text leaked Rust Debug noise {noise:?}: {rendered}"
            );
        }
    }

    #[test]
    fn simple_to_text_concatenates_multiple_in_order() {
        let simple = PromptAstSimple::Multiple(vec![
            Arc::new(PromptAstSimple::String("foo".to_string())),
            Arc::new(PromptAstSimple::String("bar".to_string())),
        ]);
        assert_eq!(simple.to_text(), "foobar");
    }
}
