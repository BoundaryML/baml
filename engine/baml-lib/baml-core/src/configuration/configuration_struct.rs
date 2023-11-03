use super::Generator;
use crate::{internal_baml_diagnostics::Diagnostics, PreviewFeature};
use enumflags2::BitFlags;

#[derive(Debug)]
pub struct Configuration {
    pub generators: Vec<Generator>,
}

impl Configuration {
    pub fn validate_that_one_datasource_is_provided(&self) -> Result<(), Diagnostics> {
        Ok(())
    }

    pub fn max_identifier_length(&self) -> usize {
        1024
    }

    pub fn preview_features(&self) -> BitFlags<PreviewFeature> {
        self.generators
            .iter()
            .fold(BitFlags::empty(), |acc, _generator| acc)
    }
}
