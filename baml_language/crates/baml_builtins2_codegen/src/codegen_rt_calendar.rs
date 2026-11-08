use crate::{BamlType as T, BuiltinPipeline, NativeBuiltin};
pub(crate) fn validate(d: &NativeBuiltin) -> Result<(), String> {
    let (params, ret) = match d.path.as_str() {
        "baml.time.Instant.parse" => (vec![T::String], T::Named("Instant".into())),
        "baml.time.Instant._to_string_impl" => (vec![], T::String),
        "baml.time.PlainDate.from_components" => {
            (vec![T::Int, T::Int, T::Int], T::Named("PlainDate".into()))
        }
        "baml.time.PlainDate.parse" => (vec![T::String], T::Named("PlainDate".into())),
        "baml.time.PlainDate._to_string_impl" => (vec![], T::String),
        "baml.time.PlainDate._to_plain_datetime" => {
            (vec![T::Int], T::Named("PlainDateTime".into()))
        }
        "baml.time.PlainDate.year" => (vec![], T::Int),
        "baml.time.PlainDate.month" => (vec![], T::Int),
        "baml.time.PlainDate.day" => (vec![], T::Int),
        "baml.time.PlainDateTime._from_components" => (
            vec![
                T::Int,
                T::Int,
                T::Int,
                T::Int,
                T::Int,
                T::Int,
                T::Int,
                T::Int,
                T::Int,
            ],
            T::Named("PlainDateTime".into()),
        ),
        "baml.time.PlainDateTime.parse" => (vec![T::String], T::Named("PlainDateTime".into())),
        "baml.time.PlainDateTime._to_string_impl" => (vec![], T::String),
        "baml.time.PlainDateTime.to_plain_date" => (vec![], T::Named("PlainDate".into())),
        "baml.time.PlainDateTime.to_plain_time" => (vec![], T::Named("PlainTime".into())),
        "baml.time.PlainDateTime.year" => (vec![], T::Int),
        "baml.time.PlainDateTime.month" => (vec![], T::Int),
        "baml.time.PlainDateTime.day" => (vec![], T::Int),
        "baml.time.PlainDateTime.hour" => (vec![], T::Int),
        "baml.time.PlainDateTime.minute" => (vec![], T::Int),
        "baml.time.PlainDateTime.second" => (vec![], T::Int),
        "baml.time.PlainDateTime.millisecond" => (vec![], T::Int),
        "baml.time.PlainTime._from_components" => (
            vec![T::Int, T::Int, T::Int, T::Int, T::Int, T::Int],
            T::Named("PlainTime".into()),
        ),
        "baml.time.PlainTime.parse" => (vec![T::String], T::Named("PlainTime".into())),
        "baml.time.PlainTime._to_string_impl" => (vec![], T::String),
        "baml.time.TimeZoneOffset.from_duration" => (
            vec![T::Named("Duration".into())],
            T::Named("TimeZoneOffset".into()),
        ),
        "baml.time.ZonedDateTime.parse" => (vec![T::String], T::Named("ZonedDateTime".into())),
        "baml.time._format_zoned" => (
            vec![T::Bigint, T::Int, T::Optional(Box::new(T::String))],
            T::String,
        ),
        _ => return Err(format!("unsupported in baml_rt: {}", d.path)),
    };
    if d.pipeline != BuiltinPipeline::Vm
        || !d.generics.is_empty()
        || d.params.iter().map(|p| &p.ty).ne(params.iter())
        || d.return_type != ret
    {
        return Err(format!(
            "unsupported in baml_rt: changed calendar signature {}",
            d.path
        ));
    }
    Ok(())
}
