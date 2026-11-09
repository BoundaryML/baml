//! `trace.Options` builders and their detached `inspect()` settings.
use super::*;

fn method(name: &str) -> Option<&str> {
    Some(match name {
        "trace._options"
        | "trace.hidden"
        | "trace.timing"
        | "trace._span"
        | "trace.context"
        | "trace.Options.context"
        | "trace.Options.mode"
        | "trace.Options._span"
        | "trace.Options.inspect" => name,
        _ => return None,
    })
}

pub(super) fn native(name: &str) -> bool {
    method(name).is_some()
}

fn class(plan: &Plan<'_>, path: &str) -> Gen<usize> {
    plan.program
        .packages
        .iter()
        .filter(|p| p.name == "trace")
        .flat_map(|p| &p.classes)
        .find_map(|(p, id)| (p.to_string() == path).then_some(id.raw()))
        .ok_or_else(|| no(format!("trace.{path}")))
}

fn mode_enum(plan: &Plan<'_>) -> Gen<usize> {
    plan.program
        .packages
        .iter()
        .filter(|p| p.name == "trace")
        .flat_map(|p| &p.enums)
        .find_map(|(p, id)| (p.to_string() == "Mode").then_some(id.raw()))
        .ok_or_else(|| no("trace.Mode"))
}

pub(super) struct Feature;
impl registry::Feature for Feature {
    fn reach(
        &self,
        db: &dyn Db,
        plan: &mut Plan<'_>,
        term: Option<&Terminator<'_>>,
    ) -> Option<Gen<registry::Reach>> {
        let Some(Terminator::Call {
            callee: Operand::Constant(Constant::Function(function)),
            ..
        }) = term
        else {
            return None;
        };
        let (id, _) = plan.index.function(db, *function)?;
        let name = plan.index.builtin_path(id).to_owned();
        native(&name).then(|| {
            let inspect = name == "trace.Options.inspect";
            plan.add(class(plan, "Options")?)?;
            plan.record(Repr::Enum(mode_enum(plan)?))?;
            if inspect {
                for path in ["Settings", "Capture", "ContextOptions"] {
                    plan.add(class(plan, path)?)?;
                }
            }
            Ok(registry::Reach::default())
        })
    }

    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        _types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        method(name).map(|name| emit(e, name, values))
    }
}

fn emit(e: &Emitter<'_, '_>, name: &str, values: &[(String, Repr)]) -> Gen<(String, Repr)> {
    let options = class(e.plan, "Options")?;
    let layout = &e.plan.layouts[&options];
    if layout.fields != [Repr::Opaque] {
        return Err(no("changed trace.Options layout"));
    }
    let mode_tag = format!("CAPTURE_ENUM_{}_TAG", mode_enum(e.plan)?);
    let wrap = |state: String| {
        e.fallible(&format!(
            "(||->R<usize>{{unsafe{{let s={state}?;let p=rt::try_alloc({}, {})? as *mut C{options};baml_rt_gc::write_ptr(p.cast(),(&raw mut (*p).f0).cast(),s);Ok(p as usize)}}}})()",
            e.descriptors[&options], layout.bytes
        ))
    };
    let tagged = |i: usize| -> Gen<String> {
        values
            .get(i)
            .map(|v| e.coerce(v.clone(), Repr::Tagged))
            .unwrap_or_else(|| Ok("rt::Dyn{tag:u64::MAX,payload:0}".into()))
    };
    let mode = |i: usize| -> Gen<String> {
        Ok(format!(
            "{{let m={};if m.tag=={mode_tag}{{Some(m.payload)}}else{{None}}}}",
            tagged(i)?
        ))
    };
    let state = |i: usize| -> Gen<String> {
        let receiver = e.coerce(values[i].clone(), Repr::Ptr(options))?;
        Ok(format!("(*({receiver} as *const C{options})).f0"))
    };
    // `(metadata, distinct_id)` starting at argument `i`.
    let patch = |i: usize| -> Gen<String> {
        let metadata = values.get(i).ok_or_else(|| no("trace context metadata"))?;
        let entries = if matches!(metadata.1, Repr::Map(_)) {
            let map = e.map_type(metadata.1)?;
            format!(
                "{{let m={map}::from_raw({});(0..m.len()).map(|i|{{let (k,v)=m.entry(i).unwrap();(k.to_owned(),v)}}).collect::<Vec<_>>()}}",
                metadata.0
            )
        } else if metadata.1 == Repr::Tagged {
            "Vec::new()".into()
        } else {
            return Err(no("trace context metadata representation"));
        };
        let distinct = match values.get(i + 1) {
            Some((v, Repr::Str)) => v.clone(),
            Some((_, Repr::Null | Repr::Tagged)) | None => "0usize".into(),
            Some(_) => return Err(no("trace distinct id representation")),
        };
        Ok(format!("rt::trace::patch(&{entries},{distinct})"))
    };
    Ok(match name {
        "trace._options" => (
            wrap(format!("rt::trace::with_mode({})", mode(0)?)),
            Repr::Ptr(options),
        ),
        "trace.hidden" => (
            wrap("rt::trace::with_mode(Some(0))".into()),
            Repr::Ptr(options),
        ),
        "trace.timing" => (
            wrap("rt::trace::with_mode(Some(1))".into()),
            Repr::Ptr(options),
        ),
        "trace._span" => (
            wrap(format!(
                "rt::trace::span(0,{},{},{})",
                tagged(0)?,
                tagged(1)?,
                tagged(2)?
            )),
            Repr::Ptr(options),
        ),
        "trace.Options._span" => (
            wrap(format!(
                "rt::trace::span({},{},{},{})",
                state(0)?,
                tagged(1)?,
                tagged(2)?,
                tagged(3)?
            )),
            Repr::Ptr(options),
        ),
        "trace.context" => (
            wrap(format!("rt::trace::context(0,{})", patch(0)?)),
            Repr::Ptr(options),
        ),
        "trace.Options.context" => (
            wrap(format!("rt::trace::context({},{})", state(0)?, patch(1)?)),
            Repr::Ptr(options),
        ),
        "trace.Options.mode" => (
            wrap(format!("rt::trace::set_mode({},{})", state(0)?, mode(1)?)),
            Repr::Ptr(options),
        ),
        _ => inspect(e, &state(0)?, &mode_tag)?,
    })
}

