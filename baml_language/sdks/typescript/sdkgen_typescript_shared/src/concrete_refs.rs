//! Concrete callers retain the compiler-selected implementation signature and
//! dispatch target. They do not widen it to the existential interface method.
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use baml_codegen_types::{Class, ConcreteDeclaration, ConcreteMethodTarget, Ty};
use baml_type::{ParamTy, RuntimeTy, Ty as SemanticTy};
use prost::Message;

use crate::{
    interface_refs::RenderedRefs,
    translate_ty::{TranslateCtx, translate_input_ty, translate_ty},
    ts_string,
};

pub(crate) fn render(
    declaration: &ConcreteDeclaration,
    class: &Class,
    ctx: &TranslateCtx,
) -> RenderedRefs {
    let mut out = RenderedRefs::default();
    let helpers = &ctx.interfaces.as_ref().expect("names").helpers[&ctx.current_leaf];
    let class_parameters: BTreeMap<_, _> = declaration
        .generic_params
        .iter()
        .zip(&class.generic_params)
        .map(|(p, name)| (p.param.clone(), name.clone()))
        .collect();
    let authored: BTreeSet<_> = declaration
        .methods
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    for method in &declaration.methods {
        let mut parameters = class_parameters.clone();
        let mut reserved = ctx.interfaces.as_ref().expect("names").helpers[&ctx.current_leaf]
            .reserved
            .clone();
        reserved.extend(class.generic_params.iter().map(ToString::to_string));
        let generics = method
            .generic_params
            .iter()
            .enumerate()
            .map(|(index, p)| {
                let native =
                    crate::interface_names::allocate(format!("_Method{index}"), &mut reserved);
                parameters.insert(p.param.clone(), baml_base::Name::new(&native));
                native
            })
            .collect::<Vec<_>>();
        let mut project = |ty: &RuntimeTy, input: bool| {
            let semantic = baml_type::unify::rewrite_ty(
                &SemanticTy::from(ty.clone()),
                &mut |node| match node {
                    SemanticTy::TypeVar(p, attr) => parameters.get(p).map(|name| {
                        SemanticTy::TypeVar(ParamTy::new(p.index(), name.clone()), attr.clone())
                    }),
                    _ => None,
                },
            );
            let ty = Ty::try_from(&semantic).unwrap_or_else(|error| {
                panic!(
                    "cannot project TypeScript concrete caller {}.{}: {error}",
                    declaration.name, method.name
                )
            });
            let native = if input {
                translate_input_ty(&ty, ctx)
            } else {
                translate_ty(&ty, ctx)
            };
            let expr = native.expr.clone();
            out.signatures.push(native);
            expr
        };
        let RuntimeTy::Function {
            params,
            ret,
            throws,
            ..
        } = &method.signature
        else {
            panic!(
                "concrete caller {}.{} is not a function",
                declaration.name, method.name
            );
        };
        let mut arguments = Vec::new();
        let mut fields = vec!["$ctx?: BamlCallContext | undefined".to_owned()];
        let mut wire = Vec::new();
        for (index, parameter) in params.iter().skip(1).enumerate() {
            let name = parameter
                .name
                .as_ref()
                .map_or_else(|| format!("arg{index}"), ToString::to_string);
            let ty = project(&parameter.ty, true);
            let ty = if generics.is_empty() {
                ty
            } else {
                format!("{}<{ty}>", helpers.no_infer)
            };
            if parameter.mode == baml_type::FunctionParamMode::Optional {
                fields.push(format!("{}?: {ty} | undefined", ts_string(&name)));
                wire.push(format!(
                    "...($opts?.[{}] === undefined ? {{}} : {{ [{}]: $opts[{}] }})",
                    ts_string(&name),
                    ts_string(&name),
                    ts_string(&name)
                ));
            } else {
                arguments.push(format!("_arg{index}: {ty}"));
                wire.push(format!("[{}]: _arg{index}", ts_string(&name)));
            }
        }
        let ret = project(ret, false);
        let error = project(throws, false).replace("*/", "* /");
        let (generic_decl, choices, slots) = if generics.is_empty() {
            (String::new(), "undefined".to_owned(), String::new())
        } else {
            out.uses_type_tokens = true;
            let slots = method
                .generic_params
                .iter()
                .map(|p| ts_string(p.param.as_str()))
                .collect::<Vec<_>>();
            fields.push(format!(
                "$types: {{ {} }}",
                slots
                    .iter()
                    .zip(&generics)
                    .map(|(name, native)| format!("{name}: {}<{native}>", helpers.typed_type))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
            (
                format!("<{}>", generics.join(", ")),
                "$opts.$types".to_owned(),
                slots.join(", "),
            )
        };
        arguments.push(format!(
            "$opts{}: {{ {} }}",
            if generics.is_empty() { "?" } else { "" },
            fields.join("; ")
        ));
        let pattern = match &method.target {
            ConcreteMethodTarget::Inherent { class } => {
                assert_eq!(class, &declaration.name);
                "null".to_owned()
            }
            ConcreteMethodTarget::Interface(view) => {
                let ty = RuntimeTy::Interface(
                    view.name.clone(),
                    view.generics.clone(),
                    view.associated_types.clone(),
                    baml_base::TyAttr::EMPTY,
                );
                let bytes = bridge_ctypes::runtime_ty_to_proto_ty(&ty).encode_to_vec();
                format!(
                    "[{}]",
                    bytes
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        };
        let native_name = crate::interface_refs::method_name(method.name.as_str(), &authored);
        writeln!(
            out.source,
            "  /** Calls {}.{}; declared BAML throws: {error}. */",
            declaration.name, method.name
        )
        .unwrap();
        writeln!(
            out.source,
            "  [{}]{generic_decl}({}): Promise<{ret}> {{",
            ts_string(&native_name),
            arguments.join(", ")
        )
        .unwrap();
        writeln!(out.source, "    return this._invokeConcrete({}, {pattern}, {}, {{ {} }}, {choices}, [{slots}], $opts?.$ctx) as Promise<{ret}>;\n  }}", ts_string(&declaration.name.to_string()), ts_string(method.name.as_str()), wire.join(", ")).unwrap();
    }
    for (name, interfaces) in &declaration.ambiguous_methods {
        writeln!(
            out.source,
            "  // {name} requires qualification: {}.",
            interfaces
                .iter()
                .map(|i| i.name.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
    }
    out
}
