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

        toolchain = fenix.packages.${system}.fromToolchainFile {
          file = ./rust-toolchain.toml;
          sha256 = "sha256-+9FmLhAOezBZCOziO0Qct1NOrfpjNsXxc/8I0c7BdKE=";
        };

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

        buildInputs = (with pkgs; [
          cmake
          git
	        go
	        gotools
          openssl
          pkg-config
          lld_17
          pythonEnv
          ruby
          ruby.devEnv
          maturin
          pnpm
	        protoc-gen-go
          vsce # VSCode extension packaging tool
          toolchain
          pkgs-unstable.nodejs_20
          nodePackages.typescript
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

        wheelName = "baml_py-${version}-cp38-abi3-linux_x86_64.whl";

        bamlRustPackage = {
          pname,
          buildPhase ? null,
          installPhase ? null,
          nativeBuildInputsExtra ? [],
          buildType,
          extraAttrs ? {},
        }:
          rustPlatform.buildRustPackage ({
              inherit pname version;
              src = ./engine;
              filter = path: type:
                  let baseName = baseNameOf path; in
                  !pkgs.lib.hasInfix "target" path &&
                  !pkgs.lib.hasInfix ".git" path &&
                  !pkgs.lib.hasInfix ".jj" path &&
                  !pkgs.lib.hasInfix ".so" path &&
                  !pkgs.lib.hasInfix ".node" path &&
                  !pkgs.lib.hasInfix "node_modules" path &&
                  baseName != "result";

              LIBCLANG_PATH = pkgs.libclang.lib + "/lib/";
              BINDGEN_EXTRA_CLANG_ARGS = if pkgs.stdenv.isDarwin then
                "-I${pkgs.llvmPackages_17.libclang.lib}/lib/clang/17/headers "
              else
                "-isystem ${pkgs.llvmPackages_17.libclang.lib}/lib/clang/17/include -isystem ${pkgs.llvmPackages_17.libclang.lib}/include -isystem ${pkgs.glibc.dev}/include";
              RUSTFLAGS = if pkgs.stdenv.isDarwin
                then "--cfg tracing_unstable -C linker=lld"
                else "--cfg tracing_unstable -Zlinker-features=+lld -C linker=gcc";
              OPENSSL_STATIC = "1";
              OPENSSL_DIR = "${pkgs.openssl.dev}";
              OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
              OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
              inherit buildInputs;
              nativeBuildInputs = nativeBuildInputs ++ nativeBuildInputsExtra;
              doCheck = false;
              inherit buildType;
              cargoLock = { lockFile = ./engine/Cargo.lock; outputHashes = {}; };
              SKIP_BAML_VALIDATION = "1";
          }
          // (if buildPhase != null then { inherit buildPhase; } else {})
          // (if installPhase != null then { inherit installPhase; } else {})
          // extraAttrs);
        in

        rec {

          packages.default = bamlRustPackage {
            pname = "baml-cli";
            buildType = "debug";
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
            extraAttrs = {
              PYTHON_SYS_EXECUTABLE = "${pythonEnv}/bin/python3";
              LD_LIBRARY_PATH = "${pythonEnv}/lib";
              PYTHONPATH = "${pythonEnv}/${pythonEnv.sitePackages}";
              # CC="${clang}/bin/clang"; # Temporarily commented out for linux testing.
            };
          };

          packages.pyLib = bamlRustPackage {
            pname = "baml-cli";
            buildType = "debug";
            nativeBuildInputsExtra = [ pkgs.maturin pythonEnv ];
            buildPhase = ''
              cargo build
              cd language_client_python
              maturin build --offline --target-dir ../target
            '';
            installPhase = ''
              mkdir -p $out/lib
              ls ../target/wheels
              cp ../target/wheels/${wheelName} $out/lib/
              touch $out/results.txt
              ls -la $out >> $out/results.txt
              ls -la $out/lib >> $out/results.txt
              ls -la $out/lib/${wheelName} >> $out/results.txt
            '';
          };

          packages.baml-py = pkgs.python39Packages.buildPythonPackage {
            pname = "baml-py";
            inherit version;
            format = "wheel";

            src = "${packages.pyLib}/lib/${wheelName}";
            propagatedBuildInputs = with pkgs.python39.pkgs; [
              pydantic
              typing-extensions
            ];

            pythonImportsCheck = [ "baml_py" ];
            doCheck = false;

            meta = with pkgs.lib; {
              description = "Python bindings for BAML";
              homepage = "https://github.com/boundaryml/baml";
              license = licenses.mit;
              platforms = platforms.linux;
            };
          };

          packages.tsLib = bamlRustPackage {
            pname = "baml-ts";
            buildType = "debug";
            nativeBuildInputsExtra = [ pkgs-unstable.nodejs_20 pkgs.napi-rs-cli pkgs.pnpm ];
            buildPhase = ''
              # Build the CLI
              echo "Building the CLI"
              cargo build -p baml-cli

              # Build specifically the typescript FFI crate
              echo "Building the typescript FFI crate"
              cargo build -p baml-typescript-ffi
              cd language_client_typescript

              echo "Listing current directory contents:"
              ls -la

              # Copy the built library to where napi expects it
              echo "Copying the built library to where napi expects it"
              mkdir -p target/debug
              find ../target -name "*.so" -o -name "*.dylib" -o -name "*.dll"
              cp ../target/debug/libbaml.so target/debug/libbaml_typescript_ffi.so

              # Build the native module directly with release flag
              napi build --platform --js ./native.js --dts ./native.d.ts

              # Compile TypeScript files using the Nix-provided TypeScript
              ${pkgs.nodePackages.typescript}/bin/tsc ./typescript_src/*.ts --outDir ./dist --module commonjs --allowJs --declaration true || true

              # Copy any pre-existing JavaScript files that might be needed
              cp *.js dist/ || true

              # Copy TypeScript declarations
              cp *.d.ts dist/ || true

              # Copy the native modules
              cp *.node dist/

              # Create minimal package.json and package-lock.json
              cat > dist/package.json << EOF
              {
                "name": "@boundaryml/baml",
                "version": "${version}",
                "bin": {
                  "baml-cli": "./bin/baml-cli"
                },
                "files": [
                  "*.js",
                  "*.ts",
                  "*.node",
                  "bin/baml-cli"
                ],
                "dependencies": {},
                "os": ["linux"],
                "cpu": ["x64"]
              }
              EOF

              cat > dist/package-lock.json << EOF
              {
                "name": "@boundaryml/baml",
                "version": "${version}",
                "lockfileVersion": 2,
                "requires": true,
                "packages": {
                  "": {
                    "name": "@boundaryml/baml",
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
              cp ../target/debug/baml-cli dist/bin/baml-cli
            '';
            installPhase = ''
              mkdir -p $out/lib
              cp -r dist/* $out/lib/
            '';
            extraAttrs = {
              SKIP_BAML_VALIDATION = "1";
              cargoLock = { lockFile = ./engine/Cargo.lock; };
            };
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

            npmDepsHash = "sha256-p7AxgJSqngcwHwKsjF6u+fS0E27KY6/ulGIIRlZLsFU=";
            forceEmptyCache = true;

            buildInputs = [ pkgs-unstable.nodejs_20 ];

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
              touch $out/results.txt
              ls -lha
              ls -la  >> $out/results.txt
              cp boundaryml-baml-${version}.tgz $out/lib/
            '';
          };

          devShell = pkgs.mkShell rec {
            inherit buildInputs;
            PATH="${clang}/bin:$PATH";
            RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc;
            LIBCLANG_PATH = pkgs.libclang.lib + "/lib/";
            # UV_PYTHON = "${pythonEnv}/bin/python3"; // This doesn't work with maturin.
            BINDGEN_EXTRA_CLANG_ARGS = if pkgs.stdenv.isDarwin then
              "" # Rely on default includes provided by stdenv.cc + libclang
            else
              "-isystem ${pkgs.llvmPackages_17.libclang.lib}/lib/clang/17/include -isystem ${pkgs.llvmPackages_17.libclang.lib}/include -isystem ${pkgs.glibc.dev}/include";
          };
        }
    );

}
