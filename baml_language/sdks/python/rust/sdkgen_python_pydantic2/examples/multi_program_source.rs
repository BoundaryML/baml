//! Source-payload fixtures consumed by `sdk_tests/multi_program/run.py`.
use std::{collections::HashMap, path::PathBuf};

fn main() {
    let destination = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .expect("fixture output directory"),
    );
    for (name, value) in [("source_a", 11), ("source_b", 22)] {
        let files = vec![(
            PathBuf::from("main.baml"),
            format!("function value() -> int {{ {value} }}"),
        )];
        let generated = sdkgen_python_pydantic2::to_source_code(
            &HashMap::new(),
            &files,
            sdkgen_python_pydantic2::NamingConvention::PreserveCase,
        );
        for (path, contents) in generated {
            let path = destination.join(name).join("python/baml_sdk").join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
    }
}
