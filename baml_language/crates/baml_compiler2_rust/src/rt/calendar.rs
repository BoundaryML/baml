//! Owned calendar adapters over shared algorithms and immutable class schemas.
use super::*;
use baml_builtins2_codegen::{BamlType as Native, ReceiverType};
pub(super) struct Feature;
impl registry::Feature for Feature {
    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if !baml_builtins2_codegen::supports_rt_calendar(name) {
            return None;
        }
        Some(if types.is_empty() {
            invoke(e, name, values)
        } else {
            Err(no("calendar type arguments"))
        })
    }
}
fn invoke(e: &Emitter<'_, '_>, name: &str, values: &[(String, Repr)]) -> Gen<(String, Repr)> {
    let (declarations, _, _) = baml_builtins2_codegen::extract_native_builtins().map_err(no)?;
    let d = declarations
        .iter()
        .find(|d| d.path == name)
        .ok_or_else(|| no(name))?;
    let mut params = Vec::new();
    if let Some(receiver) = &d.receiver
        && receiver.receiver_type != ReceiverType::Static
    {
        params.push(Native::Named(receiver.class_name.clone()));
    }
    params.extend(d.params.iter().map(|p| p.ty.clone()));
    if values.len() != params.len() {
        return Err(no("calendar arity"));
    }
    let mut schemas = provider_schema::Schemas::new(e);
    let mut args = Vec::new();
    let mut conversions = String::new();
    for (i, (value, ty)) in values.iter().zip(params).enumerate() {
        let schema = schemas.declared(&ty, Some(value.1))?;
        let value = e.capture_value(value.0.clone(), value.1)?;
        write!(
            conversions,
            "let a{i}={};",
            e.fallible(&format!("{schema}.copy_out({value},0)"))
        )
        .unwrap();
        args.push(format!("a{i}"));
    }
    let output = schemas.declared(&d.return_type, None)?;
    Ok((
        format!(
            "{{{}{conversions}let owned={};{}}}",
            schemas.finish(),
            e.fallible(&format!(
                "rt::calendar::call({name:?},&[{}])",
                args.join(",")
            )),
            e.fallible(&format!("{output}.hydrate(&owned,0)"))
        ),
        Repr::Tagged,
    ))
}
