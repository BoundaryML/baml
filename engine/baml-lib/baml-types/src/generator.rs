#[derive(
    Debug,
    Clone,
    Copy,
    strum::Display,
    strum::IntoStaticStr,
    strum::EnumString,
    strum::VariantArray,
    strum::VariantNames,
    PartialEq,
    Eq,
)]
pub enum GeneratorOutputType {
    #[strum(serialize = "rest/openapi")]
    OpenApi,

    #[strum(serialize = "python/pydantic")]
    PythonPydantic,

    #[strum(serialize = "python/pydantic/v1")]
    PythonPydanticV1,

    #[strum(serialize = "typescript")]
    Typescript,

    #[strum(serialize = "typescript/react")]
    TypescriptReact,

    #[strum(serialize = "ruby/sorbet")]
    RubySorbet,

    #[strum(serialize = "go")]
    Go,

    #[strum(serialize = "rust")]
    Rust,
}

impl std::hash::Hash for GeneratorOutputType {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
    }
}

impl GeneratorOutputType {
    pub fn default_client_mode(&self) -> GeneratorDefaultClientMode {
        match self {
            Self::OpenApi => GeneratorDefaultClientMode::Sync,
            // Due to legacy reasons, PythonPydantic and Typescript default to async
            // DO NOT CHANGE THIS DEFAULT EVER OR YOU WILL BREAK EXISTING USERS
            Self::PythonPydantic => GeneratorDefaultClientMode::Async,
            // mimic legacy version
            Self::PythonPydanticV1 => GeneratorDefaultClientMode::Async,

            Self::Typescript => GeneratorDefaultClientMode::Async,
            Self::TypescriptReact => GeneratorDefaultClientMode::Async,
            Self::RubySorbet => GeneratorDefaultClientMode::Sync,
            Self::Go => GeneratorDefaultClientMode::Sync,
            Self::Rust => GeneratorDefaultClientMode::Sync,
        }
    }

    /// Used to new generators when they are created (e.g. during baml-cli init)
    pub fn recommended_default_client_mode(&self) -> GeneratorDefaultClientMode {
        match self {
            Self::OpenApi => GeneratorDefaultClientMode::Sync,
            Self::PythonPydantic => GeneratorDefaultClientMode::Sync,
            Self::PythonPydanticV1 => GeneratorDefaultClientMode::Sync,
            Self::Typescript => GeneratorDefaultClientMode::Async,
            Self::TypescriptReact => GeneratorDefaultClientMode::Async,
            Self::RubySorbet => GeneratorDefaultClientMode::Sync,
            Self::Go => GeneratorDefaultClientMode::Sync,
            Self::Rust => GeneratorDefaultClientMode::Async,
        }
    }
}

impl clap::ValueEnum for GeneratorOutputType {
    fn value_variants<'a>() -> &'a [Self] {
        use strum::VariantArray;
        Self::VARIANTS
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(clap::builder::PossibleValue::new(
            Into::<&'static str>::into(self),
        ))
    }
}

#[derive(Debug, Clone, strum::Display, strum::EnumString, strum::VariantNames, PartialEq, Eq)]
pub enum GeneratorDefaultClientMode {
    #[strum(serialize = "sync")]
    Sync,
    #[strum(serialize = "async")]
    Async,
}
