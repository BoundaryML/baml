use std::{collections::HashMap, sync::Arc};

use baml_builtins2::{PromptAst, PromptAstSimple};
use bex_heap::TlabHolder;
use bex_vm_types::{
    HeapPtr,
    types::{Object, Value},
};

use super::{
    Continuation, NativeCallResult, make_to_string_callee,
    root::{StringRenderState, StructuralRenderSink, collect_to_string_overrides, render_to_sink},
};
use crate::{
    BexVm,
    package_ai::{
        BamlClassPrompt, BamlNamespaceInternal, BamlPackageAi, PackageAiImpl, view as ai_view,
    },
};

#[derive(Default)]
struct PromptContentSink {
    parts: Vec<Arc<PromptAstSimple>>,
}

impl PromptContentSink {
    fn into_content(mut self) -> Arc<PromptAstSimple> {
        // Legacy renderer parity: when media splits a message into parts,
        // every text chunk was trimmed and whitespace-only chunks between
        // media dropped. Text-only content is left untouched.
        if self
            .parts
            .iter()
            .any(|part| matches!(part.as_ref(), PromptAstSimple::Media(_)))
        {
            self.parts = self
                .parts
                .into_iter()
                .filter_map(|part| match part.as_ref() {
                    PromptAstSimple::String(text) => {
                        let trimmed = text.trim();
                        if trimmed.is_empty() {
                            None
                        } else if trimmed.len() == text.len() {
                            Some(part)
                        } else {
                            Some(Arc::new(PromptAstSimple::String(trimmed.to_string())))
                        }
                    }
                    _ => Some(part),
                })
                .collect();
        }
        match self.parts.len() {
            0 => Arc::new(PromptAstSimple::String(String::new())),
            1 => self.parts.pop().unwrap(),
            _ => Arc::new(PromptAstSimple::Multiple(self.parts)),
        }
    }
}

impl StructuralRenderSink for PromptContentSink {
    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        if let Some(last) = self.parts.last_mut()
            && let Some(PromptAstSimple::String(current)) = Arc::get_mut(last)
        {
            current.push_str(text);
            return;
        }
        self.parts
            .push(Arc::new(PromptAstSimple::String(text.to_string())));
    }

    fn try_push_special(&mut self, vm: &BexVm, value: Value) -> bool {
        let Some(media) = super::json::read_media_value(vm, value) else {
            return false;
        };
        self.parts.push(Arc::new(PromptAstSimple::Media(media)));
        true
    }
}

struct PromptRole {
    name: String,
    metadata: serde_json::Value,
    // The assembler prepends a synthetic "system" role when a prompt has no
    // leading marker (see ai internal helpers). Legacy parity: only messages
    // opened by an AUTHORED marker are edge-trimmed; a role-less prompt keeps
    // its rendered bytes.
    synthetic: bool,
}

// Legacy renderer parity: each chat message's content is trimmed of leading
// and trailing whitespace (whitespace-only edge text parts removed, edge text
// parts trimmed). Media parts at the edges are untouched. Without this, the
// newline after a `${role(...)}` marker line and the blank line before the
// next marker leak into every explicit-role message.
fn trim_message_edges(sink: &mut PromptContentSink) {
    while let Some(first) = sink.parts.first() {
        match first.as_ref() {
            PromptAstSimple::String(text) => {
                let trimmed = text.trim_start();
                if trimmed.is_empty() {
                    sink.parts.remove(0);
                } else {
                    if trimmed.len() != text.len() {
                        sink.parts[0] = Arc::new(PromptAstSimple::String(trimmed.to_string()));
                    }
                    break;
                }
            }
            _ => break,
        }
    }
    while let Some(last) = sink.parts.last() {
        match last.as_ref() {
            PromptAstSimple::String(text) => {
                let trimmed = text.trim_end();
                if trimmed.is_empty() {
                    sink.parts.pop();
                } else {
                    if trimmed.len() != text.len() {
                        let index = sink.parts.len() - 1;
                        sink.parts[index] = Arc::new(PromptAstSimple::String(trimmed.to_string()));
                    }
                    break;
                }
            }
            _ => break,
        }
    }
}

fn message(role: PromptRole, mut content: PromptContentSink, trim: bool) -> Arc<PromptAst> {
    if trim {
        trim_message_edges(&mut content);
    }
    Arc::new(PromptAst::Message {
        role: role.name,
        content: content.into_content(),
        metadata: role.metadata,
    })
}

