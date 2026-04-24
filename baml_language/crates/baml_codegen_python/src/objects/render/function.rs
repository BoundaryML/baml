baml_codegen_types::render_fn! {
    /// ```askama
    /// def {{function_.name}}{{ function_.render_args(*ns) }} -> {{ function_.return_type.render(*ns) }}:
    ///     {{ function_.assembed_docstring.as_docstring()|indent(4) }}
    /// ```
    pub fn print_signature(function_: &crate::objects::Function, ns: crate::ty::Namespace) -> String;
}

baml_codegen_types::render_fn! {
    /// ```askama
    /// def {{function_.name}}(self,
    ///     {{ function_.render_method_params(*ns) }}
    ///     baml_options: BamlCallOptions = {},
    /// ) -> {{ function_.return_type.render(*ns) }}:
    ///     __result__ = self.__options.merge_options(baml_options).call_function_sync(
    ///         function_name="{{ function_.name }}",
    ///         args={{ function_.render_args_dict() }},
    ///     )
    ///     return {{ function_.render_coerce_result(*ns) }}
    /// ```
    pub fn print_sync_impl(function_: &crate::objects::Function, ns: crate::ty::Namespace) -> String;
}

baml_codegen_types::render_fn! {
    /// ```askama
    /// async def {{function_.name}}(self,
    ///     {{ function_.render_method_params(*ns) }}
    ///     baml_options: BamlCallOptions = {},
    /// ) -> {{ function_.return_type.render(*ns) }}:
    ///     __result__ = await self.__options.merge_options(baml_options).call_function_async(
    ///         function_name="{{ function_.name }}",
    ///         args={{ function_.render_args_dict() }},
    ///     )
    ///     return {{ function_.render_coerce_result(*ns) }}
    /// ```
    pub fn print_async_impl(function_: &crate::objects::Function, ns: crate::ty::Namespace) -> String;
}

baml_codegen_types::render_fn! {
    /// ```askama
    /// def {{function_.name}}{{ function_.render_args(*ns) }} -> {{ function_.return_type.render(*ns) }}:
    ///     {{ function_.assembed_docstring.as_docstring()|indent(4) }}
    ///     __result__ = _get_runtime().merge_options(baml_options or {}).call_function_sync(
    ///         function_name="{{ function_.wire_name }}",
    ///         args={{ function_.render_args_dict() }},
    ///     )
    ///     return {{ function_.render_coerce_result(*ns) }}
    /// ```
    pub fn print_sync_module_fn(function_: &crate::objects::Function, ns: crate::ty::Namespace) -> String;
}

baml_codegen_types::render_fn! {
    /// ```askama
    /// async def {{function_.name}}{{ function_.render_args(*ns) }} -> {{ function_.return_type.render(*ns) }}:
    ///     {{ function_.assembed_docstring.as_docstring()|indent(4) }}
    ///     __result__ = await _get_runtime().merge_options(baml_options or {}).call_function_async(
    ///         function_name="{{ function_.wire_name }}",
    ///         args={{ function_.render_args_dict() }},
    ///     )
    ///     return {{ function_.render_coerce_result(*ns) }}
    /// ```
    pub fn print_async_module_fn(function_: &crate::objects::Function, ns: crate::ty::Namespace) -> String;
}

baml_codegen_types::render_fn! {
    /// ```askama
    /// def {{function_.name}}{{ function_.render_args(*ns) }} -> {{ function_.return_type.render(*ns) }}: ...
    /// ```
    pub fn print_module_fn_pyi(function_: &crate::objects::Function, ns: crate::ty::Namespace) -> String;
}

impl crate::objects::Function {
    fn render_args(&self, ns: crate::ty::Namespace) -> String {
        let args = self
            .arguments
            .iter()
            .map(|arg| arg.render(ns))
            .collect::<Vec<_>>();
        // if the length of the string is > 120, use multiline format
        if args.len() > 1 {
            return format!("(\n    {},\n)", args.join(",\n    "));
        }
        format!("({})", args.join(", "))
    }

    /// Same as `render_args` but callable from plain Rust code (not just askama templates).
    pub(super) fn render_args_str(&self, ns: crate::ty::Namespace) -> String {
        self.render_args(ns)
    }

    /// Callable from plain Rust code.
    pub(super) fn render_coerce_result_str(&self, ns: crate::ty::Namespace) -> String {
        self.render_coerce_result(ns)
    }

    /// Callable from plain Rust code.
    pub(super) fn render_args_dict_str(&self) -> String {
        self.render_args_dict()
    }

    /// Render method parameters (without `self`, without `baml_options`).
    /// Each param on its own line with trailing comma.
    fn render_method_params(&self, ns: crate::ty::Namespace) -> String {
        // Filter out the baml_options argument (it's added by the template)
        self.arguments
            .iter()
            .filter(|arg| arg.name.as_str() != "baml_options")
            .map(|arg| format!("{},", arg.render(ns)))
            .collect::<Vec<_>>()
            .join("\n    ")
    }

    /// Render the args dict for the runtime call: `{"arg1": arg1, "arg2": arg2}`
    fn render_args_dict(&self) -> String {
        let entries: Vec<String> = self
            .arguments
            .iter()
            .filter(|arg| arg.name.as_str() != "baml_options")
            .map(|arg| format!("\"{}\": {}", arg.name, arg.name))
            .collect();
        if entries.is_empty() {
            return "{}".to_string();
        }
        format!("{{{}}}", entries.join(", "))
    }

    /// Render the expression that coerces a `FunctionResult` into the return type.
    ///
    /// For class return types: `types.Resume(**__result__.result())`
    /// For primitive/other types: `__result__.result()`
    fn render_coerce_result(&self, ns: crate::ty::Namespace) -> String {
        render_coerce_expr("__result__.result()", &self.return_type, ns)
    }
}

/// Render an expression that coerces a raw Python value into the expected type.
///
/// - Class: `Type(**val)`
/// - Enum: `Type(val)`
/// - List(Class): `[Type(**item) for item in val]`
/// - List(other): unchanged
/// - Map with class value: `{k: Type(**v) for k, v in val.items()}`
/// - Primitives/unions: unchanged
fn render_coerce_expr(val: &str, ty: &crate::ty::Ty, ns: crate::ty::Namespace) -> String {
    match ty {
        crate::ty::Ty::Class(name) => {
            format!("{}(**{})", name.render(ns), val)
        }
        crate::ty::Ty::Enum(name) => {
            format!("{}({})", name.render(ns), val)
        }
        crate::ty::Ty::List(inner) => {
            let item_expr = render_coerce_expr("item", inner, ns);
            if item_expr == "item" {
                val.to_string()
            } else {
                format!("[{item_expr} for item in {val}]")
            }
        }
        crate::ty::Ty::Map { key: _, value } => {
            let val_expr = render_coerce_expr("v", value, ns);
            if val_expr == "v" {
                val.to_string()
            } else {
                format!("{{k: {val_expr} for k, v in {val}.items()}}")
            }
        }
        _ => val.to_string(),
    }
}

impl crate::objects::FunctionArgument {
    fn render(&self, ns: crate::ty::Namespace) -> String {
        if let Some(default_value) = &self.default_value {
            format!("{}: {} = {}", self.name, self.ty.render(ns), default_value)
        } else {
            format!("{}: {}", self.name, self.ty.render(ns))
        }
    }
}
