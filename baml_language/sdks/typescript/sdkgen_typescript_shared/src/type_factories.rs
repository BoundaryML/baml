//! SDK-bound type evidence for nominal declarations, independent of constructors.
use std::fmt::Write as _;

use baml_codegen_types::{Symbol, SymbolPool, Ty};
use baml_type::ParamTy;

use crate::{
    interface_refs::RenderedRefs,
    translate_ty::{TranslateCtx, translate_ty},
    ts_string,
};

pub(crate) fn render(pool: &SymbolPool, ctx: &TranslateCtx) -> RenderedRefs {
    let names = ctx.interfaces.as_ref().expect("names");
    let mut out = RenderedRefs::default();
    let Some(helpers) = names.helpers.get(&ctx.current_leaf) else {
        return out;
    };
    for (name, factory) in &names.type_factories {
        if crate::routing::route_class_ref(name) != ctx.current_leaf {
            continue;
        }
        let (kind, count) = match &pool[name] {
            Symbol::Class(class) => ("class", class.generic_params.len()),
            Symbol::Enum(_) => ("enum", 0),
            _ => unreachable!("nominal type factory"),
        };
        let mut reserved = helpers.reserved.clone();
        let variables = (0..count)
            .map(|i| crate::interface_names::allocate(format!("_T{i}"), &mut reserved))
            .collect::<Vec<_>>();
        let args = variables
            .iter()
            .enumerate()
            .map(|(index, name)| {
                Ty::TypeVar(
                    ParamTy::new(
                        index.try_into().expect("generic index"),
                        baml_base::Name::new(name),
                    ),
                    baml_base::TyAttr::EMPTY,
                )
            })
            .collect();
        let ty = if kind == "class" {
            Ty::Class(name.clone(), args, baml_base::TyAttr::EMPTY)
        } else {
            Ty::Enum(name.clone(), baml_base::TyAttr::EMPTY)
        };
        let native = translate_ty(&ty, ctx);
        let result = &native.expr;
        let generics = if variables.is_empty() {
            String::new()
        } else {
            format!("<{}>", variables.join(", "))
        };
        let params = variables
            .iter()
            .enumerate()
            .map(|(i, ty)| format!("_type{i}: {}<{ty}>", helpers.typed_type))
            .collect::<Vec<_>>()
            .join(", ");
        let tokens = (0..count)
            .map(|i| format!("_type{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            out.source,
            "\n/** Checked type evidence for {name}; does not construct a value. */"
        )
        .unwrap();
        writeln!(
            out.source,
            "export function {factory}{generics}({params}): {}<{result}> {{",
            helpers.typed_type
        )
        .unwrap();
        writeln!(out.source, "  if (arguments.length !== {count}) throw new TypeError(\"expected {count} type arguments\");").unwrap();
        writeln!(
            out.source,
            "  return {}<{result}>({}, {}, [{tokens}]);\n}}",
            helpers.declare_type,
            ts_string(kind),
            ts_string(&name.to_string())
        )
        .unwrap();
        out.signatures.push(native);
    }
    out
}
