mod parser;
mod value;
mod xmlish;

pub use parser::{parse, ParseOptions};
pub use value::{Fixes, Value};
pub(crate) use xmlish::parse as parse_xml;
