use crate::api_wrapper::core_types::{
    Error, EventChain, EventType, LLMEventInput, LLMEventSchema, LLMOutputModel, LogSchema,
    LogSchemaContext, IO,
};
use anyhow::Result;

#[derive(Default, Debug)]
pub(super) struct PartialMetadataType {
    pub(super) model_name: Option<String>,
    pub(super) provider: Option<String>,
    pub(super) input: Option<LLMEventInput>,
    pub(super) output: Option<LLMOutputModel>,
    pub(super) error: Option<Error>,
}

#[derive(Default)]
pub(super) struct PartialLogSchema {
    pub(super) project_id: Option<String>,
    pub(super) event_type: EventType,
    pub(super) root_event_id: String,
    pub(super) event_id: String,
    pub(super) parent_event_id: Option<String>,
    pub(super) context: LogSchemaContext,
    pub(super) io: IO,
    pub(super) error: Option<Error>,
    pub(super) metadata: Vec<PartialMetadataType>,
}

impl PartialLogSchema {
    pub(super) fn get_meta_data_mut(&mut self, create: bool) -> Option<&mut PartialMetadataType> {
        if create {
            self.metadata.push(PartialMetadataType::default());
        }
        self.metadata.last_mut()
    }
}

pub(super) trait Apply<'a, T, S>
where
    S: tracing::Subscriber,
    S: for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn apply(&mut self, event: T, span: &tracing_subscriber::registry::SpanRef<'a, S>);
}

impl PartialLogSchema {
    pub(super) fn to_final(&mut self) -> Result<Vec<LogSchema>> {
        let project_id = self.project_id.clone();

        match &self.event_type {
            EventType::FuncLlm => {
                match self.metadata.len() {
                    0 => Err(anyhow::anyhow!("No metadata found for llm event")),
                    1 => {
                        let (metadata, error) = self.metadata.pop().unwrap().to_final();
                        Ok(vec![LogSchema {
                            project_id,
                            event_type: EventType::FuncLlm,
                            root_event_id: self.root_event_id.clone(),
                            event_id: self.event_id.clone(),
                            parent_event_id: self.parent_event_id.clone(),
                            context: self.context.clone(),
                            io: std::mem::take(&mut self.io),
                            error: error.or(std::mem::take(&mut self.error)),
                            metadata,
                        }])
                    }
                    _ => {
                        let mut event_list = vec![];
                        let last_chain_item = self.context.event_chain.last();
                        if last_chain_item.is_none() {
                            return Err(anyhow::anyhow!("No event chain found"));
                        }
                        let last_chain_item = last_chain_item.unwrap();
                        // Take ownership of each element in `self.metadata`
                        while let Some(partial) = self.metadata.pop() {
                            let (metadata, error) = partial.to_final();
                            let mut schema = LogSchema {
                                project_id: project_id.clone(),
                                event_type: EventType::FuncLlm,
                                root_event_id: self.root_event_id.clone(),
                                event_id: uuid::Uuid::new_v4().to_string(),
                                parent_event_id: Some(self.event_id.clone()),
                                context: self.context.clone(),
                                io: self.io.clone(),
                                error,
                                metadata,
                            };

                            schema.context.event_chain.push(EventChain {
                                function_name: format!(
                                    "{}: {}",
                                    last_chain_item.function_name,
                                    schema.metadata.as_ref().map_or(
                                        format!("Attempt[{}]", event_list.len() + 1).as_str(),
                                        |v| v.model_name.as_ref()
                                    )
                                ),
                                variant_name: last_chain_item.variant_name.clone(),
                            });

                            event_list.push(schema);
                        }

                        if self.error.is_some() && event_list.last().unwrap().error.is_none() {
                            event_list.last_mut().unwrap().error = std::mem::take(&mut self.error);
                        }

                        // set the parent and event ids for the first event in the list from self
                        event_list.first_mut().map(|v| {
                            v.parent_event_id = self.parent_event_id.clone();
                            v.event_id = self.event_id.clone();
                        });

                        Ok(event_list)
                    }
                }
            }
            _ => {
                if !self.metadata.is_empty() {
                    panic!("Metadata should be empty for non-llm events")
                }
                Ok(vec![LogSchema {
                    project_id,
                    event_type: EventType::Log,
                    root_event_id: self.root_event_id.clone(),
                    event_id: self.event_id.clone(),
                    parent_event_id: self.parent_event_id.clone(),
                    context: self.context.clone(),
                    io: std::mem::take(&mut self.io),
                    error: std::mem::take(&mut self.error),
                    metadata: None,
                }])
            }
        }
    }
}

impl PartialMetadataType {
    fn to_final(self) -> (Option<LLMEventSchema>, Option<Error>) {
        if self.provider.is_none() {
            return (None, self.error);
        }
        if self.input.is_none() {
            return (None, self.error);
        }
        if self.output.is_none() && self.error.is_none() {
            return (None, None);
        }

        (
            Some(
                LLMEventSchema {
                    model_name: self
                        .model_name
                        .as_ref()
                        .map_or("<not-specified>", |v| v.as_str())
                        .to_string(),
                    provider: self.provider.unwrap(),
                    input: self.input.unwrap(),
                    output: self.output,
                }
                .into(),
            ),
            self.error,
        )
    }
}