fn prompt_role(vm: &BexVm, value: Value) -> Option<PromptRole> {
    let ptr = value.as_object_ptr()?;
    let (class_ptr, fields) = match vm.get_object(ptr) {
        Object::Instance(instance) => (
            instance.class,
            instance.field_values().collect::<Vec<Value>>(),
        ),
        _ => return None,
    };
    let (name_index, metadata_index) = match vm.get_object(class_ptr) {
        Object::Class(class)
            if class
                .name
                .declared()
                .is_some_and(|qtn| qtn.render_dotted(false) == "ai.Role") =>
        {
            (
                class.fields.iter().position(|field| field.name == "name")?,
                class
                    .fields
                    .iter()
                    .position(|field| field.name == "metadata")?,
            )
        }
        _ => return None,
    };
    let name = vm.as_string(fields.get(name_index)?).ok()?;
    let mut metadata = super::json::value_to_serde(vm, *fields.get(metadata_index)?);
    let mut synthetic = false;
    if let serde_json::Value::Object(entries) = &mut metadata {
        synthetic = entries.remove("__baml_synthetic_role").is_some();
    }
    Some(PromptRole {
        name: name.as_str().to_string(),
        metadata,
        synthetic,
    })
}

struct PromptAssembly {
    parts: Vec<Value>,
    values: Vec<Value>,
    pending: Vec<HeapPtr>,
    results: Vec<String>,
}

impl PromptAssembly {
    fn finish(self, vm: &mut BexVm) -> NativeCallResult {
        let mut render_state = StringRenderState::with_overrides(&self.pending, &self.results);
        let mut pending_messages: Vec<(PromptRole, PromptContentSink)> = Vec::new();
        let mut current_role: Option<PromptRole> = None;
        let mut content = PromptContentSink::default();

        for (index, value) in self.values.iter().copied().enumerate() {
            if let Some(part) = self.parts.get(index)
                && let Ok(part) = vm.as_string(part)
            {
                content.push_text(part.as_str());
            }

            if let Some(role) = prompt_role(vm, value) {
                if let Some(previous_role) = current_role.replace(role) {
                    pending_messages.push((previous_role, std::mem::take(&mut content)));
                }
            } else {
                render_to_sink(vm, value, false, 0, &mut render_state, &mut content);
            }
        }

        if let Some(part) = self.parts.get(self.values.len())
            && let Ok(part) = vm.as_string(part)
        {
            content.push_text(part.as_str());
        }

        let ast = match current_role {
            Some(role) => {
                pending_messages.push((role, content));
                // Legacy renderer parity: when the prompt carries ANY authored
                // role marker, every message's content is edge-trimmed —
                // including the leading implicit system chunk. A prompt with
                // no authored markers (only the assembler's synthetic system
                // role) keeps its rendered bytes untouched.
                let trim = pending_messages.iter().any(|(role, _)| !role.synthetic);
                let mut messages: Vec<Arc<PromptAst>> = pending_messages
                    .into_iter()
                    .map(|(role, content)| message(role, content, trim))
                    .collect();
                if messages.len() == 1 {
                    messages.pop().unwrap()
                } else {
                    Arc::new(PromptAst::Vec(messages))
                }
            }
            None => Arc::new(PromptAst::Simple(content.into_content())),
        }
        .merge_adjacent();

        let prompt_ast_class = vm.resolve_class("ai.Prompt");
        let data = Value::object(vm.alloc_rust_data(ast));
        NativeCallResult::Done(Value::object(
            vm.alloc_instance(prompt_ast_class, vec![data]),
        ))
    }

    fn dispatch_next(self, vm: &mut BexVm) -> NativeCallResult {
        let Some(&next_ptr) = self.pending.get(self.results.len()) else {
            return self.finish(vm);
        };
        let Some(callee) = make_to_string_callee(vm, Value::object(next_ptr)) else {
            return self.finish(vm);
        };
        NativeCallResult::YieldToCall {
            callee,
            args: vec![],
            type_args: vec![],
            continuation: Box::new(self),
        }
    }
}

