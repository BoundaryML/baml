use std::any::TypeId;

use lsp_server::{Notification, RequestId};
// TODO: playground_server is disabled for now
// use playground_server::WebviewRouterMessage;
use rustc_hash::FxHashMap;
use serde_json::{Value, json};
use tokio::sync::broadcast;

use super::{ClientSender, schedule::Task};

type ResponseBuilder<'s> = Box<dyn FnOnce(lsp_server::Response) -> Task<'s>>;

pub(crate) struct Client<'s> {
    notifier: Notifier,
    responder: Responder,
    pub(super) requester: Requester<'s>,
}

#[derive(Clone)]
pub struct Notifier {
    client_sender: ClientSender,
    // TODO: playground_server is disabled for now, using dummy () type
    to_webview_router_tx: broadcast::Sender<()>,
    lsp_methods_to_forward_to_webview: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct Responder(ClientSender);

pub(crate) struct Requester<'s> {
    sender: ClientSender,
    next_request_id: i32,
    response_handlers: FxHashMap<lsp_server::RequestId, ResponseBuilder<'s>>,
}

impl Client<'_> {
    pub(super) fn new(
        sender: ClientSender,
        // TODO: playground_server is disabled for now, using dummy () type
        to_webview_router_tx: broadcast::Sender<()>,
        lsp_methods_to_forward_to_webview: Vec<String>,
    ) -> Self {
        Self {
            notifier: Notifier {
                client_sender: sender.clone(),
                to_webview_router_tx,
                lsp_methods_to_forward_to_webview,
            },
            responder: Responder(sender.clone()),
            requester: Requester {
                sender,
                next_request_id: 1,
                response_handlers: FxHashMap::default(),
            },
        }
    }

    pub(super) fn notifier(&self) -> Notifier {
        self.notifier.clone()
    }

    pub(super) fn responder(&self) -> Responder {
        self.responder.clone()
    }
}

impl Notifier {
    pub(crate) fn notify<N>(&self, params: N::Params) -> anyhow::Result<()>
    where
        N: lsp_types::notification::Notification,
    {
        self.notify_raw(N::METHOD.to_string(), params)
    }

    /// Type-unsafe version of self.notify(). We have too many things that just do json!-style notifications.
    pub(crate) fn notify_raw(
        &self,
        method: String,
        params: impl serde::Serialize,
    ) -> anyhow::Result<()> {
        let notification = Notification::new(method.clone(), params);
        let message = lsp_server::Message::Notification(notification.clone());
        // Send to client
        self.client_sender.send(message)?;
        // TODO: playground_server forwarding is disabled for now
        // // Use configuration instead of hardcoded list
        // if self.lsp_methods_to_forward_to_webview.contains(&method) {
        //     let _ = self
        //         .to_webview_router_tx
        //         .send(WebviewRouterMessage::SendMessageToWebview(
        //             playground_server::WebviewCommand::LspMessage(notification),
        //         ))
        //         .inspect_err(|e| {
        //             tracing::error!(
        //                 "Failed to send SEND_LSP_NOTIFICATION_TO_WEBVIEW message to webview: {e}"
        //             );
        //         });
        // }
        Ok(())
    }

    pub(crate) fn notify_baml_error(&self, msg: &str) -> anyhow::Result<()> {
        self.notify_raw(
            "baml/message".to_string(),
            json!({
                "type": "error",
                "message": msg,
                "durationMs": 7000,
            }),
        )
    }
    pub(crate) fn notify_baml_info(&self, msg: &str) -> anyhow::Result<()> {
        self.notify_raw(
            "baml/message".to_string(),
            json!({
                "type": "info",
                "message": msg,
                "durationMs": 4000,
            }),
        )
    }
}

impl Responder {
    pub(crate) fn respond<R>(
        &self,
        id: RequestId,
        result: crate::server::Result<R>,
    ) -> anyhow::Result<()>
    where
        R: serde::Serialize,
    {
        self.0.send(
            match result {
                Ok(res) => lsp_server::Response::new_ok(id, res),
                Err(crate::server::api::Error { code, error }) => {
                    lsp_server::Response::new_err(id, code as i32, format!("{error}"))
                }
            }
            .into(),
        )
    }
}

impl<'s> Requester<'s> {
    /// Sends a request of kind `R` to the client, with associated parameters.
    /// The task provided by `response_handler` will be dispatched as soon as the response
    /// comes back from the client.
    pub(crate) fn request<R>(
        &mut self,
        params: R::Params,
        response_handler: impl Fn(R::Result) -> Task<'s> + 'static,
    ) -> anyhow::Result<()>
    where
        R: lsp_types::request::Request,
    {
        let serialized_params = serde_json::to_value(params)?;

        self.response_handlers.insert(
            self.next_request_id.into(),
            Box::new(move |response: lsp_server::Response| {
                match (response.error, response.result) {
                    (Some(err), _) => {
                        tracing::error!(
                            "Got an error from the client (code {}): {}",
                            err.code,
                            err.message
                        );
                        Task::nothing()
                    }
                    (None, Some(response)) => match serde_json::from_value(response) {
                        Ok(response) => response_handler(response),
                        Err(error) => {
                            tracing::error!("Failed to deserialize response from server: {error}");
                            Task::nothing()
                        }
                    },
                    (None, None) => {
                        if TypeId::of::<R::Result>() == TypeId::of::<()>() {
                            // We can't call `response_handler(())` directly here, but
                            // since we _know_ the type expected is `()`, we can use
                            // `from_value(Value::Null)`. `R::Result` implements `DeserializeOwned`,
                            // so this branch works in the general case but we'll only
                            // hit it if the concrete type is `()`, so the `unwrap()` is safe here.
                            response_handler(serde_json::from_value(Value::Null).unwrap());
                        } else {
                            tracing::error!(
                                "Server response was invalid: did not contain a result or error"
                            );
                        }
                        Task::nothing()
                    }
                }
            }),
        );

        self.sender
            .send(lsp_server::Message::Request(lsp_server::Request {
                id: self.next_request_id.into(),
                method: R::METHOD.into(),
                params: serialized_params,
            }))?;

        self.next_request_id += 1;

        Ok(())
    }

    pub(crate) fn pop_response_task(&mut self, response: lsp_server::Response) -> Task<'s> {
        if let Some(handler) = self.response_handlers.remove(&response.id) {
            handler(response)
        } else {
            tracing::error!(
                "Received a response with ID {}, which was not expected",
                response.id
            );
            Task::nothing()
        }
    }
}
