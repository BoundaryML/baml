fn main() {
    // libsui inserts a new Mach-O segment load command into the header when
    // embedding data. The default header padding the linker leaves is often
    // too small, causing the new command to overflow into __text. Ask the
    // Apple linker for extra room.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,-headerpad,0x300");
    }
}
