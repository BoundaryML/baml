//! Owned schemas around direct calls to the shared native I/O provider.
use super::*;
use baml_builtins2_codegen::{BamlType as Native, ReceiverType};
pub(super) struct Feature;
const ERRORS: &[&str] = &[
    "InvalidArgument",
    "ParseError",
    "Io",
    "Timeout",
    "Unsupported",
    "AccessError",
    "RenderPrompt",
    "LlmClient",
];
fn error_classes(plan: &Plan<'_>) -> Gen<Vec<(&'static str, usize)>> {
    let package = plan
        .program
        .packages
        .iter()
        .find(|p| p.name == "baml")
        .ok_or_else(|| no("native error package"))?;
    ERRORS
        .iter()
        .map(|&name| {
            let path = format!("errors.{name}");
            let id = package
                .classes
                .iter()
                .find_map(|(p, i)| (p.to_string() == path).then_some(i.raw()))
                .ok_or_else(|| no(format!("native error {name}")))?;
            Ok((name, id))
        })
        .collect()
}
pub(super) fn prepare_types(plan: &mut Plan<'_>, name: &str) -> Gen<()> {
    if baml_builtins2_codegen::supports_rt_provider(name) {
        for (_, c) in error_classes(plan)? {
            plan.add(c)?;
        }
    }
    Ok(())
}
impl registry::Feature for Feature {
    fn reach(
        &self,
        db: &dyn Db,
        plan: &mut Plan<'_>,
        term: Option<&Terminator<'_>>,
    ) -> Option<Gen<registry::Reach>> {
        let Some(Terminator::SysOp {
            callee: Operand::Constant(Constant::Function(f)),
            ..
        }) = term
        else {
            return None;
        };
        let (id, _) = plan.index.function(db, *f)?;
        let name = plan.index.builtin_path(id).to_owned();
        if !baml_builtins2_codegen::supports_rt_provider(&name) {
            return None;
        }
        Some((|| {
            prepare_types(plan, &name)?;
            Ok(registry::Reach {
                builtin: Some(name),
                ..Default::default()
            })
        })())
    }
    fn terminator(&self, e: &mut Emitter<'_, '_>, term: &Terminator<'_>) -> Option<Gen<String>> {
        let Terminator::SysOp {
            callee: Operand::Constant(Constant::Function(f)),
            args,
            destination,
            ..
        } = term
        else {
            return None;
        };
        let (id, _) = e.plan.index.function(e.db, *f)?;
        let name = e.plan.index.builtin_path(id);
        if !baml_builtins2_codegen::supports_rt_provider(name) {
            return None;
        }
        Some((|| {
            let values = args.iter().map(|a| e.operand(a)).collect::<Gen<Vec<_>>>()?;
            let hint = e.place(destination)?.1;
            let value = invoke(e, name, &values, Some(hint))?;
            e.assign(destination, value)
        })())
    }
    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if !baml_builtins2_codegen::supports_rt_provider(name) {
            return None;
        }
        Some(if types.is_empty() {
            invoke(e, name, values, None)
        } else {
            Err(no("native IO type arguments"))
        })
    }
}
fn invoke(
    e: &Emitter<'_, '_>,
    name: &str,
    values: &[(String, Repr)],
    hint: Option<Repr>,
) -> Gen<(String, Repr)> {
    let (_, declarations, _) = baml_builtins2_codegen::extract_native_builtins().map_err(no)?;
    let d = declarations
        .iter()
        .find(|d| d.path == name)
        .ok_or_else(|| no(format!("native IO declaration {name}")))?;
    let mut params = Vec::new();
    if let Some(receiver) = &d.receiver
        && receiver.receiver_type != ReceiverType::Static
    {
        params.push(Native::Named(receiver.class_name.clone()));
    }
    params.extend(d.params.iter().map(|p| p.ty.clone()));
    if values.len() != params.len() {
        return Err(no(format!("native IO arity {name}")));
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
    let output = schemas.declared(&d.return_type, hint)?;
    let mut errors = Vec::new();
    for (name, c) in error_classes(e.plan)? {
        let schema = schemas.object(c)?;
        errors.push(format!("({:?},&{schema})", format!("baml.errors.{name}")));
    }
    let error_callback = if matches!(e.all[&e.object].kind, MirFunctionKind::Builtin(_)) {
        "let IO_ERROR=|error:rt::provider::OpError,asynchronous:bool|{let mut error=rt::provider::error(error,IO_ERRORS);if asynchronous{rt::telemetry::evidence::raised_at_call_site(&mut error,_site,rt::telemetry::RaiseKind::HostBoundary);}error};".to_owned()
    } else {
        format!("fn IO_ERROR(error:rt::provider::OpError,asynchronous:bool)->Box<rt::Error>{{let mut error=rt::provider::error(error,IO_ERRORS);if asynchronous{{rt::telemetry::evidence::raised_with_kind(&mut error,&S{},{},None,Some(rt::telemetry::RaiseKind::HostBoundary));}}error}}",e.object,e.site)
    };
    let mut out = format!(
        "{{{} static IO_ERRORS:&[(&str,&rt::external::Type)]=&[{}];{error_callback}{conversions}",
        schemas.finish(),
        errors.join(","),
    );
    args.push("IO_ERROR".into());
    write!(
        out,
        "let owned={};{} }}",
        e.fallible(&format!(
            "rt_provider_{}({})",
            d.sys_op_variant_name(),
            args.join(",")
        )),
        e.fallible(&format!("{output}.hydrate(&owned,0)"))
    )
    .unwrap();
    Ok((out, Repr::Tagged))
}
