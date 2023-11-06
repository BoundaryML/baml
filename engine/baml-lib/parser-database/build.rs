fn main() {
    // If you have an existing build.rs file, just add this line to it.
    #[cfg(feature = "use-pyo3")]
    pyo3_build_config::use_pyo3_cfgs();
}
