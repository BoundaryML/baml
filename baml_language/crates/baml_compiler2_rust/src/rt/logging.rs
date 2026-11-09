//! Log locations come from the statement's immutable compact-PC table.
use super::*;
pub(super) struct Feature;
impl registry::Feature for Feature {
    fn statement(&self, e: &mut Emitter<'_, '_>, s: &StatementKind<'_>) -> Option<Gen<String>> {
        let StatementKind::Intrinsic {
            op: baml_compiler2_mir::IntrinsicOp::Log(level),
            args,
        } = s
        else {
            return None;
        };
        Some((|| {
            let data = args.first().ok_or_else(|| no("log data"))?;
            let data = e.coerce(e.operand(data)?, Repr::Tagged)?;
            let name = args
                .get(1)
                .filter(|v| !matches!(v, Operand::Constant(Constant::OmittedArg)))
                .map(|v| e.operand(v))
                .transpose()?;
            let name = match name {
                None | Some((_, Repr::Null)) => "None".into(),
                Some((v, Repr::Str)) => format!(
                    "{{let p={v};if p==0{{None}}else{{Some(rt::strings::Str::from_raw(p))}}}}"
                ),
                Some((v, Repr::Tagged)) => format!(
                    "{{let v={v};if v.tag==2 || v.tag==u64::MAX{{None}}else{{Some(rt::strings::Str::from_raw({}))}}}}",
                    e.fallible("rt::untag(v,3,false)")
                ),
                _ => return Err(no("log event name")),
            };
            Ok(format!(
                "rt::telemetry::log(&O{},{}u32,rt::telemetry::LogLevel::{level:?},{name},{data});\n",
                e.object, e.compact_sites[e.site]
            ))
        })())
    }
}
