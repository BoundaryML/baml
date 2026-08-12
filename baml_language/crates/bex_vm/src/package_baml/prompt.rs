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
}

fn message(role: PromptRole, content: PromptContentSink) -> Arc<PromptAst> {
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
        Object::Class(class) if class.name.render_dotted(false) == "baml.prompt.Role" => (
            class.fields.iter().position(|field| field.name == "name")?,
            class
                .fields
                .iter()
                .position(|field| field.name == "metadata")?,
        ),
        _ => return None,
    };
    let name = vm.as_string(fields.get(name_index)?).ok()?;
    let metadata = super::json::value_to_serde(vm, *fields.get(metadata_index)?);
    Some(PromptRole {
        name: name.as_str().to_string(),
        metadata,
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
        let mut messages: Vec<Arc<PromptAst>> = Vec::new();
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
                    messages.push(message(previous_role, std::mem::take(&mut content)));
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
                messages.push(message(role, content));
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
                .to_messages()
        };
        let message_class = vm.resolve_class("ai.PromptMessage");
        messages
            .into_iter()
            .map(|(role, content)| {
                let role = Value::object(vm.alloc_string(role));
                let content = Value::object(vm.alloc_string(content));
                Value::object(vm.alloc_instance(message_class, vec![role, content]))
            })
            .collect()
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
