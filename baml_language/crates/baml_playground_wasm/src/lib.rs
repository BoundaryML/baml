use convert_case::{Case, Casing};
use wasm_bindgen::prelude::*;

#[cfg(feature = "console_error_panic")]
extern crate console_error_panic_hook;

#[cfg(feature = "small_allocator")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(feature = "console_error_panic")]
    console_error_panic_hook::set_once();
}

/// A set of string casing variants generated from a BAML source input.
#[wasm_bindgen]
pub struct CasingVariants {
    original: String,
    lower: String,
    upper: String,
    camel: String,
    pascal: String,
    upper_snake: String,
    snake: String,
    kebab: String,
    title: String,
}

#[wasm_bindgen]
impl CasingVariants {
    #[wasm_bindgen(getter)]
    pub fn original(&self) -> String {
        self.original.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn lower(&self) -> String {
        self.lower.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn upper(&self) -> String {
        self.upper.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn camel(&self) -> String {
        self.camel.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn pascal(&self) -> String {
        self.pascal.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn snake(&self) -> String {
        "hot reload v1".to_string()
        //self.snake.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn upper_snake(&self) -> String {
        self.upper_snake.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn kebab(&self) -> String {
        self.kebab.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn title(&self) -> String {
        self.title.clone()
    }
}

impl CasingVariants {
    fn new(src: &str) -> Self {
        Self {
            original: src.to_string(),
            lower: src.to_case(Case::Lower),
            upper: src.to_case(Case::Upper),
            camel: src.to_case(Case::Camel),
            pascal: src.to_case(Case::Pascal),
            snake: src.to_case(Case::Snake),
            upper_snake: src.to_case(Case::UpperSnake),
            kebab: src.to_case(Case::Kebab),
            title: src.to_case(Case::Title),
        }
    }
}

/// A basic runtime wrapper around BAML source content.
#[wasm_bindgen]
pub struct BamlRuntime {
    baml_src: String,
}

#[wasm_bindgen]
impl BamlRuntime {
    #[wasm_bindgen(constructor)]
    pub fn new(baml_src: String) -> BamlRuntime {
        BamlRuntime { baml_src }
    }

    /// Renders the stored BAML source into a set of naming-case variants.
    #[wasm_bindgen]
    pub fn render(&self) -> CasingVariants {
        CasingVariants::new(&self.baml_src)
    }

    /// Allows updating the stored BAML source for subsequent renders.
    #[wasm_bindgen]
    pub fn set_source(&mut self, baml_src: String) {
        self.baml_src = baml_src;
    }

    /// Convenience helper returning the raw BAML source currently stored.
    #[wasm_bindgen(getter)]
    pub fn baml_src(&self) -> String {
        self.baml_src.clone()
    }
}
