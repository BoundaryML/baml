mod code_lens;
mod completion;
mod diagnostic;
mod format;
mod go_to_definition;
mod hover;
mod rename;

pub use code_lens::CodeLens;
pub(super) use completion::Completion;
pub(super) use diagnostic::DocumentDiagnosticRequestHandler;
pub(super) use format::DocumentFormatting;
pub use go_to_definition::GotoDefinition;
pub(super) use hover::Hover;
pub use rename::Rename;
type FormatResponse = Option<Vec<lsp_types::TextEdit>>;