impl Continuation for PromptAssembly {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        self.results.push(
            vm.as_string(&value)
                .map(|result| result.as_str().to_string())
                .unwrap_or_default(),
        );
        self.dispatch_next(vm)
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        self.parts
            .iter()
            .chain(&self.values)
            .filter_map(Value::as_object_ptr)
            .chain(self.pending.iter().copied())
            .collect()
    }

    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        for value in self.parts.iter_mut().chain(&mut self.values) {
            if let Some(ptr) = value.as_object_ptr()
                && let Some(&new_ptr) = forwarding.get(&ptr)
            {
                *value = Value::object(new_ptr);
            }
        }
        for ptr in &mut self.pending {
            if let Some(&new_ptr) = forwarding.get(ptr) {
                *ptr = new_ptr;
            }
        }
    }
}

impl BamlClassPrompt for PackageAiImpl {
    fn text(vm: &BexVm, prompt: &ai_view::Prompt<'_>) -> bex_str::BexStr {
        let data = prompt.instance.load_field(0);
        let prompt = vm
            .as_rust_data::<PromptAst>(&data)
            .expect("ai.Prompt._data must contain baml_builtins2::PromptAst");
        bex_str::BexStr::from(prompt.render_text())
    }

    fn messages(vm: &mut BexVm, prompt: &Value) -> Vec<Value> {
        let messages = {
            let instance = vm
                .as_instance(prompt)
                .expect("ai.Prompt.messages receiver must be an ai.Prompt instance");
            let data = instance.load_field(0);
            vm.as_rust_data::<PromptAst>(&data)
                .expect("ai.Prompt._data must contain baml_builtins2::PromptAst")
                .to_structured_messages()
        };
        let message_class = vm.resolve_class("ai.PromptMessage");
        messages
            .into_iter()
            .map(|(role, content, metadata)| {
                let role = Value::object(vm.alloc_string(role));
                let readable = Value::object(vm.alloc_string(content.to_text()));
                let parts = prompt_content_values(vm, content.as_ref());
                let parts =
                    Value::object(vm.alloc_array(bex_vm_types::RealizedTy::unknown(), parts));
                let metadata = prompt_metadata_value(vm, metadata);
                Value::object(
                    vm.alloc_instance(message_class, vec![role, readable, parts, metadata]),
                )
            })
            .collect()
    }
}

/// A message's per-message metadata as the `map<string, json>` the stdlib
/// declares. A node that carried none holds `Value::Null` in the AST, so
/// anything that is not a JSON object materializes as an empty map rather than
/// breaking the declared field type.
fn prompt_metadata_value(vm: &mut BexVm, metadata: serde_json::Value) -> Value {
    let metadata = match metadata {
        serde_json::Value::Object(_) => metadata,
        _ => serde_json::Value::Object(serde_json::Map::new()),
    };
    super::json::serde_to_value(vm, &metadata)
}

fn prompt_content_values(vm: &mut BexVm, content: &PromptAstSimple) -> Vec<Value> {
    match content {
        PromptAstSimple::String(text) => vec![Value::object(vm.alloc_string(text.clone()))],
        PromptAstSimple::Media(media) => {
            let class_name = match media.kind {
                baml_type::MediaKind::Image => "baml.media.Image",
                baml_type::MediaKind::Audio => "baml.media.Audio",
                baml_type::MediaKind::Video => "baml.media.Video",
                baml_type::MediaKind::Pdf => "baml.media.Pdf",
                baml_type::MediaKind::Generic => {
                    panic!("generic media cannot inhabit a concrete prompt part")
                }
            };
            let class = vm.resolve_class(class_name);
            let data = Value::object(vm.alloc_rust_data(media.clone()));
            vec![Value::object(vm.alloc_instance(class, vec![data]))]
        }
        PromptAstSimple::Multiple(parts) => parts
            .iter()
            .flat_map(|part| prompt_content_values(vm, part.as_ref()))
            .collect(),
    }
}

impl BamlNamespaceInternal for PackageAiImpl {
    fn assemble_prompt(vm: &mut BexVm, parts: &[Value], values: &[Value]) -> NativeCallResult {
        let mut pending = Vec::new();
        for value in values.iter().copied() {
            if prompt_role(vm, value).is_none() {
                collect_to_string_overrides(vm, value, &mut pending);
            }
        }

        PromptAssembly {
            parts: parts.to_vec(),
            values: values.to_vec(),
            pending,
            results: Vec::new(),
        }
        .dispatch_next(vm)
    }
}

impl BamlPackageAi for PackageAiImpl {}
