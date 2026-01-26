use generators_lib::GenerateOutput;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(getter_with_clone, inspectable)]
pub struct WasmGeneratorOutput {
    #[wasm_bindgen(readonly)]
    pub output_dir: String,
    #[wasm_bindgen(readonly)]
    pub output_dir_relative_to_baml_src: String,
    #[wasm_bindgen(readonly)]
    pub files: Vec<WasmGeneratedFile>,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone)]
pub struct WasmGeneratedFile {
    #[wasm_bindgen(readonly)]
    pub path_in_output_dir: String,
    #[wasm_bindgen(readonly)]
    pub contents: String,
}

impl From<GenerateOutput> for WasmGeneratorOutput {
    fn from(generate_output: GenerateOutput) -> WasmGeneratorOutput {
        WasmGeneratorOutput {
            output_dir: generate_output
                .output_dir_full
                .to_string_lossy()
                .to_string(),
            output_dir_relative_to_baml_src: generate_output
                .output_dir_shorthand
                .to_string_lossy()
                .to_string(),
            files: generate_output
                .files
                .into_iter()
                .map(|(path, contents)| WasmGeneratedFile {
                    path_in_output_dir: path.to_string_lossy().to_string(),
                    contents,
                })
                .collect(),
        }
    }
}
