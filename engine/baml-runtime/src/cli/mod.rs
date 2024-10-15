pub mod dev;
pub mod generate;
pub mod init;
pub mod serve;

use internal_baml_core::configuration::GeneratorOutputType;

/// Default values for the CLI to use.
///
/// We ship different variants of the CLI today:
///
///   - `baml-cli` as bundled with the Python package
///   - `baml-cli` as bundled with the NPM package
///   - `baml-cli` as bundled with the Ruby gem
///
/// Each of these ship with different defaults, as appropriate for
/// the language that they're bundled with.
#[derive(Clone, Copy, Debug)]
pub struct RuntimeCliDefaults {
    pub output_type: GeneratorOutputType,
}
