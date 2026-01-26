//! Reserved names validation for target languages.
//!
//! This module checks that field names, enum variants, and function parameters
//! don't use reserved keywords from target languages (Python, TypeScript, etc.).

use std::collections::{HashMap, HashSet};

/// Python reserved keywords.
/// Copied from <https://www.w3schools.com/python/python_ref_keywords.asp>
pub const RESERVED_NAMES_PYTHON: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

/// Additional reserved names for function parameters in Python.
/// These are Python built-in types that shouldn't be used as parameter names.
pub const RESERVED_NAMES_FUNCTION_PARAMETERS: &[&str] = &[
    "list", "dict", "set", "tuple", "int", "float", "str", "bool",
];

/// TypeScript is more flexible with identifiers, so we have fewer reserved names.
pub const RESERVED_NAMES_TYPESCRIPT: &[&str] = &[];

/// Ruby reserved keywords that might cause issues.
pub const RESERVED_NAMES_RUBY: &[&str] = &[
    "alias", "and", "begin", "break", "case", "class", "def", "defined?", "do", "else", "elsif",
    "end", "ensure", "false", "for", "if", "in", "module", "nil", "not", "or", "redo", "rescue",
    "retry", "return", "self", "super", "then", "true", "undef", "unless", "until", "when",
    "while", "yield",
];

/// Go reserved keywords.
pub const RESERVED_NAMES_GO: &[&str] = &[
    "break",
    "case",
    "chan",
    "const",
    "continue",
    "default",
    "defer",
    "else",
    "fallthrough",
    "for",
    "func",
    "go",
    "goto",
    "if",
    "import",
    "interface",
    "map",
    "package",
    "range",
    "return",
    "select",
    "struct",
    "switch",
    "var",
];

/// Rust reserved keywords.
/// Includes strict keywords and reserved keywords (for future use).
/// See: <https://doc.rust-lang.org/reference/keywords.html>
/// TODO: Add more reserved keywords as they are used.
pub const RESERVED_NAMES_RUST: &[&str] = &[
    // Strict keywords
    "as", "async", "await", "break", "const", "continue",
];

/// Mode for reserved names checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservedNamesMode {
    /// Checking field names in classes/enums.
    FieldNames,
    /// Checking function parameter names.
    FunctionParameters,
}

/// Output type from a generator (simplified for validation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputType {
    PythonPydantic,
    Typescript,
    Ruby,
    Go,
    Rest,
    BoundaryCloud,
    Rust,
}

impl OutputType {
    /// Parse output type from generator's `output_type` string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "python/pydantic" | "python/pydantic/v1" => Some(OutputType::PythonPydantic),
            "typescript" | "typescript/react" => Some(OutputType::Typescript),
            "ruby/sorbet" => Some(OutputType::Ruby),
            "go" => Some(OutputType::Go),
            "rest/openapi" => Some(OutputType::Rest),
            "boundary-cloud" => Some(OutputType::BoundaryCloud),
            "rust" => Some(OutputType::Rust),
            _ => None,
        }
    }

    /// Get display name for error messages.
    pub fn display_name(&self) -> &'static str {
        match self {
            OutputType::PythonPydantic => "Python",
            OutputType::Typescript => "TypeScript",
            OutputType::Ruby => "Ruby",
            OutputType::Go => "Go",
            OutputType::Rest => "REST",
            OutputType::BoundaryCloud => "Boundary Cloud",
            OutputType::Rust => "Rust",
        }
    }
}

/// For a given set of target languages, construct a map from keyword to the
/// list of target languages in which that identifier is a keyword.
///
/// This is used to generate error messages like:
/// "Field 'return' is a reserved keyword in Python"
pub fn reserved_names_for_outputs(
    output_types: &HashSet<OutputType>,
    mode: ReservedNamesMode,
) -> HashMap<&'static str, Vec<OutputType>> {
    let mut keywords: HashMap<&'static str, Vec<OutputType>> = HashMap::new();

    // Python keywords
    if output_types.contains(&OutputType::PythonPydantic) {
        let python_keywords: &[&str] = match mode {
            ReservedNamesMode::FieldNames => RESERVED_NAMES_PYTHON,
            ReservedNamesMode::FunctionParameters => {
                // For parameters, include both Python keywords and type names
                // We'll handle this by iterating both lists
                RESERVED_NAMES_PYTHON
            }
        };

        for keyword in python_keywords {
            keywords
                .entry(keyword)
                .or_default()
                .push(OutputType::PythonPydantic);
        }

        // Add additional function parameter reserved names
        if mode == ReservedNamesMode::FunctionParameters {
            for keyword in RESERVED_NAMES_FUNCTION_PARAMETERS {
                keywords
                    .entry(keyword)
                    .or_default()
                    .push(OutputType::PythonPydantic);
            }
        }
    }

    // TypeScript keywords
    if output_types.contains(&OutputType::Typescript) {
        for keyword in RESERVED_NAMES_TYPESCRIPT {
            keywords
                .entry(keyword)
                .or_default()
                .push(OutputType::Typescript);
        }
    }

    // Ruby keywords
    if output_types.contains(&OutputType::Ruby) {
        for keyword in RESERVED_NAMES_RUBY {
            keywords.entry(keyword).or_default().push(OutputType::Ruby);
        }
    }

    // Go keywords
    if output_types.contains(&OutputType::Go) {
        for keyword in RESERVED_NAMES_GO {
            keywords.entry(keyword).or_default().push(OutputType::Go);
        }
    }

    // Rust keywords
    if output_types.contains(&OutputType::Rust) {
        for keyword in RESERVED_NAMES_RUST {
            keywords.entry(keyword).or_default().push(OutputType::Rust);
        }
    }

    keywords
}

/// Check if a name is reserved in any of the given output types.
pub fn is_reserved(
    name: &str,
    output_types: &HashSet<OutputType>,
    mode: ReservedNamesMode,
) -> Option<Vec<OutputType>> {
    let reserved = reserved_names_for_outputs(output_types, mode);
    reserved.get(name).cloned()
}
