#[cfg(test)]
mod b1607_diagnostic_ownership;
#[cfg(test)]
mod lsp_issues_authoring;
#[cfg(test)]
mod lsp_issues_generic_hover;
#[cfg(test)]
mod memoization;
pub mod parser;
pub mod runner;
pub mod updater;

#[cfg(test)]
mod test_files {
    include!(concat!(env!("OUT_DIR"), "/generated_lsp2_tests.rs"));
}
mod range_tokens_test;
mod typing_robustness_test;
