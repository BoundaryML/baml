use baml_project::testing::compile_source;
use bex_vm_types::{
    CaptureCategory, CaptureOption, Function, FunctionCaptureProps, LambdaKind, Object,
};

fn function<'a>(program: &'a bex_vm_types::Program, name: &str) -> &'a Function {
    program
        .objects
        .iter()
        .find_map(|object| match object {
            Object::Function(function) if function.name == name => Some(function.as_ref()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("compiled function `{name}`"))
}

#[test]
fn compiler_emits_structural_definition_and_lambda_identity() {
    let program = compile_source(
        r#"
        interface Runnable {
            function run(self) -> int throws never
        }

        class Worker {
            implements Runnable {
                function run(self) -> int { 1 }
            }
        }

        function main() -> int {
            let add_one = (x: int) -> int { x + 1 };
            let pending = spawn { add_one(40) };
            add_one(41)
        }
        "#,
    );

    let main = function(&program, "user.main");
    assert_eq!(main.def_meta.definition_key, "function:user.main");
    assert_eq!(main.def_meta.owner_type_key, None);
    assert_eq!(main.def_meta.lambda, None);

    let method = function(&program, "user.Worker.Runnable.run");
    assert_eq!(
        method.def_meta.definition_key,
        "function:user.Worker.Runnable.run"
    );
    assert_eq!(
        method.def_meta.owner_type_key.as_deref(),
        Some("class:user.Worker")
    );

    let mut lambdas = program
        .objects
        .iter()
        .filter_map(|object| match object {
            Object::Function(function) => function
                .def_meta
                .lambda
                .as_ref()
                .filter(|identity| identity.parent_definition_key == "function:user.main")
                .map(|identity| {
                    (
                        function.def_meta.definition_key.clone(),
                        identity.ordinal,
                        identity.kind,
                    )
                }),
            _ => None,
        })
        .collect::<Vec<_>>();
    lambdas.sort_by_key(|(_, ordinal, _)| *ordinal);
    assert_eq!(
        lambdas,
        vec![
            (
                "lambda:function:user.main#0".to_string(),
                0,
                LambdaKind::Lambda,
            ),
            (
                "lambda:function:user.main#1".to_string(),
                1,
                LambdaKind::SpawnedClosure,
            ),
        ]
    );

    let bytes = borsh::to_vec(&program).expect("serialize compiled program");
    let decoded: bex_vm_types::Program =
        borsh::from_slice(&bytes).expect("deserialize compiled program");
    assert_eq!(
        function(&decoded, "user.main").def_meta,
        main.def_meta,
        "definition metadata survives the Function borsh wire format"
    );
}

#[test]
fn compiler_emits_capture_policy_defaults() {
    let program = compile_source(
        r##"
        client C {
            provider openai
            options { model "gpt-4o" api_key "sk-test" }
        }

        function ask(name: string) -> string {
            client C
            prompt #"Hello, {{ name }}"#
        }

        function plain(name: string) -> string { name }
        "##,
    );

    let plain = function(&program, "user.plain").capture;
    assert_eq!(plain.inputs, CaptureOption::Disabled);
    assert_eq!(plain.output, CaptureOption::Disabled);
    assert_eq!(plain.error, CaptureOption::Auto);
    assert_eq!(plain.promote_on_error, CaptureOption::Auto);

    let llm = function(&program, "user.ask").capture;
    for category in [
        CaptureCategory::Input,
        CaptureCategory::Output,
        CaptureCategory::Error,
        CaptureCategory::PromoteOnError,
    ] {
        assert_eq!(llm.option(category), CaptureOption::Auto);
    }

    let builtin = program
        .objects
        .iter()
        .find_map(|object| match object {
            Object::Function(function)
                if function.origin == bex_vm_types::FunctionOrigin::Builtin =>
            {
                Some(function.capture)
            }
            _ => None,
        })
        .expect("stdlib builtin function");
    assert_eq!(builtin, FunctionCaptureProps::disabled());
}
