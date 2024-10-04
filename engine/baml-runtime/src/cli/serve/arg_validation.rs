use baml_types::{BamlMediaContent, BamlValue};

use super::error::BamlError;
use core::result::Result;

pub(super) trait BamlServeValidate {
    fn validate_for_baml_serve(&self) -> Result<(), BamlError>;
}

impl BamlServeValidate for BamlValue {
    fn validate_for_baml_serve(&self) -> Result<(), BamlError> {
        match &self {
          BamlValue::Media(m) => {
            match m.content {
              BamlMediaContent::File(_) => Err(BamlError::InvalidArgument {
                message: format!("BAML-over-HTTP only supports URLs and base64-encoded {} media (file is invalid)", m.media_type)
              }),
              BamlMediaContent::Url(_) => Ok(()),
              BamlMediaContent::Base64(_) => Ok(()),
            }
          }
          BamlValue::List(l) => {
            for v in l {
              v.validate_for_baml_serve()?;
            }
            Ok(())
          }
          BamlValue::Map(m) => {
            for (k, v) in m {
              v.validate_for_baml_serve()?;
            }
            Ok(())
          }
          BamlValue::Class(_, fields) => {
            for (_, v) in fields {
              v.validate_for_baml_serve()?;
            }
            Ok(())
          }
          BamlValue::Bool(_) |
          BamlValue::Enum(_, _) |
          BamlValue::Float(_) |
          BamlValue::Int(_) |
          BamlValue::Null |
          BamlValue::String(_) => Ok(()),
        }
    }
}
