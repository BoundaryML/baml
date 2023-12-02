mod client;
mod file;
mod template;
mod traits;

mod class;
mod configuration;
mod r#enum;
mod expression;
mod field_type;
mod function;
mod identifier;
mod parser_db;
mod variants;

pub(super) use file::{File, FileCollector};
pub(super) use traits::{TargetLanguage, WithFileName, WithToCode};
