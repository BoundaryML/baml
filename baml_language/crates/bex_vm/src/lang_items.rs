//! The standard-library declarations the runtime's own code names, in one
//! place and by identity ([`LangItem`]). Load resolves every one whose
//! package the executable holds ([`runtime_items`], `require_lang_items`), so
//! a declaration the stdlib renamed or moved refuses the executable instead
//! of panicking at the first native call that asks for it; `tests/lang_items.rs`
//! runs that resolution against the toolchain's own standard library.

use baml_base::{LangItem, MediaKind, lang_item};
use baml_type::{compiler_aliases, normalize::WellKnownHead, type_kind::TypeKind};
use bex_vm_types::{PanicClass, types::ErrorClass};

pub const AI_CACHE_DELIMITER: LangItem = lang_item!(Ai: CacheDelimiter);
pub const AI_FUNCTION_SPEC: LangItem = lang_item!(Ai: FunctionSpec);
pub const AI_PROMPT: LangItem = lang_item!(Ai: Prompt);
pub const AI_PROMPT_MESSAGE: LangItem = lang_item!(Ai: PromptMessage);
pub const AI_ROLE: LangItem = lang_item!(Ai: Role);
pub const AI_STREAM_STREAM: LangItem = lang_item!(Ai: stream.Stream);
pub const BAML_CRYPTO_DECRYPTION_FAILURE: LangItem = lang_item!(Baml: crypto.DecryptionFailure);
pub const BAML_CSV_ERROR_KIND: LangItem = lang_item!(Baml: csv.ErrorKind);
pub const BAML_CSV_NEED_DATA: LangItem = lang_item!(Baml: csv._NeedData);
pub const BAML_FROM_JSON: LangItem = lang_item!(Baml: FromJson);
pub const BAML_FUTURE_STATE: LangItem = lang_item!(Baml: future.State);
pub const BAML_HASH: LangItem = lang_item!(Baml: Hash);
pub const BAML_HASH_HASHER: LangItem = lang_item!(Baml: hash.Hasher);
pub const BAML_ITER_DONE: LangItem = lang_item!(Baml: iter.Done);
pub const BAML_JSON_DECODE_ERROR: LangItem = lang_item!(Baml: json.DecodeError);
pub const BAML_JSON_JSON: LangItem = lang_item!(Baml: json.json);
pub const BAML_JSON_PARSE_ERROR: LangItem = lang_item!(Baml: json.ParseError);
pub const BAML_JSON_PATH_ERROR: LangItem = lang_item!(Baml: json.PathError);
pub const BAML_JSON_SERIALIZATION_ERROR: LangItem = lang_item!(Baml: json.SerializationError);
pub const BAML_OPS_ADD: LangItem = lang_item!(Baml: ops.Add);
pub const BAML_OPS_DIVIDE: LangItem = lang_item!(Baml: ops.Divide);
pub const BAML_OPS_EQUALS: LangItem = lang_item!(Baml: ops.Equals);
pub const BAML_OPS_MULTIPLY: LangItem = lang_item!(Baml: ops.Multiply);
pub const BAML_OPS_NEGATE: LangItem = lang_item!(Baml: ops.Negate);
pub const BAML_OPS_REMAINDER: LangItem = lang_item!(Baml: ops.Remainder);
pub const BAML_OPS_SUBTRACT: LangItem = lang_item!(Baml: ops.Subtract);
pub const BAML_REGEX_ERROR_KIND: LangItem = lang_item!(Baml: regex.ErrorKind);
pub const BAML_SAP_NO_YIELD: LangItem = lang_item!(Baml: sap._NoYield);
pub const BAML_SAP_PARSE_CACHE: LangItem = lang_item!(Baml: sap._ParseCache);
pub const BAML_SPAWN_CANCEL_TOKEN: LangItem = lang_item!(Baml: spawn.CancelToken);
pub const BAML_SPAWN_LIMIT: LangItem = lang_item!(Baml: spawn.Limit);
pub const BAML_TIME_DURATION: LangItem = lang_item!(Baml: time.Duration);
pub const BAML_TIME_INSTANT: LangItem = lang_item!(Baml: time.Instant);
pub const BAML_TIME_PLAIN_DATE: LangItem = lang_item!(Baml: time.PlainDate);
pub const BAML_TIME_PLAIN_DATE_TIME: LangItem = lang_item!(Baml: time.PlainDateTime);
pub const BAML_TOML_PARSE_ERROR: LangItem = lang_item!(Baml: toml.ParseError);
pub const BAML_TOML_TABLE: LangItem = lang_item!(Baml: toml.Table);
pub const BAML_TO_JSON: LangItem = lang_item!(Baml: ToJson);
pub const BAML_TO_STRING: LangItem = lang_item!(Baml: ToString);
pub const BAML_YAML_PARSE_ERROR: LangItem = lang_item!(Baml: yaml.ParseError);
pub const REFLECT_ARG: LangItem = lang_item!(Reflect: Arg);
pub const REFLECT_CLASS_BUILDER: LangItem = lang_item!(Reflect: class.Builder);
pub const REFLECT_CLASS_PENDING_TYPE: LangItem = lang_item!(Reflect: class.PendingType);
pub const REFLECT_DIAGNOSTIC: LangItem = lang_item!(Reflect: Diagnostic);
pub const REFLECT_DIAGNOSTIC_ANNOTATION: LangItem = lang_item!(Reflect: DiagnosticAnnotation);
pub const REFLECT_DIAGNOSTIC_HIGHLIGHT: LangItem = lang_item!(Reflect: DiagnosticHighlight);
pub const REFLECT_DIAGNOSTIC_RELATED_INFO: LangItem = lang_item!(Reflect: DiagnosticRelatedInfo);
pub const REFLECT_ENUM_VALUE: LangItem = lang_item!(Reflect: enum.Value);
pub const REFLECT_ERRORS_COMPILATION_ERROR: LangItem = lang_item!(Reflect: errors.CompilationError);
pub const REFLECT_INTERFACE_IMPLEMENTATION: LangItem =
    lang_item!(Reflect: interface.Implementation);
