//! A stateful LSP implementation that calls into the Ruff API.

use std::sync::Arc;

// use crate::baml_project::ProjectDatabase;
use lsp_types::notification::Notification as LSPNotification;
use lsp_types::request::Request;
use parking_lot::Mutex;

use super::notifications::DidSaveTextDocument;
use crate::{
    baml_project::Project,
    server::client::{Notifier, Requester},
    session::{DocumentSnapshot, Session},
};

/// A supertrait for any server request handler.
pub(super) trait RequestHandler {
    type RequestType: Request;
    const METHOD: &'static str = <<Self as RequestHandler>::RequestType as Request>::METHOD;
}

/// A request handler that needs mutable access to the session.
/// This will block the main message receiver loop, meaning that no
/// incoming requests or notifications will be handled while `run` is
/// executing. Try to avoid doing any I/O or long-running computations.
pub(super) trait SyncRequestHandler: RequestHandler {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        requester: &mut Requester,
        params: <<Self as RequestHandler>::RequestType as Request>::Params,
    ) -> super::Result<<<Self as RequestHandler>::RequestType as Request>::Result>;
}

/// A request handler that can be run on a background thread.
pub(super) trait BackgroundDocumentRequestHandler: RequestHandler {
    fn document_url(
        params: &<<Self as RequestHandler>::RequestType as Request>::Params,
    ) -> std::borrow::Cow<'_, lsp_types::Url>;

    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        db: Arc<Mutex<Project>>,
        notifier: Notifier,
        params: <<Self as RequestHandler>::RequestType as Request>::Params,
    ) -> super::Result<<<Self as RequestHandler>::RequestType as Request>::Result>;
}

/// A supertrait for any server notification handler.
pub trait NotificationHandler {
    type NotificationType: LSPNotification;
    const METHOD: &'static str =
        <<Self as NotificationHandler>::NotificationType as LSPNotification>::METHOD;
}

/// A notification handler that needs mutable access to the session.
/// This will block the main message receiver loop, meaning that no
/// incoming requests or notifications will be handled while `run` is
/// executing. Try to avoid doing any I/O or long-running computations.
pub(super) trait SyncNotificationHandler: NotificationHandler {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        requester: &mut Requester,
        params: <<Self as NotificationHandler>::NotificationType as LSPNotification>::Params,
    ) -> super::Result<()>;
}

/// A notification handler that can be run on a background thread.
pub(super) trait BackgroundDocumentNotificationHandler: NotificationHandler {
    /// `document_url` can be implemented automatically with
    /// `define_document_url!(params: &<YourParameterType>)` in the trait
    /// implementation.
    fn document_url(
        params: &<<Self as NotificationHandler>::NotificationType as LSPNotification>::Params,
    ) -> std::borrow::Cow<'_, lsp_types::Url>;

    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        notifier: Notifier,
        params: <<Self as NotificationHandler>::NotificationType as LSPNotification>::Params,
    ) -> super::Result<()>;
}
