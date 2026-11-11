mod codegen_rt;
mod codegen_rt_bytes;
mod codegen_rt_regex;
mod codegen_rt_strings;
pub use codegen_rt::{generate_rt, supports_rt};
mod codegen;
mod codegen_errors;
mod codegen_io;
mod codegen_panics;
mod extract;
mod lang_item;
mod rust_ident;
mod types;

pub use codegen::{generate_native_trait, generate_native_trait_for};
pub use codegen_errors::generate_error_enums;
pub use codegen_io::{
    generate_io_adapter, generate_io_structs, generate_io_traits, generate_rt_provider,
    generate_runtime_io, generate_sys_op_enum,
};
pub use codegen_panics::generate_panic_enums;
pub use extract::{
    ExtractNativeBuiltinsError, extract_native_builtins, extract_native_builtins_for,
};
pub use types::{
    BamlType, BuiltinPipeline, NativeBuiltin, NativeClassDef, NativeClassField, Param, Receiver,
    ReceiverType,
};

/// Format a `TokenStream` into a pretty-printed Rust source string.
fn format_tokens(tokens: &proc_macro2::TokenStream) -> String {
    let code_string = tokens.to_string();
    let syntax_tree = syn::parse_file(&code_string).expect("Failed to parse generated code");
    prettyplease::unparse(&syntax_tree)
}

mod triage;
pub use triage::{
    BuiltinGroup, RtDisposition, RtStrategy, classify_builtin, implementation_strategy,
    native_inventory, supports_rt_reflection,
};

/// Calendar natives implemented through shared heap-neutral algorithms.
pub fn supports_rt_calendar(path: &str) -> bool {
    matches!(
        path,
        "baml.time.Instant.parse"
            | "baml.time.Instant._to_string_impl"
            | "baml.time.PlainDate.from_components"
            | "baml.time.PlainDate.parse"
            | "baml.time.PlainDate._to_string_impl"
            | "baml.time.PlainDate._to_plain_datetime"
            | "baml.time.PlainDate.year"
            | "baml.time.PlainDate.month"
            | "baml.time.PlainDate.day"
            | "baml.time.PlainDateTime._from_components"
            | "baml.time.PlainDateTime.parse"
            | "baml.time.PlainDateTime._to_string_impl"
            | "baml.time.PlainDateTime.to_plain_date"
            | "baml.time.PlainDateTime.to_plain_time"
            | "baml.time.PlainDateTime.year"
            | "baml.time.PlainDateTime.month"
            | "baml.time.PlainDateTime.day"
            | "baml.time.PlainDateTime.hour"
            | "baml.time.PlainDateTime.minute"
            | "baml.time.PlainDateTime.second"
            | "baml.time.PlainDateTime.millisecond"
            | "baml.time.PlainTime._from_components"
            | "baml.time.PlainTime.parse"
            | "baml.time.PlainTime._to_string_impl"
            | "baml.time.TimeZoneOffset.from_duration"
            | "baml.time.ZonedDateTime.parse"
            | "baml.time._format_zoned"
    )
}

mod codegen_rt_calendar;

mod codegen_rt_bigint;
mod codegen_rt_clock;
mod codegen_rt_io;
mod codegen_rt_provider;
mod codegen_rt_random;
mod codegen_rt_scalar;

pub use codegen_rt_provider::supports as supports_rt_provider;
mod codegen_rt_string_bytes;

pub use codegen_rt_io::supports as supports_rt_io;
