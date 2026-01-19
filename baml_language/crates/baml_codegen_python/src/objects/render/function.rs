baml_codegen_types::render_fn! {
    /// ```askama
    /// def {{function_.name}}{{ function_.render_args(*ns) }} -> {{ function_.return_type.render(*ns) }}:
    ///     {{ function_.assembed_docstring.as_docstring()|indent(4) }}
    /// ```
    pub fn print_signature(function_: &crate::objects::Function, ns: crate::ty::Namespace) -> String;
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
