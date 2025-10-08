{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/7df7ff7d8e00218376575f0acdcc5d66741351ee";
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane = {
      url = "github:ipetkov/crane";
    };
    flake-compat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      nixpkgs-unstable,
      flake-utils,
      fenix,
      crane,
      ...
    }:

    flake-utils.lib.eachDefaultSystem (
      system:

      let
        pkgs = nixpkgs.legacyPackages.${system};
        pkgs-unstable = nixpkgs-unstable.legacyPackages.${system};
        clang = pkgs.llvmPackages.clang;
        pythonEnv = pkgs.python3.withPackages (ps: [ ]);

        toolchain = fenix.packages.${system}.fromToolchainFile {
          file = ./rust-toolchain.toml;
          sha256 = "sha256-+9FmLhAOezBZCOziO0Qct1NOrfpjNsXxc/8I0c7BdKE=";
        };

        version = (builtins.fromTOML (builtins.readFile ./engine/Cargo.toml)).workspace.package.version;

        appleDeps = pkgs.lib.optionals pkgs.stdenv.isDarwin (
          with pkgs.darwin;
          [
            libiconv
          ]
        );

        rustPlatform = pkgs.makeRustPlatform {
          inherit (fenix.packages.${system}.minimal) cargo rustc;
          inherit (fenix.packages.${system}.latest) rust-std;
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

        protocGenGo = pkgs.buildGoModule rec {
          pname = "protoc-gen-go";
          version = "1.34.1";

          src = pkgs.fetchFromGitHub {
            owner = "protocolbuffers";
            repo = "protobuf-go";
            rev = "v${version}";
            hash = "sha256-xbfqN/t6q5dFpg1CkxwxAQkUs8obfckMDqytYzuDwF4=";
          };

          vendorHash = "sha256-nGI/Bd6eMEoY0sBwWEtyhFowHVvwLKjbT4yfzFz6Z3E=";

          subPackages = [ "cmd/protoc-gen-go" ];

          meta = with pkgs.lib; {
            description = "Go support for Google's protocol buffers";
            mainProgram = "protoc-gen-go";
            homepage = "https://google.golang.org/protobuf";
            license = licenses.bsd3;
            maintainers = with maintainers; [ jojosch ];
          };
        };

        # Common source filtering for crane
        src = pkgs.lib.cleanSourceWith {
          src = ./engine;
          filter =
            path: type:
            let
              baseName = baseNameOf path;
            in
            !pkgs.lib.hasInfix "target" path
            && !pkgs.lib.hasInfix ".git" path
            && !pkgs.lib.hasInfix ".jj" path
            && !pkgs.lib.hasInfix ".so" path
            && !pkgs.lib.hasInfix ".node" path
            && !pkgs.lib.hasInfix "node_modules" path
            && baseName != "result";
        };

        # Common arguments for all crane builds
        commonArgs = {
          inherit
            src
            version
            buildInputs
            nativeBuildInputs
            ;
          strictDeps = true;

          LIBCLANG_PATH = pkgs.libclang.lib + "/lib/";
          BINDGEN_EXTRA_CLANG_ARGS =
            if pkgs.stdenv.isDarwin then
              "-I${pkgs.llvmPackages.libclang.lib}/lib/clang/${pkgs.llvmPackages.libclang.version}/headers "
            else
              "-isystem ${pkgs.llvmPackages.libclang.lib}/lib/clang/${pkgs.llvmPackages.libclang.version}/include -isystem ${pkgs.llvmPackages.libclang.lib}/include -isystem ${pkgs.glibc.dev}/include";
          RUSTFLAGS =
            if pkgs.stdenv.isDarwin then
              "--cfg tracing_unstable"
            else
              "--cfg tracing_unstable -C target-feature=+crt-static";
          OPENSSL_STATIC = "1";
          OPENSSL_DIR = "${pkgs.openssl.dev}";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
          OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
          PROTOC_GEN_GO_PATH = "${protocGenGo}/bin/protoc-gen-go";
          SKIP_BAML_VALIDATION = "1";
        };

        # Build dependencies only (this will be cached separately)
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        devEnvInputs = (
          with pkgs;
          [
          ]
        );

        buildInputs =
          (with pkgs; [
            cmake
            git
            go
            gotools
            ruby
            ruby.devEnv
            mise
            openssl
            pkg-config
            lld
            pythonEnv
            maturin
            pnpm
            protocGenGo
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
          ])
          ++ appleDeps;
        nativeBuildInputs = [
          pkgs.cmake
          pkgs.openssl
          pkgs.pkg-config
          pythonEnv
          pkgs.maturin
          pkgs.perl
          pkgs.ruby
        ]
        ++ pkgs.lib.optionals (!pkgs.stdenv.isDarwin) [
          pkgs.lld
          pkgs.gcc
        ];

        bamlRustPackage =
          {
            pname,
            buildPhase ? null,
            installPhase ? null,
            nativeBuildInputsExtra ? [ ],
            buildType,
            extraAttrs ? { },
          }:
          let
            cargoProfileDir = if buildType == "release" then "release" else "debug";
            releaseFlag = if buildType == "release" then "--release" else "";

            # Crane build function based on build type
            buildFn = if buildType == "release" then craneLib.buildPackage else craneLib.buildPackage;

            # Unset DEVELOPER_DIR_FOR_TARGET on macOS to avoid SDK conflicts
            preBuildWrapper = pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
              unset DEVELOPER_DIR_FOR_TARGET
            '';
          in
          buildFn (
            {
              inherit pname cargoArtifacts;
              inherit (commonArgs) src version buildInputs;
              inherit (commonArgs) LIBCLANG_PATH BINDGEN_EXTRA_CLANG_ARGS RUSTFLAGS;
              inherit (commonArgs)
                OPENSSL_STATIC
                OPENSSL_DIR
                OPENSSL_LIB_DIR
                OPENSSL_INCLUDE_DIR
                ;
              inherit (commonArgs) PROTOC_GEN_GO_PATH SKIP_BAML_VALIDATION;

              CARGO_PROFILE_DIR = cargoProfileDir;
              CARGO_RELEASE_FLAG = releaseFlag;
              CARGO_BUILD_RUSTFLAGS = commonArgs.RUSTFLAGS;

              nativeBuildInputs = nativeBuildInputs ++ nativeBuildInputsExtra;
              doCheck = false;

              # Set CARGO_PROFILE to control release vs debug builds
              CARGO_PROFILE = if buildType == "release" then "release" else "dev";

              # Prevent SDK conflicts on macOS
              preBuild = preBuildWrapper + (extraAttrs.preBuild or "");
            }
            // (if buildPhase != null then { inherit buildPhase; } else { })
            // (if installPhase != null then { inherit installPhase; } else { })
            // extraAttrs
          );
      in

      rec {

        packages.default = bamlRustPackage {
          pname = "baml-cli";
          buildType = "release";
          installPhase = ''
            runHook preInstall
            build_root=''${CARGO_TARGET_DIR:-target}
            profile_dir="$build_root/$CARGO_PROFILE_DIR"
            echo "Listing baml binaries under $profile_dir:"
            find "$profile_dir" -maxdepth 1 -type f -name "baml*" || true
            mkdir -p $out/bin
            BINARY_NAME="$profile_dir/baml-cli"
            if [ ! -x "$BINARY_NAME" ]; then
              echo "Unable to locate the compiled CLI binary at $BINARY_NAME" >&2
              exit 1
            fi
            echo "Found binary: $BINARY_NAME"
            cp "$BINARY_NAME" $out/bin/baml-cli
            strip $out/bin/baml-cli 2>/dev/null || true
            runHook postInstall
          '';
          extraAttrs = {
            PYTHON_SYS_EXECUTABLE = "${pythonEnv}/bin/python3";
            LD_LIBRARY_PATH = "${pythonEnv}/lib";
            PYTHONPATH = "${pythonEnv}/${pythonEnv.sitePackages}";
            # CC="${clang}/bin/clang"; # Temporarily commented out for linux testing.
          };
        };

        packages."baml-cli-musl" =
          if pkgs.stdenv.isDarwin then
            throw "musl builds are not supported on macOS - use the default package instead"
          else
            let
              muslPkgs = pkgs.pkgsStatic;

              muslCommonArgs = commonArgs // {
                buildInputs = (
                  with muslPkgs;
                  [
                    cmake
                    git
                    openssl
                    pkg-config
                    pythonEnv
                    gcc
                  ]
                );
                nativeBuildInputs = [
                  pkgs.cmake
                  muslPkgs.openssl
                  pkgs.pkg-config
                  pythonEnv
                  pkgs.perl
                ];
                CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
                CARGO_BUILD_RUSTFLAGS = "--cfg tracing_unstable -C target-feature=+crt-static";
                OPENSSL_STATIC = "1";
                OPENSSL_DIR = "${muslPkgs.openssl.dev}";
                OPENSSL_LIB_DIR = "${muslPkgs.openssl.out}/lib";
                OPENSSL_INCLUDE_DIR = "${muslPkgs.openssl.dev}/include";
              };
            in
            craneLib.buildPackage (
              muslCommonArgs
              // {
                pname = "baml-cli";
                cargoExtraArgs = "--target x86_64-unknown-linux-musl";

                installPhase = ''
                  runHook preInstall
                  build_root=''${CARGO_TARGET_DIR:-target}
                  mkdir -p $out/bin
                  BINARY_NAME="$build_root/x86_64-unknown-linux-musl/release/baml-cli"
                  if [ ! -x "$BINARY_NAME" ]; then
                    echo "Unable to locate the compiled musl CLI binary at $BINARY_NAME" >&2
                    exit 1
                  fi
                  cp "$BINARY_NAME" $out/bin/baml-cli
                  strip $out/bin/baml-cli 2>/dev/null || true
                  runHook postInstall
                '';
              }
            );

        packages."baml-cli-debug" = bamlRustPackage {
          pname = "baml-cli";
          buildType = "debug";
          installPhase = ''
            runHook preInstall
            build_root=''${CARGO_TARGET_DIR:-target}
            profile_dir="$build_root/$CARGO_PROFILE_DIR"
            echo "Listing baml binaries under $profile_dir:"
            find "$profile_dir" -maxdepth 1 -type f -name "baml*" || true
            mkdir -p $out/bin
            BINARY_NAME="$profile_dir/baml-cli"
            if [ ! -x "$BINARY_NAME" ]; then
              echo "Unable to locate the compiled CLI binary at $BINARY_NAME" >&2
              exit 1
            fi
            echo "Found binary: $BINARY_NAME"
            cp "$BINARY_NAME" $out/bin/baml-cli
            runHook postInstall
          '';
          extraAttrs = {
            PYTHON_SYS_EXECUTABLE = "${pythonEnv}/bin/python3";
            LD_LIBRARY_PATH = "${pythonEnv}/lib";
            PYTHONPATH = "${pythonEnv}/${pythonEnv.sitePackages}";
          };
        };

        packages.pyLib = bamlRustPackage {
          pname = "baml-cli";
          buildType = "release";
          nativeBuildInputsExtra = [
            pkgs.maturin
            pythonEnv
          ];
          buildPhase = ''
            # Unset conflicting environment variable for macOS SDK
            echo "Unsetting DEVELOPER_DIR_FOR_TARGET"
            unset DEVELOPER_DIR_FOR_TARGET

            cargo build $CARGO_RELEASE_FLAG
            cd language_client_python
            maturin build --offline $CARGO_RELEASE_FLAG --target-dir ../target --interpreter ${pythonEnv}/bin/python3
          '';
          installPhase = ''
            mkdir -p $out/lib
            ls ../target/wheels
            wheel_path=$(find ../target/wheels -maxdepth 1 -type f -name 'baml_py-*.whl' | head -n1)
            if [ -z "$wheel_path" ]; then
              echo "No wheel produced by maturin build" >&2
              exit 1
            fi
            # Preserve the actual wheel filename with platform tags
            cp "$wheel_path" "$out/lib/"
            echo "$wheel_path" > $out/wheel-name.txt
          '';
        };

        packages."baml-py-debug" = bamlRustPackage {
          pname = "baml-cli";
          buildType = "debug";
          nativeBuildInputsExtra = [
            pkgs.maturin
            pythonEnv
          ];
          buildPhase = ''
            # Unset conflicting environment variable for macOS SDK
            echo "Unsetting DEVELOPER_DIR_FOR_TARGET"
            unset DEVELOPER_DIR_FOR_TARGET

            cargo build
            cd language_client_python
            maturin build --offline --target-dir ../target --interpreter ${pythonEnv}/bin/python3
          '';
          installPhase = ''
            mkdir -p $out/lib
            ls ../target/wheels
            wheel_path=$(find ../target/wheels -maxdepth 1 -type f -name 'baml_py-*.whl' | head -n1)
            if [ -z "$wheel_path" ]; then
              echo "No wheel produced by maturin build" >&2
              exit 1
            fi
            # Preserve the actual wheel filename with platform tags
            cp "$wheel_path" "$out/lib/"
            echo "$wheel_path" > $out/wheel-name.txt
          '';
        };

        packages.baml-py = pkgs.python3Packages.buildPythonPackage {
          pname = "baml-py";
          inherit version;
          format = "wheel";

          # Find the actual wheel file with platform tags
          src =
            let
              wheelDir = "${packages.pyLib}/lib";
              wheelFile = builtins.head (builtins.attrNames (builtins.readDir wheelDir));
            in
            "${wheelDir}/${wheelFile}";

          propagatedBuildInputs = with pkgs.python3.pkgs; [
            pydantic
            typing-extensions
          ];

          pythonImportsCheck = [ "baml_py" ];
          doCheck = false;

          meta = with pkgs.lib; {
            description = "Python bindings for BAML";
            homepage = "https://github.com/boundaryml/baml";
            license = licenses.mit;
            platforms = platforms.unix;
          };
        };

        packages.tsLib = bamlRustPackage {
          pname = "baml-ts";
          buildType = "release";
          nativeBuildInputsExtra = [
            pkgs-unstable.nodejs_20
            pkgs.napi-rs-cli
            pkgs.pnpm
          ];
          buildPhase = ''
            # Unset conflicting environment variable for macOS SDK
            echo "Unsetting DEVELOPER_DIR_FOR_TARGET"
            unset DEVELOPER_DIR_FOR_TARGET

            # Build the CLI
            echo "Building the CLI"
            cargo build $CARGO_RELEASE_FLAG -p baml-cli

            # Build specifically the typescript FFI crate
            echo "Building the typescript FFI crate"
            cargo build $CARGO_RELEASE_FLAG -p baml-typescript-ffi

            # The build artifacts are in the crane-managed target directory
            echo "CARGO_TARGET_DIR is: ''${CARGO_TARGET_DIR:-target}"
            echo "Looking for build outputs..."
            find . -name "libbaml.*" -type f 2>/dev/null || true

            cd language_client_typescript

            echo "Listing current directory contents:"
            ls -la

            # Copy the built library to where napi expects it
            echo "Copying the built library to where napi expects it"
            build_root=''${CARGO_TARGET_DIR:-../target}
            cargo_lib_dir="$build_root"
            mkdir -p "$build_root/$CARGO_PROFILE_DIR"
            echo "Searching for shared libraries in $cargo_lib_dir:"
            find "$cargo_lib_dir" -name "*.so" -o -name "*.dylib" -o -name "*.dll" 2>/dev/null || true
            shared_lib=$(find "$cargo_lib_dir" -type f \( -name "libbaml.so" -o -name "libbaml.dylib" -o -name "libbaml.dll" \) 2>/dev/null | head -n1)
            if [ -z "$shared_lib" ]; then
              echo "Unable to locate built shared library" >&2
              echo "Trying absolute search from root of build..."
              find .. -name "libbaml.*" -type f 2>/dev/null || true
              exit 1
            fi
            lib_basename=$(basename "$shared_lib")
            case "$lib_basename" in
              *.so)
                cp "$shared_lib" "$build_root/$CARGO_PROFILE_DIR/libbaml_typescript_ffi.so"
                ;;
              *.dylib)
                cp "$shared_lib" "$build_root/$CARGO_PROFILE_DIR/libbaml_typescript_ffi.dylib"
                cp "$shared_lib" "$build_root/$CARGO_PROFILE_DIR/libbaml_typescript_ffi.so"
                ;;
              *.dll)
                cp "$shared_lib" "$build_root/$CARGO_PROFILE_DIR/libbaml_typescript_ffi.dll"
                ;;
            esac

            mkdir -p dist
            cp "$shared_lib" "dist/$lib_basename"

            # Only create symlink if it doesn't already exist with the right name
            ffi_lib=$(find "$cargo_lib_dir/$CARGO_PROFILE_DIR" -type f -name 'libbaml_typescript_ffi*.dylib' | head -n1)
            if [ -n "$ffi_lib" ] && [ "$(basename "$ffi_lib")" != "libbaml_typescript_ffi.dylib" ]; then
              mkdir -p "$build_root/$CARGO_PROFILE_DIR"
              ln -sf "$ffi_lib" "$build_root/$CARGO_PROFILE_DIR/libbaml_typescript_ffi.dylib"
            fi

            # Build the native module directly with release flag
            env -u DEVELOPER_DIR_FOR_TARGET napi build --platform $CARGO_RELEASE_FLAG --js ./native.js --dts ./native.d.ts

            # Compile TypeScript files using the Nix-provided TypeScript
            ${pkgs.nodePackages.typescript}/bin/tsc ./typescript_src/*.ts --outDir ./dist --module commonjs --allowJs --declaration true || true

            # Copy any pre-existing JavaScript files that might be needed
            cp *.js dist/ || true

            # Copy TypeScript declarations
            cp *.d.ts dist/ || true

            # Copy the native modules
            cp *.node dist/

            if [ "$(uname)" = "Darwin" ]; then
              echo "Fixing macOS Mach-O install names for bundled native modules"

              pending=1
              while [ "$pending" -eq 1 ]; do
                pending=0
                for bundle in dist/*.node dist/*.dylib dist/*.so; do
                  [ -e "$bundle" ] || continue
                  chmod +w "$bundle" 2>/dev/null || true

                  case "$(basename "$bundle")" in
                    *.dylib|*.so|*.node)
                      install_name_tool -id "@loader_path/$(basename "$bundle")" "$bundle"
                      ;;
                  esac

                  for dep in $(otool -L "$bundle" | tail -n +2 | awk '{print $1}'); do
                    [ -z "$dep" ] && continue
                    case "$dep" in
                      /System/*|@loader_path/*|@rpath/*)
                        ;;
                      /usr/lib/libiconv.2.dylib|/usr/lib/libcharset.1.dylib|/usr/lib/libSystem.B.dylib)
                        install_name_tool -change "$dep" "$dep" "$bundle"
                        ;;
                      *)
                        dep_name=$(basename "$dep")

                        case "$dep_name" in
                          libiconv.2.dylib)
                            install_name_tool -change "$dep" "/usr/lib/libiconv.2.dylib" "$bundle"
                            continue
                            ;;
                          libcharset.1.dylib)
                            install_name_tool -change "$dep" "/usr/lib/libcharset.1.dylib" "$bundle"
                            continue
                            ;;
                          libSystem.B.dylib)
                            install_name_tool -change "$dep" "/usr/lib/libSystem.B.dylib" "$bundle"
                            continue
                            ;;
                        esac

                        dest="dist/$dep_name"
                        if [ ! -e "$dest" ]; then
                          echo "  bundling $dep_name"
                          cp "$dep" "$dest"
                          chmod +w "$dest" 2>/dev/null || true
                          pending=1
                        fi
                        echo "    rewriting $(basename "$bundle") dependency $dep"
                        install_name_tool -change "$dep" "@loader_path/$dep_name" "$bundle"
                        ;;
                    esac
                  done
                done
              done
              strip -x dist/*.dylib 2>/dev/null || true
              strip -x dist/*.node 2>/dev/null || true
            else
              strip dist/*.so 2>/dev/null || true
            fi

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
                "*.dylib",
                "*.so",
                "*.dll",
                "bin/baml-cli"
              ],
              "dependencies": {},
              "os": ["linux", "darwin"],
              "cpu": ["x64", "arm64"]
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
            cp "$cargo_lib_dir/$CARGO_PROFILE_DIR/baml-cli" dist/bin/baml-cli
            strip dist/bin/baml-cli 2>/dev/null || true
          '';
          installPhase = ''
            mkdir -p $out/lib
            cp -r dist/* $out/lib/
          '';
          extraAttrs = {
            SKIP_BAML_VALIDATION = "1";
          };
        };

        packages."tsLib-debug" = bamlRustPackage {
          pname = "baml-ts";
          buildType = "debug";
          nativeBuildInputsExtra = [
            pkgs-unstable.nodejs_20
            pkgs.napi-rs-cli
            pkgs.pnpm
          ];
          buildPhase = ''
            echo "Unsetting DEVELOPER_DIR_FOR_TARGET"
            unset DEVELOPER_DIR_FOR_TARGET

            echo "Building the CLI"
            cargo build -p baml-cli

            echo "Building the typescript FFI crate"
            cargo build -p baml-typescript-ffi

            echo "CARGO_TARGET_DIR is: ''${CARGO_TARGET_DIR:-target}"
            echo "Looking for build outputs..."
            find . -name "libbaml.*" -type f 2>/dev/null || true

            cd language_client_typescript

            echo "Listing current directory contents:"
            ls -la

            echo "Copying the built library to where napi expects it"
            build_root=''${CARGO_TARGET_DIR:-../target}
            cargo_lib_dir="$build_root"
            mkdir -p "$build_root/debug"
            echo "Searching for shared libraries in $cargo_lib_dir:"
            find "$cargo_lib_dir" -name "*.so" -o -name "*.dylib" -o -name "*.dll" 2>/dev/null || true
            shared_lib=$(find "$cargo_lib_dir" -type f \( -name "libbaml.so" -o -name "libbaml.dylib" -o -name "libbaml.dll" \) 2>/dev/null | head -n1)
            if [ -z "$shared_lib" ]; then
              echo "Unable to locate built shared library" >&2
              echo "Trying absolute search from root of build..."
              find .. -name "libbaml.*" -type f 2>/dev/null || true
              exit 1
            fi
            lib_basename=$(basename "$shared_lib")
            case "$lib_basename" in
              *.so)
                cp "$shared_lib" "$build_root/debug/libbaml_typescript_ffi.so"
                ;;
              *.dylib)
                cp "$shared_lib" "$build_root/debug/libbaml_typescript_ffi.dylib"
                cp "$shared_lib" "$build_root/debug/libbaml_typescript_ffi.so"
                ;;
              *.dll)
                cp "$shared_lib" "$build_root/debug/libbaml_typescript_ffi.dll"
                ;;
            esac

            mkdir -p dist
            cp "$shared_lib" "dist/$lib_basename"

            # Only create symlink if it doesn't already exist with the right name
            ffi_lib=$(find "$cargo_lib_dir/debug" -type f -name 'libbaml_typescript_ffi*.dylib' | head -n1)
            if [ -n "$ffi_lib" ] && [ "$(basename "$ffi_lib")" != "libbaml_typescript_ffi.dylib" ]; then
              mkdir -p "$build_root/debug"
              ln -sf "$ffi_lib" "$build_root/debug/libbaml_typescript_ffi.dylib"
            fi

            env -u DEVELOPER_DIR_FOR_TARGET napi build --platform --js ./native.js --dts ./native.d.ts

            ${pkgs.nodePackages.typescript}/bin/tsc ./typescript_src/*.ts --outDir ./dist --module commonjs --allowJs --declaration true || true

            cp *.js dist/ || true
            cp *.d.ts dist/ || true
            cp *.node dist/

            if [ "$(uname)" = "Darwin" ]; then
              echo "Fixing macOS Mach-O install names for bundled native modules"

              pending=1
              while [ "$pending" -eq 1 ]; do
                pending=0
                for bundle in dist/*.node dist/*.dylib dist/*.so; do
                  [ -e "$bundle" ] || continue
                  chmod +w "$bundle" 2>/dev/null || true

                  case "$(basename "$bundle")" in
                    *.dylib|*.so|*.node)
                      install_name_tool -id "@loader_path/$(basename "$bundle")" "$bundle"
                      ;;
                  esac

                  for dep in $(otool -L "$bundle" | tail -n +2 | awk '{print $1}'); do
                    [ -z "$dep" ] && continue
                    case "$dep" in
                      /System/*|/usr/lib/*|@loader_path/*|@rpath/*)
                        ;;
                      *)
                        dep_name=$(basename "$dep")
                        dest="dist/$dep_name"

                        case "$dep_name" in
                          libiconv.2.dylib)
                            echo "    remapping $dep_name to /usr/lib/libiconv.2.dylib"
                            install_name_tool -change "$dep" "/usr/lib/libiconv.2.dylib" "$bundle"
                            continue
                            ;;
                          libcharset.1.dylib)
                            echo "    remapping $dep_name to /usr/lib/libcharset.1.dylib"
                            install_name_tool -change "$dep" "/usr/lib/libcharset.1.dylib" "$bundle"
                            continue
                            ;;
                          libintl.8.dylib)
                            echo "    remapping $dep_name to /usr/local/lib/libintl.8.dylib"
                            install_name_tool -change "$dep" "/usr/local/lib/libintl.8.dylib" "$bundle"
                            continue
                            ;;
                        esac

                        if [ ! -e "$dest" ]; then
                          echo "  bundling $dep_name"
                          cp "$dep" "$dest"
                          chmod +w "$dest" 2>/dev/null || true
                          pending=1
                        fi
                        echo "    rewriting $(basename "$bundle") dependency $dep"
                        install_name_tool -change "$dep" "@loader_path/$dep_name" "$bundle"
                        ;;
                    esac
                  done
                done
              done
            fi

            mkdir -p dist/bin
            cp "$cargo_lib_dir/debug/baml-cli" dist/bin/baml-cli
          '';
          installPhase = ''
            mkdir -p $out/lib
            cp -r dist/* $out/lib/
          '';
          extraAttrs = {
            SKIP_BAML_VALIDATION = "1";
          };
        };

        packages.baml-ts =
          let
            # Create a source with files in the correct location
            npmSource = pkgs.runCommand "baml-ts-${version}-source" { } ''
              mkdir -p $out
              cp -r ${packages.tsLib}/lib/* $out/
            '';
          in
          pkgs.buildNpmPackage {
            pname = "baml";
            inherit version;

            src = npmSource;

            npmDepsHash = "sha256-6l5OwLGhW+c2mUhVUDwxH5rs5pzxd0+uTOrx14q04KY=";
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

        packages."baml-ts-debug" =
          let
            npmSource = pkgs.runCommand "baml-ts-${version}-debug-source" { } ''
              mkdir -p $out
              cp -r ${packages."tsLib-debug"}/lib/* $out/
            '';
          in
          pkgs.buildNpmPackage {
            pname = "baml";
            inherit version;

            src = npmSource;

            npmDepsHash = "sha256-6l5OwLGhW+c2mUhVUDwxH5rs5pzxd0+uTOrx14q04KY=";
            forceEmptyCache = true;

            buildInputs = [ pkgs-unstable.nodejs_20 ];

            NPM_CONFIG_CACHE = "./tmp/npm";
            NPM_CONFIG_TMP = "./tmp/npm";
            NPM_CONFIG_PREFIX = "./tmp/npm";

            buildPhase = ''
              mkdir -p tmp/npm
              npm pack
            '';

            installPhase = ''
              mkdir -p $out/lib
              cp boundaryml-baml-${version}.tgz $out/lib/
            '';
          };

        devShell = pkgs.mkShell rec {
          inherit buildInputs;
          PATH = "${clang}/bin:$PATH";
          RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc;
          LIBCLANG_PATH = pkgs.libclang.lib + "/lib/";
          # UV_PYTHON = "${pythonEnv}/bin/python3"; // This doesn't work with maturin.
          BINDGEN_EXTRA_CLANG_ARGS =
            if pkgs.stdenv.isDarwin then
              "" # Rely on default includes provided by stdenv.cc + libclang
            else
              "-isystem ${pkgs.llvmPackages_17.libclang.lib}/lib/clang/17/include -isystem ${pkgs.llvmPackages_17.libclang.lib}/include -isystem ${pkgs.glibc.dev}/include";

          # Prevent SDK conflicts on macOS
          shellHook = pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
            unset DEVELOPER_DIR_FOR_TARGET
          '';
        };
      }
    );

}
