use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use baml_ids::FunctionCallId;
use baml_types::{tracing::events::TraceEvent, BamlValue};

use super::runtime_context::BamlSrcReader;
use crate::{
    client_registry::ClientRegistry, tracing::BamlTracer, tracingv2::storage::storage::BAML_TRACER,
    type_builder::TypeBuilder, CallCtx, RuntimeContext,
};
pub type BamlContext = (
    uuid::Uuid,
    String,
    HashMap<String, BamlValue>,
    FunctionCallId,
);

#[derive(Clone)]
pub struct RuntimeContextManager {
    baml_src_reader: Arc<BamlSrcReader>,
    context: Arc<Mutex<Vec<BamlContext>>>,
    global_tags: Arc<Mutex<HashMap<String, BamlValue>>>,
}

impl fmt::Debug for RuntimeContextManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuntimeContextManager")
            .field("context", &self.context.lock())
            .field("global_tags", &self.global_tags)
            .finish()
    }
}

impl Default for RuntimeContextManager {
    fn default() -> Self {
        Self {
            baml_src_reader: Arc::new(None),
            context: Default::default(),
            global_tags: Default::default(),
        }
    }
}

impl RuntimeContextManager {
    pub fn deep_clone(&self) -> Self {
        Self {
            baml_src_reader: self.baml_src_reader.clone(),
            context: Arc::new(Mutex::new(self.context.lock().unwrap().clone())),
            global_tags: Arc::new(Mutex::new(self.global_tags.lock().unwrap().clone())),
        }
    }

    // pub fn call_id(&self) -> Result<uuid::Uuid> {
    //     self.context
    //         .lock()
    //         .unwrap()
    //         .last()
    //         .map(|(id, ..)| *id)
    //         .ok_or_else(|| anyhow::anyhow!("No call id found. This indicates a bug in BAML. Please report this with a stack trace (RUST_BACKTRACE=1)"))
    // }

    pub fn call_id_stack(&self, allow_empty: bool) -> Result<Vec<FunctionCallId>> {
        let res: Vec<FunctionCallId> = self
            .context
            .lock()
            .unwrap()
            .iter()
            .map(|(.., call_id)| call_id.clone())
            .collect();
        if res.is_empty() && !allow_empty {
            Err(anyhow::anyhow!("No call_id found. This indicates a bug in BAML. Please report this with a stack trace (RUST_BACKTRACE=1)"))
        } else {
            Ok(res)
        }
    }

    pub fn new(baml_src_reader: BamlSrcReader) -> Self {
        Self {
            baml_src_reader: Arc::new(baml_src_reader),
            context: Default::default(),
            global_tags: Default::default(),
        }
    }

    pub fn upsert_tags(&self, tags: HashMap<String, BamlValue>) {
        let call_id_stack = {
            let mut ctx = self.context.lock().unwrap();
            if let Some((.., last_tags, _)) = ctx.last_mut() {
                last_tags.extend(tags.clone());
            } else {
                self.global_tags.lock().unwrap().extend(tags.clone());
            }

            // Extract call_id_stack while we have the lock to avoid deadlock
            ctx.iter()
                .map(|(.., call_id)| call_id.clone())
                .collect::<Vec<FunctionCallId>>()
        };

        if !call_id_stack.is_empty() {
            // Get all tags: global tags + current context tags (which now include the new tags)
            let all_tags = {
                let mut all_tags = self.global_tags.lock().unwrap().clone();
                let ctx = self.context.lock().unwrap();
                if let Some((.., ctx_tags, _)) = ctx.last() {
                    all_tags.extend(ctx_tags.iter().map(|(k, v)| (k.clone(), v.clone())));
                }
                all_tags
            };

            let event = TraceEvent::new_set_tags(
                call_id_stack,
                serde_json::Map::from_iter(
                    all_tags
                        .into_iter()
                        .map(|(k, v)| (k, serde_json::to_value(v).unwrap_or_default())),
                ),
            );
            BAML_TRACER.lock().unwrap().put(Arc::new(event));
        }
    }

    fn clone_last_tags(&self) -> HashMap<String, BamlValue> {
        self.context
            .lock()
            .unwrap()
            .last()
            .map(|(_, _, tags, _)| tags.clone())
            .unwrap_or_default()
    }

