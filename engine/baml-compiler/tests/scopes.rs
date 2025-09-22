//! Compiler tests for local variable scoping.

use baml_compiler::test::ast;
use baml_vm::{BamlVmProgram, EvalStack};

#[test]
fn locals_in_scope() -> anyhow::Result<()> {
    let ast = ast(r#"
        function main() -> int {
            let x = 0;

            let a = {
                let y = 0;

                let b = {
                    let c = 1;
                    let d = 2;
                    [c, d]
                };
                let e = {
                    let f = 4;
                    let g = 5;
                    [f, g]
                };

                [b, e]
            };

            let h = {
                let z = 0;

                let i = {
                    let w = 0;
                    let j = 8;
                    [w, j]
                };

                [i]
            };

            0
        }
    "#)?;

    let BamlVmProgram {
        objects,
        resolved_function_names,
        globals,
        ..
    } = baml_compiler::compile(&ast)?;

    let main = objects[resolved_function_names["main"].0].as_function()?;
    baml_vm::debug::disassemble(main, &EvalStack::new(), &objects, &globals);

    let expected_locals_in_scope = [
        vec!["<fn main>", "x", "a", "h"],
        vec!["<fn main>", "x", "y", "b", "e"],
        vec!["<fn main>", "x", "y", "c", "d"],
        vec!["<fn main>", "x", "y", "b", "f", "g"],
        vec!["<fn main>", "x", "a", "z", "i"],
        vec!["<fn main>", "x", "a", "z", "w", "j"],
    ];

    assert_eq!(
        main.locals_in_scope,
        expected_locals_in_scope
            .iter()
            .map(|scope| scope.iter().map(ToString::to_string).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    );

    Ok(())
}
