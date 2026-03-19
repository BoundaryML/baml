pub mod parser;
pub mod runner;
pub mod updater;

#[cfg(test)]
mod test_files {
    include!(concat!(env!("OUT_DIR"), "/generated_lsp2_tests.rs"));
}
