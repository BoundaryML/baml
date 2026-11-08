//! Scalar-field validation errors used by the JSON list workloads.
use super::*;
pub(super) struct Feature;
impl registry::Feature for Feature {
    fn terminator(&self, e: &mut Emitter<'_, '_>, term: &Terminator<'_>) -> Option<Gen<String>> {
        let Terminator::Throw { value } = term else {
            return None;
        };
        let Ok((value, Repr::Ptr(c))) = e.operand(value) else {
            return None;
        };
        let fields = &e.plan.layouts[&c].fields;
        if fields.iter().all(|r| *r == Repr::Int)
            || fields
                .iter()
                .any(|r| !matches!(r, Repr::Int | Repr::Float | Repr::Bool | Repr::Str))
        {
            return None;
        }
        Some((|| {
            let class = e.plan.class(c);
            let package = e
                .plan
                .program
                .packages
                .iter()
                .find(|p| p.objects.contains(&(e.plan.class_origin(c) as u32)))
                .ok_or_else(|| no("validation error package"))?;
            let name = format!("{}.{}", package.name, class.path);
            let display = format!(
                "{name} {{{{{}}}}}",
                class
                    .fields
                    .iter()
                    .map(|f| format!("{}: {{}}", f.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let values=fields.iter().enumerate().map(|(i,r)|{
                let value=format!("(*(p as *const C{c})).f{i}");
                match r {
                    Repr::Str=>format!("if {value}==0{{String::from(\"null\")}}else{{format!(\"{{:?}}\",rt::strings::Str::from_raw({value}).as_str(&rt::strings::Cx::current()))}}"),
                    Repr::Bool=>format!("({value}!=0)"),
                    Repr::Float=>format!("{{let v={value};let mut text=v.to_string();if v.is_finite()&&!text.contains('.'){{text.push_str(\".0\");}}text}}"),
                    _=>value,
                }
            }).collect::<Vec<_>>().join(",");
            let error = format!(
                "Box::new(rt::Error{{telemetry_at:None,telemetry_raise:None,value:p,class:{name:?},message:String::new(),frames:Vec::new(),display_value:Some(format!({display:?},{values}))}}).at(&S{},{})",
                e.object, e.site
            );
            Ok(format!("{{let p={value};{}}}", e.raise(&error)))
        })())
    }
}
