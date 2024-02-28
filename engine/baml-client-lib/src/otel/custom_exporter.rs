// use futures_core::future::BoxFuture;
// use opentelemetry_sdk::export::trace::{ExportResult, SpanData, SpanExporter};

// use crate::{
//     api_wrapper::{self, api_interface::BoundaryAPI, APIWrapper},
//     otel::partial_types::PartialLogSchema,
// };

// use super::partial_types::UUIDLutImpl;

// pub(super) struct CustomBackendExporter {
//     api_wrapper: APIWrapper, // Assume APIWrapper is already defined
//     uuid_lut: UUIDLutImpl,
// }

// impl CustomBackendExporter {
//     pub fn new(api_wrapper: APIWrapper) -> Self {
//         Self {
//             api_wrapper,
//             uuid_lut: UUIDLutImpl::default(),
//         }
//     }
// }

// impl std::fmt::Debug for CustomBackendExporter {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("CustomBackendExporter")
//             .field("api_wrapper", &self.api_wrapper)
//             .finish()
//     }
// }

// impl SpanExporter for CustomBackendExporter {
//     fn export(&mut self, batch: Vec<SpanData>) -> BoxFuture<'static, ExportResult> {
//         let items = batch
//             .iter()
//             .filter_map(|span| {
//                 PartialLogSchema::maybe_create(&mut self.uuid_lut, &self.api_wrapper, span)
//             })
//             .flatten()
//             .collect::<Vec<_>>();

//         let counts = (batch.len(), items.len());

//         if counts.0 != counts.1 {
//             println!("Warning: {} spans were dropped", counts.0 - counts.1);
//         }

//         let api_wrapper = self.api_wrapper.clone();
//         Box::pin(async move {
//             for item in items {
//                 let _ = api_wrapper.log_schema(&item).await;
//                 println!(
//                     "name: {} -- id: {} -- parent_id: {:?}",
//                     item.context
//                         .event_chain
//                         .last()
//                         .map_or("??", |f| f.function_name.as_str()),
//                     item.event_id,
//                     item.parent_event_id
//                 );
//             }

//             Ok(())
//         })
//     }
// }
