use convert_case::{Case, Casing};
use wasm_bindgen::prelude::*;

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
        "hot reload v5".to_string()
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
    pub fn new(src: &str) -> Self {
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
