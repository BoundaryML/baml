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

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{
        objects::Function,
        ty::{Namespace, Ty},
    };

    macro_rules! test_function_signature {
        (
            $test_name:ident:
            fn $name:ident($($arg_name:ident: $arg_ty:literal $(@ $arg_doc:literal)?),* $(,)?)
                $(@ $doc:literal)?
                -> $ret_ty:literal
            =>
            $expected:expr
        ) => {
            #[test]
            fn $test_name() {
                let function = baml_codegen_tests::function!(
                    fn $name($($arg_name: $arg_ty $(@ $arg_doc)?),*) $(@ $doc)? -> $ret_ty
                );
                let function = Function::from_codegen_types(&function, Ty::from_codegen_types(&function.return_type));
                assert_eq!(
                    print_signature(&function, Namespace::Types),
                    crate::docstring::dedent($expected).trim()
                );
            }
        };
    }

    test_function_signature! {
        fn_no_args:
        fn get_value() -> "string"
        =>
        r#"
            def get_value(baml_options: typing.Optional[baml.Options] = None) -> str:
                """
                Args:
                  baml_options: See `baml.Options` for more information
                """
            "#
    }

    test_function_signature! {
        fn_one_arg:
        fn greet(name: "string") -> "string"
        =>
        r#"
            def greet(
                name: str,
                baml_options: typing.Optional[baml.Options] = None,
            ) -> str:
                """
                Args:
                  name: none
                  baml_options: See `baml.Options` for more information
                """
            "#
    }

    test_function_signature! {
        fn_multiple_args:
        fn add(a: "int", b: "int") -> "int"
        =>
        r#"
            def add(
                a: int,
                b: int,
                baml_options: typing.Optional[baml.Options] = None,
            ) -> int:
                """
                Args:
                  a: none
                  b: none
                  baml_options: See `baml.Options` for more information
                """
            "#
    }

    test_function_signature! {
        fn_with_docstring:
        fn process(input: "string")
            @ "Process the input string"
            -> "string"
        =>
        r#"
            def process(
                input: str,
                baml_options: typing.Optional[baml.Options] = None,
            ) -> str:
                """
                Process the input string

                Args:
                  input: none
                  baml_options: See `baml.Options` for more information
                """
            "#
    }

    test_function_signature! {
        fn_with_arg_docstring:
        fn calculate(value: "int" @ "The value to calculate") -> "float"
        =>
        r#"
            def calculate(
                value: int,
                baml_options: typing.Optional[baml.Options] = None,
            ) -> float:
                """
                Args:
                  value: The value to calculate
                  baml_options: See `baml.Options` for more information
                """
            "#
    }

    test_function_signature! {
        fn_with_all_docstrings:
        fn transform(data: "string" @ "Input data", factor: "float" @ "Scale factor")
            @ "Transform data by a factor"
            -> "string"
        =>
        r#"
            def transform(
                data: str,
                factor: float,
                baml_options: typing.Optional[baml.Options] = None,
            ) -> str:
                """
                Transform data by a factor

                Args:
                  data: Input data
                  factor: Scale factor
                  baml_options: See `baml.Options` for more information
                """
            "#
    }

    test_function_signature! {
        fn_optional_return:
        fn find(id: "int") -> "string?"
        =>
        r#"
            def find(
                id: int,
                baml_options: typing.Optional[baml.Options] = None,
            ) -> typing.Optional[str]:
                """
                Args:
                  id: none
                  baml_options: See `baml.Options` for more information
                """
            "#
    }

    test_function_signature! {
        fn_list_return:
        fn list_items() -> "string[]"
        =>
        r#"
            def list_items(baml_options: typing.Optional[baml.Options] = None) -> typing.List[str]:
                """
                Args:
                  baml_options: See `baml.Options` for more information
                """
            "#
    }

    test_function_signature! {
        fn_class_return:
        fn get_user(id: "int") -> "User"
        =>
        r#"
            def get_user(
                id: int,
                baml_options: typing.Optional[baml.Options] = None,
            ) -> User:
                """
                Args:
                  id: none
                  baml_options: See `baml.Options` for more information
                """
            "#
    }

    test_function_signature! {
        fn_complex_types:
        fn process_users(users: "User[]", filter: "string?") -> "User[]"
        =>
        r#"
            def process_users(
                users: typing.List[User],
                filter: typing.Optional[str] = None,
                baml_options: typing.Optional[baml.Options] = None,
            ) -> typing.List[User]:
                """
                Args:
                  users: none
                  filter: none
                  baml_options: See `baml.Options` for more information
                """
            "#
    }

    test_function_signature! {
        fn_multiple_default_values:
        fn process_users(users: "User[]", filter: "string?", option: "int?") -> "User[]"
        =>
        r#"
            def process_users(
                users: typing.List[User],
                filter: typing.Optional[str] = None,
                option: typing.Optional[int] = None,
                baml_options: typing.Optional[baml.Options] = None,
            ) -> typing.List[User]:
                """
                Args:
                  users: none
                  filter: none
                  option: none
                  baml_options: See `baml.Options` for more information
                """
            "#
    }

    test_function_signature! {
        fn_multiple_default_values_with_gap:
        // note filter is now mandatory, but option is optional
        // because default values are only applied if the subsequent arguments have default values
        fn process_users(users: "User[]", filter: "string?", value: "int", option: "int?") -> "User[]"
        =>
        r#"
            def process_users(
                users: typing.List[User],
                filter: typing.Optional[str],
                value: int,
                option: typing.Optional[int] = None,
                baml_options: typing.Optional[baml.Options] = None,
            ) -> typing.List[User]:
                """
                Args:
                  users: none
                  filter: none
                  value: none
                  option: none
                  baml_options: See `baml.Options` for more information
                """
            "#
    }
}
