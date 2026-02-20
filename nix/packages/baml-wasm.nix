# BAML WASM package (baml-schema-wasm via wasm-bindgen)
{
  crane,
  pkgs,
  toolchain,
  src,
  version,
  wasm-bindgen-cli,
}:

let
  craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

  # Separate cargoArtifacts for WASM target
  cargoArtifacts = craneLib.buildDepsOnly {
    inherit src version;
    strictDeps = true;
    doCheck = false;
    CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
    cargoExtraArgs = "-p baml-schema-build";
  };

  baml-schema-wasm = craneLib.buildPackage {
    inherit src version cargoArtifacts;
    pname = "baml-schema-wasm";
    strictDeps = true;
    doCheck = false;

    CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
    cargoExtraArgs = "-p baml-schema-build";

    nativeBuildInputs = [
      wasm-bindgen-cli
    ];

    # Skip default install (cargo install doesn't work for cdylib wasm targets)
    # Instead, run wasm-bindgen to produce JS/TS glue code
    installPhaseCommand = ''
      build_root=''${CARGO_TARGET_DIR:-target}
      wasm_file=$(find "$build_root/wasm32-unknown-unknown/release" -name "*.wasm" -type f | head -n1)
      if [ -z "$wasm_file" ]; then
        echo "No .wasm file found" >&2
        find "$build_root" -name "*.wasm" -type f 2>/dev/null || true
        exit 1
      fi
      echo "Found wasm file: $wasm_file"

      mkdir -p $out/pkg
      wasm-bindgen --out-dir $out/pkg --target bundler "$wasm_file"
    '';
  };

in
{
  inherit cargoArtifacts baml-schema-wasm;
}
