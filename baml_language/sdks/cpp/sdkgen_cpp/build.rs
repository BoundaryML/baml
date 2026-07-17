//! Gzips the checked-in generated protobuf sources (~1.9 MB of text) so the
//! emitter embeds ~150 KB instead: `baml-cli` carries every generator, and
//! the size gate budgets binary growth. The vendored bridge headers stay
//! plain `include_str!` (they are ~60 KB total and greppable in the binary).

use std::{env, fs, io::Write as _, path::Path};

const PB_DIR: &str = "../bridge_cpp/pb/baml_bridge/cffi/v1";
const PB_FILES: &[&str] = &[
    "baml_handle.pb.h",
    "baml_handle.pb.cc",
    "baml_inbound.pb.h",
    "baml_inbound.pb.cc",
    "baml_outbound.pb.h",
    "baml_outbound.pb.cc",
    "baml_type.pb.h",
    "baml_type.pb.cc",
];

fn main() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR");
    for name in PB_FILES {
        let src = Path::new(PB_DIR).join(name);
        println!("cargo:rerun-if-changed={}", src.display());
        let bytes = fs::read(&src)
            .unwrap_or_else(|e| panic!("read {name}: {e} (run the pb_generation bless test)"));
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        enc.write_all(&bytes).expect("gzip write");
        let gz = enc.finish().expect("gzip finish");
        fs::write(Path::new(&out_dir).join(format!("{name}.gz")), gz)
            .unwrap_or_else(|e| panic!("write {name}.gz: {e}"));
    }
}
