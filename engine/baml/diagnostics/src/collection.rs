use super::DatamodelError;
use crate::warning::DatamodelWarning;

/// Represents a list of validation or parser errors and warnings.
///
/// This is used to accumulate multiple errors and warnings during validation.
/// It is used to not error out early and instead show multiple errors at once.
#[derive(Debug)]
pub struct Diagnostics {
    errors: Vec<DatamodelError>,
    warnings: Vec<DatamodelWarning>,
}

impl Diagnostics {
    pub fn new() -> Diagnostics {
        Diagnostics {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn warnings(&self) -> &[DatamodelWarning] {
        &self.warnings
    }

    pub fn into_warnings(self) -> Vec<DatamodelWarning> {
        self.warnings
    }

    pub fn errors(&self) -> &[DatamodelError] {
        &self.errors
    }

    pub fn push_error(&mut self, err: DatamodelError) {
        self.errors.push(err)
    }

    pub fn push_warning(&mut self, warning: DatamodelWarning) {
        self.warnings.push(warning)
    }

    /// Returns true, if there is at least one error in this collection.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn to_result(&mut self) -> Result<(), Diagnostics> {
        if self.has_errors() {
            Err(std::mem::take(self))
        } else {
            Ok(())
        }
    }

    pub fn to_pretty_string(&self, file_name: &str, datamodel_string: &str) -> String {
        let mut message: Vec<u8> = Vec::new();

        for err in self.errors() {
            err.pretty_print(&mut message, file_name, datamodel_string)
                .expect("printing datamodel error");
        }

        String::from_utf8_lossy(&message).into_owned()
    }

    pub fn warnings_to_pretty_string(&self, file_name: &str, datamodel_string: &str) -> String {
        let mut message: Vec<u8> = Vec::new();

        for warn in self.warnings() {
            warn.pretty_print(&mut message, file_name, datamodel_string)
                .expect("printing datamodel warning");
        }

        String::from_utf8_lossy(&message).into_owned()
    }
}

impl From<DatamodelError> for Diagnostics {
    fn from(error: DatamodelError) -> Self {
        let mut col = Diagnostics::new();
        col.push_error(error);
        col
    }
}

impl From<DatamodelWarning> for Diagnostics {
    fn from(warning: DatamodelWarning) -> Self {
        let mut col = Diagnostics::new();
        col.push_warning(warning);
        col
    }
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self::new()
    }
}
