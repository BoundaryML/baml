use std::collections::{HashMap, HashSet};

use baml_types::GeneratorOutputType;
use once_cell::sync::OnceCell;

// This list of keywords was copied from
// https://www.w3schools.com/python/python_ref_keywords.asp
pub const RESERVED_NAMES_PYTHON: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield", // additional keywords that we use in BAML as they are types
];

pub const RESERVED_NAMES_FUNCTION_PARAMETERS: &[&str] = &[
    "list", "dict", "set", "tuple", "int", "float", "str", "bool",
];

// Typescript is much more flexible in the key names it allows.
pub const RESERVED_NAMES_TYPESCRIPT: &[&str] = &[];

// Keywords that are reserved in BAML.
static BAML_KEYWORDS_CELL: OnceCell<HashSet<&'static str>> = OnceCell::new();

pub fn baml_keywords() -> &'static HashSet<&'static str> {
    BAML_KEYWORDS_CELL.get_or_init(|| {
        HashSet::from([
            // Existing BAML keywords
            "std",
            "this",
            "yield",
            "emit",
            "async",
            "await",
            "image",
            "audio",
            "video",
            "pdf",
            // keywords from other languages
            "and",
            "and_eq",
            "any",
            "assert",
            "asserts",
            "async",
            "await",
            "bigint",
            "bool",
            "box",
            "break",
            "byte",
            "catch",
            "chan",
            "char",
            "char8_t",
            "char16_t",
            "char32_t",
            "class",
            "compl",
            "const",
            "consteval",
            "constexpr",
            "continue",
            "constructor",
            "crossinline",
            "debugger",
            "declare",
            "delegate",
            "defer",
            "do",
            "dynamic",
            "dyn",
            "elif",
            "else",
            "enum",
            "export",
            "extends",
            "extern",
            "false",
            "finally",
            "fixed",
            "float",
            "for",
            "foreach",
            "func",
            "function",
            "get",
            "global",
            "go",
            "goto",
            "implements",
            "import",
            "in",
            "inline",
            "instanceof",
            "int",
            "internal",
            "into",
            "join",
            "keyof",
            "lambda",
            "lazy",
            "let",
            "lock",
            "long",
            "macro",
            "module",
            "move",
            "mut",
            "namespace",
            "native",
            "new",
            "nil",
            "noexcept",
            "nonlocal",
            "not",
            "not_eq",
            "null",
            "nullptr",
            "or",
            "or_eq",
            "package",
            "params",
            "private",
            "protected",
            "protocol",
            "provides",
            "public",
            "pub",
            "readonly",
            "ref",
            "reflexpr",
            "register",
            "reinterpret_cast",
            "requires",
            "return",
            "sbyte",
            "sealed",
            "select",
            "self",
            "Self",
            "set",
            "short",
            "sizeof",
            "some",
            "stackalloc",
            "static",
            "static_assert",
            "static_cast",
            "strictfp",
            "string",
            "struct",
            "subscript",
            "super",
            "switch",
            "template",
            "this",
            "thread_local",
            "throw",
            "throws",
            "trait",
            "transient",
            "transitive",
            "true",
            "try",
            "typedef",
            "typealias",
            "typeof",
            "u16",
            "u32",
            "u64",
            "uint",
            "ulong",
            "union",
            "unsafe",
            "unsized",
            "unsigned",
            "use",
            "using",
            "var",
            "virtual",
            "void",
            "volatile",
            "wchar_t",
            "while",
            "with",
            "xor",
            "xor_eq",
        ])
    })
}

pub enum ReservedNamesMode {
    FieldNames,
    FunctionParameters,
}

/// For a given set of target languages, construct a map from keyword to the
/// list of target languages in which that identifier is a keyword.
///
/// This will be used later to make error messages like, "Could not use name
/// `continue` becase that is a keyword in Python", "Could not use the name
/// `return` because that is a keyword in Python and Typescript".
pub fn reserved_client_code_names(
    generator_output_types: &HashSet<GeneratorOutputType>,
    mode: ReservedNamesMode,
) -> HashMap<&'static str, Vec<GeneratorOutputType>> {
    let mut keywords: HashMap<&str, Vec<GeneratorOutputType>> = HashMap::new();

    let language_keywords: Vec<(&str, GeneratorOutputType)> = [
        if generator_output_types.contains(&GeneratorOutputType::PythonPydantic) {
            match mode {
                ReservedNamesMode::FieldNames => RESERVED_NAMES_PYTHON
                    .iter()
                    .map(|name| (*name, GeneratorOutputType::PythonPydantic))
                    .collect(),
                ReservedNamesMode::FunctionParameters => RESERVED_NAMES_FUNCTION_PARAMETERS
                    .iter()
                    .chain(RESERVED_NAMES_PYTHON.iter())
                    .map(|name| (*name, GeneratorOutputType::PythonPydantic))
                    .collect(),
            }
        } else {
            Vec::new()
        },
        if generator_output_types.contains(&GeneratorOutputType::Typescript) {
            RESERVED_NAMES_TYPESCRIPT
                .iter()
                .map(|name| (*name, GeneratorOutputType::Typescript))
                .collect()
        } else {
            Vec::new()
        },
    ]
    .iter()
    .flatten()
    .cloned()
    .collect();

    language_keywords
        .into_iter()
        .for_each(|(keyword, generator_output_type)| {
            keywords
                .entry(keyword)
                .and_modify(|types| (*types).push(generator_output_type))
                .or_insert(vec![generator_output_type]);
        });

    keywords
}
