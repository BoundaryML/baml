fn main() {
    let target = std::env::var("TARGET").unwrap();
    if target.contains("apple") {
        println!("cargo:rustc-env=MACOSX_DEPLOYMENT_TARGET=11.0");
    }
}
