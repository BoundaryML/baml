# BAML TypeScript NAPI bindings + npm tarball
{
  bamlRustPackage,
  pkgs,
  pkgs-unstable,
  commonArgs,
  version,
}:

let
  tsLib = bamlRustPackage {
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

  tsLibDebug = bamlRustPackage {
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

  baml-ts =
    let
      npmSource = pkgs.runCommand "baml-ts-${version}-source" { } ''
        mkdir -p $out
        cp -r ${tsLib}/lib/* $out/
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
        touch $out/results.txt
        ls -lha
        ls -la  >> $out/results.txt
        cp boundaryml-baml-${version}.tgz $out/lib/
      '';
    };

  baml-ts-debug =
    let
      npmSource = pkgs.runCommand "baml-ts-${version}-debug-source" { } ''
        mkdir -p $out
        cp -r ${tsLibDebug}/lib/* $out/
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

in
{
  inherit tsLib tsLibDebug baml-ts baml-ts-debug;
}
