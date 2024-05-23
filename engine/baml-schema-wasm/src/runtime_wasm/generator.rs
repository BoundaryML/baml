use internal_baml_codegen::GenerateOutput;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(getter_with_clone, inspectable)]
pub struct WasmGeneratorOutput {
    #[wasm_bindgen(readonly)]
    pub output_dir: String,
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

impl Into<WasmGeneratorOutput> for GenerateOutput {
    fn into(self) -> WasmGeneratorOutput {
        WasmGeneratorOutput {
            output_dir: self.output_dir.to_string_lossy().to_string(),
            files: self
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
