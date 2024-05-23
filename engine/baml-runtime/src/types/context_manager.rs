use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use baml_types::BamlValue;

use crate::{RuntimeContext, SpanCtx};

type Context = (uuid::Uuid, String, HashMap<String, BamlValue>);

#[derive(Default, Clone)]
pub struct RuntimeContextManager {
    context: Arc<Mutex<Vec<Context>>>,
    env_vars: HashMap<String, String>,
}

impl RuntimeContextManager {
    pub fn deep_clone(&self) -> Self {
        Self {
            context: Arc::new(Mutex::new(self.context.lock().unwrap().clone())),
            env_vars: self.env_vars.clone(),
        }
    }

    pub fn new_from_env_vars(env_vars: HashMap<String, String>) -> Self {
        Self {
            context: Default::default(),
            env_vars,
        }
    }

    pub fn upsert_tags(&self, tags: HashMap<String, BamlValue>) {
        let mut ctx = self.context.lock().unwrap();
        if let Some((.., last_tags)) = ctx.last_mut() {
            last_tags.extend(tags);
        }
    }

    fn clone_last_tags(&self) -> HashMap<String, BamlValue> {
        self.context
            .lock()
            .unwrap()
            .last()
            .map(|(_, _, tags)| tags.clone())
            .unwrap_or_default()
    }

    pub fn enter(&self, name: &str) -> uuid::Uuid {
        let last_tags = self.clone_last_tags();
        let span = uuid::Uuid::new_v4();
        self.context
            .lock()
            .unwrap()
            .push((span.clone(), name.to_string(), last_tags));
        span
    }

    pub fn exit(&self) -> Option<(uuid::Uuid, Vec<SpanCtx>, HashMap<String, BamlValue>)> {
        let mut ctx = self.context.lock().unwrap();

        let prev = ctx
            .iter()
            .map(|(span, name, _)| SpanCtx {
                span_id: span.clone(),
                name: name.clone(),
            })
            .collect();
        let Some((id, _, tags)) = ctx.pop() else {
            return None;
        };

        Some((id, prev, tags))
    }

    pub fn create_ctx(&self) -> RuntimeContext {
        let ctx = self.context.lock().unwrap();

        RuntimeContext {
            env: self.env_vars.clone(),
            tags: ctx.last().map(|(.., x)| x).cloned().unwrap_or_default(),
        }
    }

    pub fn create_ctx_with_default<T: AsRef<str>>(
        &self,
        env_vars: impl Iterator<Item = T>,
    ) -> RuntimeContext {
        let ctx = self.context.lock().unwrap();

        let env_vars = env_vars
            .map(|x| (x.as_ref().to_string(), "".to_string()))
            .chain(self.env_vars.iter().map(|(k, v)| (k.clone(), v.clone())));

        RuntimeContext {
            env: env_vars.collect(),
            tags: ctx.last().map(|(.., x)| x).cloned().unwrap_or_default(),
        }
    }
}
