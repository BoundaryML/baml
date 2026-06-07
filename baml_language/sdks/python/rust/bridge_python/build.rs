fn main() {
    pyo3_build_config::add_extension_module_link_args();
    pyo3_build_config::add_libpython_rpath_link_args();
}
