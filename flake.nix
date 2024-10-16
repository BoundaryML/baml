{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    flake-compat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        version = (builtins.fromTOML (builtins.readFile ./engine/Cargo.toml)).workspace.package.version;

        appleDeps = with pkgs.darwin.apple_sdk.frameworks; [
          CoreServices
          SystemConfiguration
          pkgs.libiconv-darwin
        ];
      in
        {
          packages.default = pkgs.rustPlatform.buildRustPackage {
            pname = "baml-cli";
            version = version;
            src = ./engine;
            LIBCLANG_PATH = pkgs.libclang.lib + "/lib/";
            BINDGEN_EXTRA_CLANG_ARGS = if pkgs.stdenv.isDarwin then
              "-I${pkgs.llvmPackages_18.libclang.lib}/lib/clang/18/headers "
            else
              "-isystem ${pkgs.llvmPackages_18.libclang.lib}/lib/clang/18/include -isystem ${pkgs.glibc.dev}/include";

            # Modify the test phase to only run library tests
            checkPhase = ''
              runHook preCheck
              echo "Running cargo test --lib"
              cargo test --lib
              runHook postCheck
            '';

            buildInputs = [pkgs.openssl] ++ (if pkgs.stdenv.isDarwin then appleDeps else []);
            nativeBuildInputs = [
              pkgs.openssl
              pkgs.ruby
            ];
            cargoLock = {
              lockFile = ./engine/Cargo.lock;
              outputHashes = {
                "pyo3-asyncio-0.21.0" = "sha256-5ZLzWkxp3e2u0B4+/JJTwO9SYKhtmBpMBiyIsTCW5Zw=";
                "serde_magnus-0.9.0" = "sha256-+iIHleftJ+Yl9QHEBVI91NOhBw9qtUZfgooHKoyY1w4=";
              };
            };
          };
        }
    );
}
