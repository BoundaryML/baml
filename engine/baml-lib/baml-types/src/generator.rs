#[derive(
    Debug,
    Clone,
    Copy,
    strum::Display,
    strum::IntoStaticStr,
    strum::EnumString,
    strum::VariantArray,
    strum::VariantNames,
)]
pub enum GeneratorOutputType {
    #[strum(serialize = "rest/openapi")]
    OpenApi,

    #[strum(serialize = "python/pydantic")]
    PythonPydantic,

    #[strum(serialize = "typescript")]
    Typescript,

    #[strum(serialize = "ruby/sorbet")]
    RubySorbet,
}

impl GeneratorOutputType {
    pub fn default_client_mode(&self) -> GeneratorDefaultClientMode {
        match self {
            Self::OpenApi => GeneratorDefaultClientMode::Sync,
            // Due to legacy reasons, PythonPydantic and Typescript default to async
            // DO NOT CHANGE THIS DEFAULT EVER OR YOU WILL BREAK EXISTING USERS
            Self::PythonPydantic => GeneratorDefaultClientMode::Async,
            Self::Typescript => GeneratorDefaultClientMode::Async,
            Self::RubySorbet => GeneratorDefaultClientMode::Sync,
        }
    }

    /// Used to new generators when they are created (e.g. during baml-cli init)
    pub fn recommended_default_client_mode(&self) -> GeneratorDefaultClientMode {
        match self {
            Self::OpenApi => GeneratorDefaultClientMode::Sync,
            Self::PythonPydantic => GeneratorDefaultClientMode::Sync,
            Self::Typescript => GeneratorDefaultClientMode::Async,
            Self::RubySorbet => GeneratorDefaultClientMode::Sync,
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