    // Note, after entering, calling ctx.call_id() will return the call id of the old context still.
    // Returns the user tags, and global tags separately.
    // The user tags get replicated to downstream contexts.
    pub fn enter(
        &self,
        name: &str,
    ) -> (
        uuid::Uuid,
        Vec<FunctionCallId>,
        HashMap<String, BamlValue>,
        HashMap<String, BamlValue>,
    ) {
        let last_tags = self.clone_last_tags();
        let call = uuid::Uuid::new_v4();
        let call_id = FunctionCallId::new();
        let mut ctx = self.context.lock().unwrap();
        ctx.push((call, name.to_string(), last_tags.clone(), call_id.clone()));

        let call_stack = ctx
            .iter()
            .map(|(.., call_id)| call_id.clone())
            .collect::<Vec<_>>();

        // log::info!("Entering with: {:#?}", ctx);
        let mut last_tags = last_tags;
        for (k, v) in self.global_tags.lock().unwrap().iter() {
            last_tags.entry(k.clone()).or_insert_with(|| v.clone());
        }
        let global_tags = self.global_tags.lock().unwrap().clone();
        (call, call_stack, last_tags, global_tags)
    }

    // This returns ALL tags together (global and user)
    pub fn exit(&self) -> Option<(uuid::Uuid, Vec<CallCtx>, HashMap<String, BamlValue>)> {
        let mut ctx = self.context.lock().unwrap();
        log::trace!("Exiting: {ctx:#?}");

        let tracing_v1_call_stack = ctx
            .iter()
            .map(|(call, name, _, call_id)| CallCtx {
                call_id: *call,
                name: name.clone(),
                new_call_id: call_id.clone(),
            })
            .collect();

        let (id, _, mut tags, new_id) = ctx.pop()?;

        for (k, v) in self.global_tags.lock().unwrap().iter() {
            tags.entry(k.clone()).or_insert_with(|| v.clone());
        }

        Some((id, tracing_v1_call_stack, tags))
    }

    pub fn create_ctx(
        &self,
        type_builder: Option<&TypeBuilder>,
        client_registry: Option<&ClientRegistry>,
        // the tracer initializes the new call_id,
        // and then passes it back in here for a new context. It's kind of circular since tracer uses this class to _create_ the call_id....
        // Anyway RuntimeCtx is passed everywhere and we need to know what the last call_id that the tracer created was.
        // tl;dr
        // 1. Tracer creates a new call id using the current context that's passe dinto call_function()
        // 2. Tracer passes the call_id back in here for a new context
        // 3. profit
        env_vars: HashMap<String, String>,
        call_id_stack: Vec<FunctionCallId>,
    ) -> Result<RuntimeContext> {
        // let mut tags = self.global_tags.lock().unwrap().clone();
        // let ctx_tags = {
        //     self.context
        //         .lock()
        //         .unwrap()
        //         .last()
        //         .map(|(.., x, _)| x)
        //         .cloned()
        //         .unwrap_or_default()
        // };
        // tags.extend(ctx_tags);
        let tags = {
            let mut tags = self.global_tags.lock().unwrap().clone();
            let ctx = self.context.lock().unwrap();
            let ctx = ctx.last();
            if let Some((.., ctx_tags, _)) = ctx {
                tags.extend(ctx_tags.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
            tags
        };

        let (cls, enm, als, rec_cls, rec_als) = type_builder
            .map(TypeBuilder::to_overrides)
            .unwrap_or_default();

        let mut ctx = RuntimeContext::new(
            self.baml_src_reader.clone(),
            env_vars,
            tags,
            Default::default(),
            cls,
            enm,
            als,
            rec_cls,
            rec_als,
            call_id_stack,
        );

        ctx.client_overrides = match client_registry {
            Some(cr) => Some(
                cr.to_clients(&ctx)
                    .with_context(|| "Failed to create clients from client_registry")?,
            ),
            None => None,
        };

        Ok(ctx)
    }

    // for tests only
    pub fn create_ctx_with_default(&self) -> RuntimeContext {
        let ctx = self.context.lock().unwrap();

        RuntimeContext::new(
            self.baml_src_reader.clone(),
            HashMap::new(),
            ctx.last().map(|(.., x, _)| x).cloned().unwrap_or_default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            vec![FunctionCallId::new()],
        )
    }

    pub fn context_depth(&self) -> usize {
        let ctx = self.context.lock().unwrap();
        ctx.len()
    }
}
