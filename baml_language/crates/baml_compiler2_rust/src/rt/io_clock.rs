//! Owned clock and system requests, hydrated with the pack's concrete layouts.
use super::*;
pub(super) struct Feature;
impl registry::Feature for Feature {
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
        if !matches!(
            name,
            "baml.time.Instant.now"
                | "baml.time.system_timezone"
                | "baml.time._tz_offset_at"
                | "baml.time._tz_to_instant"
                | "baml.sys._sleep"
                | "baml.sys.host_info"
                | "baml.sys.collect_garbage"
        ) {
            return None;
        }
        Some((|| {
            let arg = |i: usize, t: Repr| -> Gen<String> {
                let value = e.coerce(
                    e.operand(args.get(i).ok_or_else(|| no("clock argument"))?)?,
                    t,
                )?;
                Ok(match t {
                    Repr::Str => format!("rt::strings::Str::from_raw({value}).to_owned()"),
                    Repr::Bigint => format!("rt::native::bigint_owned({value})"),
                    _ => value,
                })
            };
            if name == "baml.sys.collect_garbage" {
                return e.assign(
                    destination,
                    (
                        format!("{{{};0usize}}", e.fallible("rt::tasks::collect()")),
                        Repr::Null,
                    ),
                );
            }
            let request = match name {
                "baml.sys._sleep" => format!("rt::io::clock::sleep({})", arg(0, Repr::Bigint)?),
                "baml.time.Instant.now" => "rt::io::clock::now()".into(),
                "baml.sys.host_info" => "rt::io::clock::host_info()".into(),
                "baml.time.system_timezone" => "rt::io::clock::system_timezone()".into(),
                "baml.time._tz_offset_at" => format!(
                    "rt::io::clock::offset_at({},{})",
                    arg(0, Repr::Str)?,
                    arg(1, Repr::Bigint)?
                ),
                _ => format!(
                    "rt::io::clock::to_instant({},{},{})",
                    arg(0, Repr::Str)?,
                    arg(1, Repr::Bigint)?,
                    arg(2, Repr::Str)?
                ),
            };
            let owned = e.fallible(&format!("rt::io::perform({request})"));
            let (value,repr)=match name {
                "baml.sys._sleep"=>(format!("{{let _={owned};0usize}}"),Repr::Null),
                "baml.time.system_timezone"=>(format!("{{let owned={owned};{}}}",e.fallible("rt::strings::Cx::current().string(owned.as_str().expect(\"typed clock string\")).map(|s|s.as_raw())")),Repr::Str),
                "baml.time._tz_offset_at"=>(format!("{{let owned={owned};rt::native::optional_int(owned.as_i64())}}"),Repr::Tagged),
                "baml.time._tz_to_instant"=>(format!("{{let owned={owned};if let Some(s)=owned.as_str(){{{}}}else{{0usize}}}}",e.fallible("rt::native::bigint_literal(s)")),Repr::Bigint),
                _=>{
                    let (_,Repr::Ptr(c))=e.place(destination)? else{return Err(no("clock owned class destination"));};
                    let layout=&e.plan.layouts[&c];
                    let class=e.plan.class(c);
                    let expected: &[(&str, Repr)] = if name=="baml.time.Instant.now" {
                        &[("_nanoseconds",Repr::Bigint)]
                    } else {
                        &[("platform",Repr::Str),("architecture",Repr::Str),("cpu_count",Repr::Int),("stderr_is_terminal",Repr::Bool)]
                    };
                    if class.fields.len()!=expected.len() || class.fields.iter().zip(&layout.fields).zip(expected).any(|((field,repr),(name,expected))|field.name.as_str()!=*name || repr!=expected) {
                        return Err(no("changed clock class schema"));
                    }
                    let mut out=format!("{{let owned={owned};");
                    for (i,field) in class.fields.iter().enumerate(){
                        let key=field.name.as_str();
                        let input=if name=="baml.time.Instant.now"{"owned".to_owned()}else{format!("owned[{key:?}]")};
                        let value=match layout.fields[i]{
                            Repr::Str=>e.fallible(&format!("rt::strings::Cx::current().string({input}.as_str().expect(\"owned string field\")).map(|s|s.as_raw())")),
                            Repr::Bigint=>e.fallible(&format!("rt::native::bigint_literal({input}.as_str().expect(\"owned bigint field\"))")),
                            Repr::Int=>format!("{input}.as_i64().expect(\"owned int field\")"),
                            Repr::Bool=>format!("{input}.as_bool().expect(\"owned bool field\")"),
                            _=>return Err(no("clock owned field layout")),
                        };
                        write!(out,"let a{i}={value};").unwrap();
                    }
                    let allocation=e.fallible(&format!("rt::try_alloc({}, {})",e.descriptors[&c],layout.bytes));
                    write!(out,"let p={allocation} as *mut C{c};").unwrap();
                    for (i,repr) in layout.fields.iter().enumerate(){
                        if repr.pointer(){write!(out,"baml_rt_gc::write_ptr(p.cast(),(&raw mut (*p).f{i}).cast(),a{i});").unwrap();}
                        else {write!(out,"(*p).f{i}=a{i} as _;").unwrap();}
                    }
                    out.push_str("p as usize}");(out,Repr::Ptr(c))
                }
            };
            e.assign(destination, (value, repr))
        })())
    }
}
