mod iterative_parser;

#[cfg(test)]
mod test_iterative_parser;

pub use iterative_parser::{parse_jsonish_value, JSONishOptions};
