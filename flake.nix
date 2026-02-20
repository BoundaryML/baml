{
  nixConfig = {
    extra-substituters = [ "https://boundaryml.cachix.org" ];
    extra-trusted-public-keys = [
      "boundaryml.cachix.org-1:mNYg3LOiGXpNx4qkjP7cviRT6ExZTUyC5HJkXPIEJd8="
    ];
  };

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
        pythonEnv = pkgs.python3.withPackages (ps: [ ]);

        # --- Import library modules ---

        toolchainSet = import ./nix/lib/toolchain.nix { inherit fenix system; };
        toolchain = toolchainSet.toolchain;

        protocGenGo = import ./nix/lib/protoc-gen-go.nix { inherit pkgs; };

        src = import ./nix/lib/source-filter.nix { inherit pkgs; };

        version = (builtins.fromTOML (builtins.readFile ./engine/Cargo.toml)).workspace.package.version;

        commonArgsSet = import ./nix/lib/common-args.nix {
          inherit
            pkgs
            pkgs-unstable
            src
            version
            protocGenGo
            pythonEnv
            toolchain
            ;
        };
        commonArgs = commonArgsSet.commonArgs;
        buildInputs = commonArgsSet.buildInputs;
        nativeBuildInputs = commonArgsSet.nativeBuildInputs;

        # --- Crane setup ---

        craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

        # Native cargo artifacts (cached separately — ~15min cold build)
        cargoArtifacts = craneLib.buildDepsOnly (commonArgs // { CARGO_PROFILE = "release"; });
        cargoArtifactsDebug = craneLib.buildDepsOnly (commonArgs // { CARGO_PROFILE = "dev"; });

        # --- Import package modules ---

        cliPkgs = import ./nix/packages/baml-cli.nix {
          inherit
            craneLib
            commonArgs
            cargoArtifacts
            cargoArtifactsDebug
            nativeBuildInputs
            pythonEnv
            pkgs
            ;
        };

        pyPkgs = import ./nix/packages/baml-py.nix {
          inherit pkgs pythonEnv version;
          bamlRustPackage = cliPkgs.bamlRustPackage;
        };

        tsPkgs = import ./nix/packages/baml-ts.nix {
          inherit
            pkgs
            pkgs-unstable
            commonArgs
            version
            ;
          bamlRustPackage = cliPkgs.bamlRustPackage;
        };

        # wasm-bindgen-cli pinned to match Cargo.lock (nixpkgs has 0.2.100, we need 0.2.105)
        wasm-bindgen-cli = import ./nix/lib/wasm-bindgen-cli.nix { inherit pkgs; };

        wasmPkgs = import ./nix/packages/baml-wasm.nix {
          inherit
            crane
            pkgs
            toolchain
            src
            version
            wasm-bindgen-cli
            ;
        };

        # --- Shared pnpm workspace source + deps (used by JS/TS packages) ---

        pnpmSrc = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter =
            path: type:
            let
              baseName = baseNameOf path;
            in
            baseName != "node_modules"
            && baseName != "result"
            && baseName != ".jj"
            && baseName != ".git"
            && baseName != ".claude"
            && !pkgs.lib.hasInfix "/target/" path
            && !pkgs.lib.hasSuffix ".so" baseName
            && !pkgs.lib.hasSuffix ".node" baseName
            && !pkgs.lib.hasSuffix ".vsix" baseName;
        };

        # Single pnpm dependency fetch shared by all JS/TS packages
        pnpmDeps = pkgs.pnpm_9.fetchDeps {
          pname = "baml-workspace";
          inherit version;
          src = pnpmSrc;
          hash = "sha256-PjYsVup7GLEHMvUN47OYcurkAYR0xUQPAr1NNyX+oe8=";
          fetcherVersion = 2;
        };

        codemirrorLangBaml = import ./nix/packages/codemirror-lang-baml.nix {
          inherit pkgs pkgs-unstable version pnpmDeps;
          src = pnpmSrc;
        };

        fiddleFrontend = import ./nix/packages/fiddle-frontend.nix {
          inherit
            pkgs
            pkgs-unstable
            version
            pnpmDeps
            ;
          src = pnpmSrc;
          baml-schema-wasm = wasmPkgs.baml-schema-wasm;
          codemirror-lang-baml = codemirrorLangBaml;
        };

        bamlVsix = import ./nix/packages/baml-vsix.nix {
          inherit
            pkgs
            pkgs-unstable
            version
            pnpmDeps
            ;
          src = pnpmSrc;
          baml-schema-wasm = wasmPkgs.baml-schema-wasm;
          baml-cli = cliPkgs.release;
          codemirror-lang-baml = codemirrorLangBaml;
        };

      in
      {
        # --- Packages ---

        packages = {
          default = cliPkgs.release;
          baml-cli = cliPkgs.release;
          baml-cli-debug = cliPkgs.debug;

          # Musl build (Linux only)
          baml-cli-musl =
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

          # Python packages
          inherit (pyPkgs) baml-py;
          pyLib = pyPkgs.pyLib;
          baml-py-debug = pyPkgs.pyLibDebug;

          # TypeScript packages
          inherit (tsPkgs) baml-ts baml-ts-debug;
          tsLib = tsPkgs.tsLib;
          tsLib-debug = tsPkgs.tsLibDebug;

          # WASM package
          baml-schema-wasm = wasmPkgs.baml-schema-wasm;

          # Frontend / extension packages
          codemirror-lang-baml = codemirrorLangBaml;
          fiddle-frontend = fiddleFrontend;
          baml-vsix = bamlVsix;
        };

        # --- Checks (intermediates exposed for Cachix caching) ---

        checks = {
          inherit cargoArtifacts cargoArtifactsDebug;
          cargoArtifacts-wasm = wasmPkgs.cargoArtifacts;
        };

        # --- Dev shell ---

        devShell = import ./nix/dev/shell.nix {
          inherit
            pkgs
            pkgs-unstable
            buildInputs
            pythonEnv
            ;
        };
      }
    );
}
