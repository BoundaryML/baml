{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-compat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, flake-utils, fenix, ... }:


    flake-utils.lib.eachDefaultSystem (system:

      let
        pkgs = nixpkgs.legacyPackages.${system};
        clang = pkgs.llvmPackages_19.clang;
        pythonEnv = pkgs.python3.withPackages (ps: []);

        toolchain = with fenix.packages.${system}; combine [
          minimal.cargo
          minimal.rustc
          minimal.rust-std
          targets.wasm32-unknown-unknown.latest.rust-std
        ];

        version = (builtins.fromTOML (builtins.readFile ./engine/Cargo.toml)).workspace.package.version;

        appleDeps = with pkgs.darwin.apple_sdk.frameworks; [
          CoreServices
          SystemConfiguration
          pkgs.libiconv-darwin
        ];

        rustPlatform = pkgs.makeRustPlatform {
          inherit (fenix.packages.${system}.minimal) cargo rustc;
          inherit (fenix.packages.${system}.latest) rust-std;
        };

        buildInputs = (with pkgs; [
          git
          openssl
          pkg-config
          lld_19
          pythonEnv
          ruby
          ruby.devEnv
          maturin
          pnpm
          nodejs
          vsce # VSCode extension packaging tool
          toolchain
          uv
          wasm-pack
          wasm-bindgen-cli
          pkgs.gcc
        ]) ++ (if pkgs.stdenv.isDarwin then appleDeps else []);
        nativeBuildInputs = [
          pkgs.openssl
          pkgs.pkg-config
          pkgs.ruby
          pythonEnv
          pkgs.maturin
          pkgs.perl
          pkgs.lld_19
          pkgs.gcc
        ];
        
        bamlCliInitData = pkgs.runCommand "baml-cli-init-data" {} ''
          mkdir -p $out
          cp -r ${./engine/baml-runtime/src/cli/initial_project/baml_src}/* $out
        '';

        promptFiddleExampleData = pkgs.runCommand "prompt-fiddle-example-data" {} ''
          mkdir -p $out
          cp -r ${./engine/baml-runtime/src/cli/initial_project/baml_src}/* $out
        '';

      in
        {
          packages.default = rustPlatform.buildRustPackage {

            # Disable tests in this build - FFI is a little tricky.
            doCheck = false;

            # Temporary: do a debug build instead of a release build, to speed up the dev cycle.
            buildType = "debug";

            pname = "baml-cli";
            version = version;
            src = ./engine;
            LIBCLANG_PATH = pkgs.libclang.lib + "/lib/";
            BINDGEN_EXTRA_CLANG_ARGS = if pkgs.stdenv.isDarwin then
              "-I${pkgs.llvmPackages_19.libclang.lib}/lib/clang/19/headers "
            else
              "-isystem ${pkgs.llvmPackages_19.libclang.lib}/lib/clang/19/include -isystem ${pkgs.glibc.dev}/include";

            cargoBuildFlags = "--bin baml-cli";

            cargoLock = { lockFile = ./engine/Cargo.lock; outputHashes = {
              "serde_magnus-0.9.0" = "sha256-+iIHleftJ+Yl9QHEBVI91NOhBw9qtUZfgooHKoyY1w4=";
            }; };

            # Add build-time environment variables
            RUSTFLAGS = if pkgs.stdenv.isDarwin
              then
                "-C target-feature=+crt-static --cfg tracing_unstable -C linker=lld --cfg tracing_unstable"
              else
                "-C target-feature=+crt-static --cfg tracing_unstable --cfg tracing_unstable -Zlinker-features=+lld -C linker=gcc";

            OPENSSL_STATIC = "1";
            OPENSSL_DIR = "${pkgs.openssl.dev}";
            OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
            OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";

            # Modify the test phase to only run library tests
            checkPhase = ''
              runHook preCheck
              echo "Running cargo test --lib"
              cargo test --lib
              runHook postCheck
            '';

            postPatch = ''
              # Disable baml syntax validation tests in build. They require too much
              # file system access to run.
              cat > baml-lib/baml/build.rs << 'EOF'
                fn main() {
                  println!("cargo:warning=Skipping baml syntax validation tests");
                }
              EOF
            '';

            inherit buildInputs;
            inherit nativeBuildInputs;

            BAML_CLI_INIT_DATA_DIR = bamlCliInitData;
            PROMPT_FIDDLE_EXAMPLE_DIR = promptFiddleExampleData;

            PYTHON_SYS_EXECUTABLE="${pythonEnv}/bin/python3";
            LD_LIBRARY_PATH="${pythonEnv}/lib";
            PYTHONPATH="${pythonEnv}/${pythonEnv.sitePackages}";
            # CC="${clang}/bin/clang"; # Temporarily commented out for linux testing.

          };
          devShell = pkgs.mkShell rec {
            inherit buildInputs;
            PATH="${clang}/bin:$PATH";
            LIBCLANG_PATH = pkgs.libclang.lib + "/lib/";
            RUSTFLAGS = "--cfg tracing_unstable";
          };
        }
    );
}
