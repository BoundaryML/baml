fn main() {
    println!("cargo::rustc-check-cfg=cfg(getrandom_backend, values(\"custom\"))");
}
