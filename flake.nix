{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.05";
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
        clang = pkgs.llvmPackages_17.clang;
        pythonEnv = pkgs.python3.withPackages (ps: []);

        toolchain = with fenix.packages.${system}; combine [
          minimal.cargo
          minimal.rustc
          minimal.rust-std
          complete.rustfmt
          targets.wasm32-unknown-unknown.latest.rust-std
	        targets.x86_64-unknown-linux-musl.latest.rust-std
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

	# wasm-bindgen-cli = pkgs.rustPlatform.buildRustPackage rec {
	#   pname = "wasm-bindgen-cli";
	#   version = "0.2.92";
	#   src = pkgs.fetchFromGitHub {
	#     owner = "rustwasm";
	#     repo = "wasm-bindgen";
	#     rev = "${version}";
	#     sha256 = "sha256-VMt+J5sazHPqmAdsoueS2WW6Pn1tvugaJaPnSJq9038=";
	#   };
	#   cargoHash = "sha256-+iIHleftJ+Yl9QHEBVI91NOhBw9qtUZfgooHKoyY1w4=";
	#   buildInputs = with pkgs; [ openssl ];
	#   nativeBuildInputs = with pkgs; [ pkg-config ];
	#   cargoBuildFlags = ["--package wasm-bindgen-cli"];
	# };

        buildInputs = (with pkgs; [
          cmake
          git
          openssl
          pkg-config
          lld_17
          pythonEnv
          ruby
          ruby.devEnv
          maturin
          vsce # VSCode extension packaging tool
          toolchain
          nodejs
          uv
          wasm-pack
          pkgs.gcc
          napi-rs-cli
          wasm-bindgen-cli

          # For building the typescript client.
          pixman
          cairo
          pango
          libjpeg
          giflib
          librsvg
        ]) ++ (if pkgs.stdenv.isDarwin then appleDeps else []);
        nativeBuildInputs = [
          pkgs.openssl
          pkgs.pkg-config
          pkgs.ruby
          pythonEnv
          pkgs.maturin
          pkgs.perl
          pkgs.lld_17
          pkgs.gcc
        ];

        
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
              "-I${pkgs.llvmPackages_17.libclang.lib}/lib/clang/17/headers "
            else
              "-isystem ${pkgs.llvmPackages_17.libclang.lib}/lib/clang/17/include -isystem ${pkgs.llvmPackages_17.libclang.lib}/include -isystem ${pkgs.glibc.dev}/include";

            cargoLock = { lockFile = ./engine/Cargo.lock; outputHashes = {
            }; };

            # Add build-time environment variables
            RUSTFLAGS = if pkgs.stdenv.isDarwin
              then
                "--cfg tracing_unstable -C linker=lld"
              else
                "--cfg tracing_unstable -Zlinker-features=+lld -C linker=gcc";

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

            PYTHON_SYS_EXECUTABLE="${pythonEnv}/bin/python3";
            LD_LIBRARY_PATH="${pythonEnv}/lib";
            PYTHONPATH="${pythonEnv}/${pythonEnv.sitePackages}";
            # CC="${clang}/bin/clang"; # Temporarily commented out for linux testing.

          };
          devShell = pkgs.mkShell rec {
            inherit buildInputs;
            PATH="${clang}/bin:$PATH";
            LIBCLANG_PATH = pkgs.libclang.lib + "/lib/";
            BINDGEN_EXTRA_CLANG_ARGS = if pkgs.stdenv.isDarwin then
              "" # Rely on default includes provided by stdenv.cc + libclang
            else
              "-isystem ${pkgs.llvmPackages_17.libclang.lib}/lib/clang/17/include -isystem ${pkgs.llvmPackages_17.libclang.lib}/include -isystem ${pkgs.glibc.dev}/include";
          };
        }
    );
}
