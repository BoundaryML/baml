use std::{collections::HashMap, path::PathBuf};

use super::DatamodelError;
use crate::{warning::DatamodelWarning, SourceFile, Span};

/// Represents a list of validation or parser errors and warnings.
///
/// This is used to accumulate multiple errors and warnings during validation.
/// It is used to not error out early and instead show multiple errors at once.
#[derive(Debug, Default, Clone)]
pub struct Diagnostics {
    pub root_path: PathBuf,
    current_file: Option<SourceFile>,
    errors: Vec<DatamodelError>,
    warnings: Vec<DatamodelWarning>,
}

impl std::fmt::Display for Diagnostics {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let err_str = self.to_pretty_string();
        let warn_str = self.warnings_to_pretty_string();

        if !err_str.is_empty() {
            writeln!(f, "{err_str}")?;
        }
        if !warn_str.is_empty() {
            writeln!(f, "{warn_str}")?;
        }

        Ok(())
    }
}

impl std::error::Error for Diagnostics {}

impl Diagnostics {
    pub fn new(root_path: PathBuf) -> Diagnostics {
        Diagnostics {
            root_path,
            current_file: None,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn span(&self, p: pest::Span<'_>) -> Span {
        match self.current_file {
            Some(ref file) => Span::new(file.clone(), p.start(), p.end()),
            None => panic!("No current file set."),
        }
    }

    pub fn set_source(&mut self, source: &SourceFile) {
        self.current_file = Some(source.clone())
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

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    pub fn to_result(&mut self) -> Result<(), Diagnostics> {
        if self.has_errors() {
            Err(std::mem::take(self))
        } else {
            Ok(())
        }
    }

    pub fn to_pretty_string(&self) -> String {
        let mut message: Vec<u8> = Vec::new();

        for err in self.errors() {
            err.pretty_print(&mut message)
                .expect("printing datamodel error");
        }

        String::from_utf8_lossy(&message).into_owned()
    }

    pub fn warnings_to_pretty_string(&self) -> String {
        let mut message: Vec<u8> = Vec::new();

        for warn in self.warnings() {
            warn.pretty_print(&mut message)
                .expect("printing datamodel warning");
        }

        String::from_utf8_lossy(&message).into_owned()
    }

    pub fn push(&mut self, mut other: Diagnostics) {
        self.errors.append(&mut other.errors);
        self.warnings.append(&mut other.warnings);
    }

    pub fn adjust_spans(&mut self, position_mapping: &HashMap<usize, usize>) {
        self.errors = self
            .errors
            .iter()
            .map(|err| {
                let new_start = *position_mapping
                    .get(&err.span().start)
                    .unwrap_or(&err.span().start);
                let new_end = *position_mapping
                    .get(&err.span().end)
                    .unwrap_or(&err.span().end);
                let new_span = Span::new(err.span().file.clone(), new_start, new_end);
                DatamodelError::new(err.message().to_string(), new_span)
            })
            .collect();

        self.warnings = self
            .warnings
            .iter()
            .map(|warn| {
                let new_start = *position_mapping
                    .get(&warn.span().start)
                    .unwrap_or(&warn.span().start);
                let new_end = *position_mapping
                    .get(&warn.span().end)
                    .unwrap_or(&warn.span().end);
                let new_span = Span::new(warn.span().file.clone(), new_start, new_end);
                DatamodelWarning::new(warn.message().to_string(), new_span)
            })
            .collect();
    }
}
