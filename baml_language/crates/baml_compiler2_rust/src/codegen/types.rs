//! Rust expressions for realized types.

use crate::index::ProgramIndex;
use baml_compiler2_mir::TyTemplate;
use baml_type::Literal;

/// Frame references require a running function; unsupported templates return `None`.
pub(crate) fn realized_expr(
    index: &ProgramIndex,
    template: &TyTemplate,
    frame: bool,
) -> Option<String> {
    build_type(index, template, frame).ok()
}

pub(crate) fn template_blocker(index: &ProgramIndex, template: &TyTemplate) -> &'static str {
    build_type(index, template, true)
        .err()
        .unwrap_or("a type this backend does not build")
}

fn build_type(
    index: &ProgramIndex,
    template: &TyTemplate,
    frame: bool,
) -> Result<String, &'static str> {
    let part = |part: &TyTemplate| build_type(index, part, frame);
    let parts = |parts: &[TyTemplate]| {
        parts
            .iter()
            .map(part)
            .collect::<Result<Vec<_>, _>>()
            .map(|parts| parts.join(", "))
    };
    Ok(match template {
        TyTemplate::Int
        | TyTemplate::Float
        | TyTemplate::Bool
        | TyTemplate::String
        | TyTemplate::Null
        | TyTemplate::Never => format!("RealizedTy::{template:?}"),
        TyTemplate::Unknown => "RealizedTy::unknown()".to_string(),
        TyTemplate::TypeArgRef(slot) => {
            if !frame {
                return Err("a type this backend does not build");
            }
            format!("vm.frame_type_arg(fr, {slot})?")
        }
        TyTemplate::List(element) => format!("RealizedTy::List(Box::new({}))", part(element)?),
        TyTemplate::Map { key, value } => format!(
            "RealizedTy::Map {{ key: Box::new({}), value: Box::new({}) }}",
            part(key)?,
            part(value)?
        ),
        TyTemplate::Union(members) => format!("RealizedTy::Union(Box::new([{}]))", parts(members)?),
        TyTemplate::Class(name, args) => {
            let (object, _) = index.class_at(name).ok_or("a class not in the program")?;
            if args.is_empty() {
                format!("vm.class_type(vm.record_object(fr, ObjectIndex::from_raw({object})))")
            } else {
                format!(
                    "vm.class_type_at(vm.record_object(fr, ObjectIndex::from_raw({object})), Box::new([{}]))",
                    parts(args)?
                )
            }
        }
        TyTemplate::Enum(name) => {
            let object = index.enum_at(name).ok_or("an enum not in the program")?;
            format!("vm.enum_type(vm.record_object(fr, ObjectIndex::from_raw({object})))")
        }
        TyTemplate::Interface(name, args, assoc) => {
            let object = index
                .interface_at(name)
                .ok_or("an interface not in the program")?;
            let args = parts(args)?;
            let assoc = assoc
                .iter()
                .map(|(member, ty)| part(ty).map(|ty| format!("({:?}, {ty})", member.as_str())))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ");
            format!(
                "vm.interface_type_at(vm.record_object(fr, ObjectIndex::from_raw({object})), Box::new([{args}]), &[{assoc}])"
            )
        }
        // The runtime boundary normalizes a literal's freshness to `Regular`.
        TyTemplate::Literal(literal, _) => {
            let literal = match literal {
                Literal::Int(n) => format!("Literal::Int({n}i64)"),
                Literal::Bool(b) => format!("Literal::Bool({b})"),
                Literal::String(s) => format!("Literal::String(String::from({s:?}))"),
                Literal::Float(f) => format!("Literal::Float(String::from({f:?}))"),
                Literal::Bigint(_) => return Err("a bigint literal type"),
            };
            format!(
                "RealizedTy::Literal(bex_vm::compiled_rt::{literal}, bex_vm::compiled_rt::Freshness::Regular)"
            )
        }
        unsupported => {
            return Err(match unsupported {
                TyTemplate::Function { .. } => "a function type",
                TyTemplate::Future(..) => "a future type",
                TyTemplate::AssociatedTypeProjection { .. } => "a projection",
                TyTemplate::TypeAlias(_) => "a recursive alias",
                TyTemplate::EnumVariant(..) => "an enum variant type",
                _ => "an opaque type",
            });
        }
    })
}
