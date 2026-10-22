//! Rust expressions for realized types.

use crate::index::ProgramIndex;
use baml_type::{Literal, TyTemplate};

/// Frame references require a running function; unsupported templates return `None`.
pub(crate) fn realized_expr(
    index: &ProgramIndex,
    template: &baml_compiler2_mir::TyTemplate,
    frame: bool,
) -> Option<String> {
    let part = |part: &baml_compiler2_mir::TyTemplate| realized_expr(index, part, frame);
    let parts = |parts: &[baml_compiler2_mir::TyTemplate]| {
        parts
            .iter()
            .map(part)
            .collect::<Option<Vec<_>>>()
            .map(|parts| parts.join(", "))
    };
    Some(match template {
        TyTemplate::Int => "RealizedTy::Int".to_string(),
        TyTemplate::Float => "RealizedTy::Float".to_string(),
        TyTemplate::Bool => "RealizedTy::Bool".to_string(),
        TyTemplate::String => "RealizedTy::String".to_string(),
        TyTemplate::Null => "RealizedTy::Null".to_string(),
        TyTemplate::Unknown => "RealizedTy::unknown()".to_string(),
        TyTemplate::Never => "RealizedTy::Never".to_string(),
        TyTemplate::TypeArgRef(slot) => {
            if !frame {
                return None;
            }
            format!("vm.frame_type_arg(depth, {slot})?")
        }
        TyTemplate::List(element) => format!("RealizedTy::List(Box::new({}))", part(element)?),
        TyTemplate::Map { key, value } => format!(
            "RealizedTy::Map {{ key: Box::new({}), value: Box::new({}) }}",
            part(key)?,
            part(value)?
        ),
        TyTemplate::Union(members) => format!("RealizedTy::Union(Box::new([{}]))", parts(members)?),
        TyTemplate::Class(name, args) => {
            let (object, _) = index.class_at(name)?;
            if args.is_empty() {
                format!("vm.class_type(vm.idx_to_ptr(ObjectIndex::from_raw({object})))")
            } else {
                format!(
                    "vm.class_type_at(vm.idx_to_ptr(ObjectIndex::from_raw({object})), Box::new([{}]))",
                    parts(args)?
                )
            }
        }
        TyTemplate::Enum(name) => {
            let object = index.enum_at(name)?;
            format!("vm.enum_type(vm.idx_to_ptr(ObjectIndex::from_raw({object})))")
        }
        TyTemplate::Interface(name, args, assoc) => {
            let object = index.interface_at(name)?;
            let assoc = assoc
                .iter()
                .map(|(member, ty)| Some(format!("({:?}, {})", member.as_str(), part(ty)?)))
                .collect::<Option<Vec<_>>>()?
                .join(", ");
            format!(
                "vm.interface_type_at(vm.idx_to_ptr(ObjectIndex::from_raw({object})), Box::new([{}]), &[{assoc}])",
                parts(args)?
            )
        }
        // The runtime boundary normalizes a literal's freshness to `Regular`.
        TyTemplate::Literal(literal, _) => {
            let literal = match literal {
                Literal::Int(n) => format!("Literal::Int({n}i64)"),
                Literal::Bool(b) => format!("Literal::Bool({b})"),
                Literal::String(s) => format!("Literal::String(String::from({s:?}))"),
                Literal::Float(f) => format!("Literal::Float(String::from({f:?}))"),
                Literal::Bigint(_) => return None,
            };
            format!(
                "RealizedTy::Literal(bex_vm::compiled_rt::{literal}, bex_vm::compiled_rt::Freshness::Regular)"
            )
        }
        _ => return None,
    })
}

pub(crate) fn template_blocker(
    index: &ProgramIndex,
    template: &baml_compiler2_mir::TyTemplate,
) -> &'static str {
    fn blocker_of(
        index: &ProgramIndex,
        template: &baml_compiler2_mir::TyTemplate,
    ) -> Option<&'static str> {
        let parts = |parts: &[baml_compiler2_mir::TyTemplate]| {
            parts.iter().find_map(|part| blocker_of(index, part))
        };
        Some(match template {
            TyTemplate::List(element) => return blocker_of(index, element),
            TyTemplate::Map { key, value } => {
                return blocker_of(index, key).or_else(|| blocker_of(index, value));
            }
            TyTemplate::Union(members) => return parts(members),
            TyTemplate::Class(name, args) => {
                if index.class_at(name).is_none() {
                    return Some("a class not in the program");
                }
                return parts(args);
            }
            TyTemplate::Enum(name) => {
                if index.enum_at(name).is_none() {
                    return Some("an enum not in the program");
                }
                return None;
            }
            TyTemplate::Interface(name, args, assoc) => {
                if index.interface_at(name).is_none() {
                    return Some("an interface not in the program");
                }
                return parts(args)
                    .or_else(|| assoc.iter().find_map(|(_, ty)| blocker_of(index, ty)));
            }
            TyTemplate::Function { .. } => "a function type",
            TyTemplate::Future(..) => "a future type",
            TyTemplate::AssociatedTypeProjection { .. } => "a projection",
            TyTemplate::TypeAlias(_) => "a recursive alias",
            TyTemplate::EnumVariant(..) => "an enum variant type",
            TyTemplate::Literal(Literal::Bigint(_), _) => "a bigint literal type",
            TyTemplate::Bigint
            | TyTemplate::Uint8Array
            | TyTemplate::Media(_)
            | TyTemplate::RustType
            | TyTemplate::Type
            | TyTemplate::Resource
            | TyTemplate::PromptAst => "an opaque type",
            _ => return None,
        })
    }
    blocker_of(index, template).unwrap_or("a type this backend does not build")
}