pub const REFLECT_META: LangItem = lang_item!(Reflect: Meta);
pub const REFLECT_WITH_META: LangItem = lang_item!(Reflect: WithMeta);
pub const TESTING_TEST_COLLECTOR: LangItem = lang_item!(Testing: TestCollector);
pub const TRACE_MODE: LangItem = lang_item!(Trace: Mode);

/// Every item this module names.
pub const ALL: &[LangItem] = &[
    AI_CACHE_DELIMITER,
    AI_FUNCTION_SPEC,
    AI_PROMPT,
    AI_PROMPT_MESSAGE,
    AI_ROLE,
    AI_STREAM_STREAM,
    BAML_CRYPTO_DECRYPTION_FAILURE,
    BAML_CSV_ERROR_KIND,
    BAML_CSV_NEED_DATA,
    BAML_FROM_JSON,
    BAML_FUTURE_STATE,
    BAML_HASH,
    BAML_HASH_HASHER,
    BAML_ITER_DONE,
    BAML_JSON_DECODE_ERROR,
    BAML_JSON_JSON,
    BAML_JSON_PARSE_ERROR,
    BAML_JSON_PATH_ERROR,
    BAML_JSON_SERIALIZATION_ERROR,
    BAML_OPS_ADD,
    BAML_OPS_DIVIDE,
    BAML_OPS_EQUALS,
    BAML_OPS_MULTIPLY,
    BAML_OPS_NEGATE,
    BAML_OPS_REMAINDER,
    BAML_OPS_SUBTRACT,
    BAML_REGEX_ERROR_KIND,
    BAML_SAP_NO_YIELD,
    BAML_SAP_PARSE_CACHE,
    BAML_SPAWN_CANCEL_TOKEN,
    BAML_SPAWN_LIMIT,
    BAML_TIME_DURATION,
    BAML_TIME_INSTANT,
    BAML_TIME_PLAIN_DATE,
    BAML_TIME_PLAIN_DATE_TIME,
    BAML_TOML_PARSE_ERROR,
    BAML_TOML_TABLE,
    BAML_TO_JSON,
    BAML_TO_STRING,
    BAML_YAML_PARSE_ERROR,
    REFLECT_ARG,
    REFLECT_CLASS_BUILDER,
    REFLECT_CLASS_PENDING_TYPE,
    REFLECT_DIAGNOSTIC,
    REFLECT_DIAGNOSTIC_ANNOTATION,
    REFLECT_DIAGNOSTIC_HIGHLIGHT,
    REFLECT_DIAGNOSTIC_RELATED_INFO,
    REFLECT_ENUM_VALUE,
    REFLECT_ERRORS_COMPILATION_ERROR,
    REFLECT_INTERFACE_IMPLEMENTATION,
    REFLECT_META,
    REFLECT_WITH_META,
    TESTING_TEST_COLLECTOR,
    TRACE_MODE,
];

/// Every declaration the runtime reaches by identity: [`ALL`] and the tables
/// other crates keep for their own purposes — the reflection view of each
/// [`TypeKind`], the media wrapper classes, the declarations behind the
/// compiler's aliases, the algebra's well-known heads, and the generated error
/// and panic classes. The one list load validates and the resolving test
/// walks.
pub fn runtime_items() -> impl Iterator<Item = LangItem> {
    ALL.iter()
        .copied()
        .chain(TypeKind::ALL.into_iter().map(TypeKind::class))
        .chain(
            MediaKind::ALL
                .into_iter()
                .filter_map(MediaKind::wrapper_class),
        )
        .chain(
            compiler_aliases::ALL
                .iter()
                .filter_map(|alias| alias.definition.map(compiler_aliases::Definition::item)),
        )
        .chain(WellKnownHead::ALL.into_iter().map(WellKnownHead::item))
        .chain(ErrorClass::ALL.iter().map(ErrorClass::item))
        .chain(PanicClass::ALL.iter().map(PanicClass::item))
}
