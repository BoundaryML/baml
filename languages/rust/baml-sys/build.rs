fn main() {
    // The `links` key in Cargo.toml requires a build script that outputs
    // at least one `cargo:*` line. Since we load the library dynamically
    // at runtime, we don't need to output any link directives here.
    //
    // This script exists only to satisfy Cargo's requirement for a build
    // script when `links` is specified.
    //
    // Note: The `links` key prevents multiple versions of baml-sys from
    // being linked, which is important since the BAML library uses global
    // state internally.
}