fn inspect(e: &Emitter<'_, '_>, state: &str, mode_tag: &str) -> Gen<(String, Repr)> {
    let settings = class(e.plan, "Settings")?;
    let capture = class(e.plan, "Capture")?;
    let context = class(e.plan, "ContextOptions")?;
    let field = |c: usize, name: &str| -> Gen<Repr> {
        let i = e
            .plan
            .class(c)
            .fields
            .iter()
            .position(|f| f.name.as_str() == name)
            .ok_or_else(|| no("trace settings field"))?;
        Ok(e.plan.layouts[&c].fields[i])
    };
    for (c, f) in [
        (settings, "mode"),
        (capture, "inputs"),
        (capture, "output"),
        (capture, "error"),
    ] {
        if field(c, f)? != Repr::Tagged {
            return Err(no("trace settings layout"));
        }
    }
    let metadata = field(context, "metadata")?;
    let map = e.map_type(metadata)?;
    let map_desc = e.lists[&metadata];
    let capture_object = reflection::build(
        e,
        capture,
        &[
            ("inputs", "rt::trace::optional_bool(s.inputs)".into()),
            ("output", "rt::trace::optional_bool(s.output)".into()),
            ("error", "rt::trace::optional_bool(s.error)".into()),
        ],
    )?;
    let context_object = reflection::build(
        e,
        context,
        &[
            (
                "distinct_id",
                "match &s.distinct_id{Some(v)=>cx.string(v)?.as_raw(),None=>0usize}".into(),
            ),
            (
                "metadata",
                format!(
                    "{{let m={map}::new(&mut cx,{map_desc})?;for (k,v) in &s.metadata{{let k=cx.string(k)?;let v=rt::trace::metadata_value(v)?;m.set(&mut cx,k,v)?;}}m.as_raw()}}"
                ),
            ),
        ],
    )?;
    let object = reflection::build(
        e,
        settings,
        &[
            (
                "mode",
                format!(
                    "match s.mode{{Some(i)=>rt::Dyn{{tag:{mode_tag},payload:i}},None=>rt::Dyn{{tag:2,payload:0}}}}"
                ),
            ),
            ("capture", capture_object),
            ("context", context_object),
        ],
    )?;
    Ok((
        e.fallible(&format!(
            "(||->R<usize>{{unsafe{{let mut cx=rt::strings::Cx::current();let s=rt::trace::inspect({state});Ok({object})}}}})()"
        )),
        Repr::Ptr(settings),
    ))
}
