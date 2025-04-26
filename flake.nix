{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.05";
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixos-unstable";
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

  outputs = { self, nixpkgs, nixpkgs-unstable, flake-utils, fenix, ... }:


    flake-utils.lib.eachDefaultSystem (system:

      let
        pkgs = nixpkgs.legacyPackages.${system};
        pkgs-unstable = nixpkgs-unstable.legacyPackages.${system};
        clang = pkgs.llvmPackages_17.clang;
        pythonEnv = pkgs.python39.withPackages (ps: []);

        toolchain = with fenix.packages.${system}; combine [
          complete.cargo
          complete.rustc
          complete.rust-std
          complete.rustfmt
          complete.rust-analyzer
          targets.wasm32-unknown-unknown.latest.rust-std
	        targets.x86_64-unknown-linux-musl.latest.rust-std
        ];

        version = (builtins.fromTOML (builtins.readFile ./engine/Cargo.toml)).workspace.package.version;

        appleDeps = with pkgs.darwin.apple_sdk.frameworks; [
          CoreServices
          System
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
          pkgs-unstable.uv
          pkgs-unstable.flatbuffers
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
          pkgs.cmake
          pkgs.openssl
          pkgs.pkg-config
          pkgs.ruby
          pythonEnv
          pkgs.maturin
          pkgs.perl
          pkgs.lld_17
          pkgs.gcc
        ];

        wheelName = "baml-${version}-cp39-cp39-manylinux_2_28_x86_64.whl";

        
      in
        rec {
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

            installPhase = ''
              runHook preInstall
              echo "Listing baml binaries in target/debug:"
              find target/debug -type f -name "baml*"
              mkdir -p $out/bin
              BINARY_NAME=$(find target/debug -type f -executable -name "baml*" | head -n1)
              echo "Found binary: $BINARY_NAME"
              cp "$BINARY_NAME" $out/bin/baml-cli
              runHook postInstall
            '';
          };

          packages.pyLib = rustPlatform.buildRustPackage {
            pname = "baml-cli";
            inherit version;
            src = ./engine;
            cargoLock.lockFile = ./engine/Cargo.lock;

            LIBCLANG_PATH = pkgs.libclang.lib + "/lib/";
            BINDGEN_EXTRA_CLANG_ARGS = if pkgs.stdenv.isDarwin then
              "-I${pkgs.llvmPackages_17.libclang.lib}/lib/clang/17/headers "
            else
              "-isystem ${pkgs.llvmPackages_17.libclang.lib}/lib/clang/17/include -isystem ${pkgs.llvmPackages_17.libclang.lib}/include -isystem ${pkgs.glibc.dev}/include";

            buildType = "debug";
            doCheck = false;

            # Skip BAML validation during build
            SKIP_BAML_VALIDATION = "1";

            RUSTFLAGS = if pkgs.stdenv.isDarwin
              then
                "--cfg tracing_unstable -C linker=lld"
              else
                "--cfg tracing_unstable -Zlinker-features=+lld -C linker=gcc";

            OPENSSL_STATIC = "1";
            OPENSSL_DIR = "${pkgs.openssl.dev}";
            OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
            OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";

            nativeBuildInputs = nativeBuildInputs ++ [
              pkgs.maturin
              pythonEnv
            ];

            buildInputs = buildInputs;

            buildPhase = ''
              cargo build
              cd language_client_python
              maturin build --offline --target-dir ../target
            '';

            installPhase = ''
              mkdir -p $out/lib
              cp ../target/wheels/baml_py-${version}-cp38-abi3-linux_x86_64.whl $out/lib/
            '';
          };

          packages.baml-py = pkgs.python39Packages.buildPythonPackage {
            pname = "baml-py";
            inherit version;
            format = "wheel";
            
            src = "${packages.pyLib}/lib/baml_py-${version}-cp38-abi3-linux_x86_64.whl";
            
            propagatedBuildInputs = with pkgs.python39Packages; [
              pydantic
              typing-extensions
            ];

            pythonImportsCheck = [ "baml_py" ];
            doCheck = false;

            meta = with pkgs.lib; {
              description = "Python bindings for BAML";
              homepage = "https://github.com/BoundaryML/baml";
              license = licenses.mit;
              maintainers = [];
            };
          };

          packages.tsLib = rustPlatform.buildRustPackage {
            pname = "baml-ts";
            inherit version;
            src = ./engine;
            cargoLock.lockFile = ./engine/Cargo.lock;

            LIBCLANG_PATH = pkgs.libclang.lib + "/lib/";
            BINDGEN_EXTRA_CLANG_ARGS = if pkgs.stdenv.isDarwin then
              "-I${pkgs.llvmPackages_17.libclang.lib}/lib/clang/17/headers "
            else
              "-isystem ${pkgs.llvmPackages_17.libclang.lib}/lib/clang/17/include -isystem ${pkgs.llvmPackages_17.libclang.lib}/include -isystem ${pkgs.glibc.dev}/include";

            doCheck = false;

            # Skip BAML validation during build
            SKIP_BAML_VALIDATION = "1";

            RUSTFLAGS = if pkgs.stdenv.isDarwin
              then
                "--cfg tracing_unstable -C linker=lld"
              else
                "--cfg tracing_unstable -Zlinker-features=+lld -C linker=gcc";

            nativeBuildInputs = nativeBuildInputs ++ [ 
              pkgs.nodejs
              pkgs.napi-rs-cli
            ];

            buildInputs = buildInputs;

            buildPhase = ''
              # Build specifically the typescript FFI crate
              cargo build -p baml-typescript-ffi
              cd language_client_typescript
              
              echo "Listing target directory contents:"
              ls -R ../target
              
              echo "Listing current directory contents:"
              ls -la
              
              # Copy the built library to where napi expects it
              mkdir -p target/debug
              find ../target -name "*.so" -o -name "*.dylib" -o -name "*.dll"
              cp ../target/debug/libbaml.so target/debug/libbaml_typescript_ffi.so
              echo "Running napi build..."
              napi build --platform 2>&1
              mkdir -p dist
              cp index.js index.d.ts native.js native.d.ts stream.js stream.d.ts type_builder.js type_builder.d.ts dist/
              
              # Create minimal package.json and package-lock.json
              cat > dist/package.json << EOF
              {
                "name": "baml-ts",
                "version": "${version}",
                "bin": {
                  "baml-cli": "./bin/baml-cli"
                },
                "dependencies": {},
                "os": ["linux"],
                "cpu": ["x64"]
              }
              EOF
              
              cat > dist/package-lock.json << EOF
              {
                "name": "baml-ts",
                "version": "${version}",
                "lockfileVersion": 2,
                "requires": true,
                "packages": {
                  "": {
                    "name": "baml-ts",
                    "version": "${version}",
                    "dependencies": {},
                    "bin": {
                      "baml-cli": "bin/baml-cli"
                    }
                  }
                }
              }
              EOF

              # Copy the CLI binary
              mkdir -p dist/bin
              cp ${packages.default}/bin/baml-cli dist/bin/baml-cli
            '';

            installPhase = ''
              mkdir -p $out/lib
              cp -r dist/* $out/lib/
            '';
          };

          packages.baml-ts = let
            # Create a source with files in the correct location
            npmSource = pkgs.runCommand "baml-ts-${version}-source" {} ''
              mkdir -p $out
              cp -r ${packages.tsLib}/lib/* $out/
            '';
          in pkgs.buildNpmPackage {
            pname = "baml";
            inherit version;
            
            src = npmSource;

            npmDepsHash = "sha256-VCrDNrdYv0X5XtPA8iLwpji8+bla1vK4M8p9mfMIP5w=";
            forceEmptyCache = true;

            buildInputs = [ pkgs.nodejs ];

            # Configure npm to use temporary directories
            NPM_CONFIG_CACHE = "./tmp/npm";
            NPM_CONFIG_TMP = "./tmp/npm";
            NPM_CONFIG_PREFIX = "./tmp/npm";

            buildPhase = ''
              # Ensure temp directories exist
              mkdir -p tmp/npm
              npm pack
            '';

            installPhase = ''
              mkdir -p $out/lib
              cp baml-ts-${version}.tgz $out/lib/
            '';
          };

          devShell = pkgs.mkShell rec {
            inherit buildInputs;
            PATH="${clang}/bin:$PATH";
            RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc;
            LIBCLANG_PATH = pkgs.libclang.lib + "/lib/";
            BINDGEN_EXTRA_CLANG_ARGS = if pkgs.stdenv.isDarwin then
              "" # Rely on default includes provided by stdenv.cc + libclang
            else
              "-isystem ${pkgs.llvmPackages_17.libclang.lib}/lib/clang/17/include -isystem ${pkgs.llvmPackages_17.libclang.lib}/include -isystem ${pkgs.glibc.dev}/include";
          };
        }
    );
}
