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

    let

      # buildTargets = {
      #   "x86_64-linux" = {
      #     crossSystemConfig = "x86_64-unknown-linux-musl";
      #     rustTarget = "x86_64-unknown-linux-musl";
      #   };
      #   "aarch64-linux" = {
      #     crossSystemConfig = "x86_64-unknown-linux-musl";
      #     rustTarget = "x86_64-unknown-linux-musl";
      #   };
      #   "aarch64-darwin" = {};
      #   "wasm" = {
      #     crossSystemConfig = "wasm32-unknown-unknown";
      #     rustTarget = "wasm32-unknown-unknown";
      #     makeBuildPackageAttrs = pkgsCross: {
      #       OPENSSL_STATIC = null;
      #       OPENSSL_LIB_DIR = null;
      #       OPENSSL_INCLUDE_DIR = null;
      #     };
      #   };
      # };

      # mkPkgs = buildSystem: targetSystem: import nixpkgs ({
      #   system = buildSystem;
      # } // (if targetSystem == null then {} else {
      #   crossSystemcnofig = buildTargets.${targetSystem}.crossSystemConfig;
      # }));

      # eachSystem = supportedSystems: callback: builtins.fold'
      #   (overall: system: overall // { ${system} = callback system; })
      #   {}
      #   supportedSystems;

    in

    flake-utils.lib.eachDefaultSystem (system:

      let
        pkgs = nixpkgs.legacyPackages.${system};
        clang = pkgs.llvmPackages_19.clang;
        pythonEnv = pkgs.python3.withPackages (ps: []);

        toolchain = with fenix.packages.${system}; combine [
          minimal.cargo
          minimal.rustc
          latest.rust-std
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

      in
        {
          packages.default = rustPlatform.buildRustPackage {
            pname = "baml-cli";
            version = version;
            src = let
              extraFiles = pkgs.copyPathToStore ./engine/baml-runtime/src/cli/initial_project/baml_src;
            in pkgs.symlinkJoin {
              name = "source";
              paths = [ ./engine extraFiles ];
            };
            LIBCLANG_PATH = pkgs.libclang.lib + "/lib/";
            BINDGEN_EXTRA_CLANG_ARGS = if pkgs.stdenv.isDarwin then
              "-I${pkgs.llvmPackages_19.libclang.lib}/lib/clang/19/headers "
            else
              "-isystem ${pkgs.llvmPackages_19.libclang.lib}/lib/clang/19/include -isystem ${pkgs.glibc.dev}/include";

            cargoLock = { lockFile = ./engine/Cargo.lock; outputHashes = {
              "pyo3-asyncio-0.21.0" = "sha256-5ZLzWkxp3e2u0B4+/JJTwO9SYKhtmBpMBiyIsTCW5Zw=";
              "serde_magnus-0.9.0" = "sha256-+iIHleftJ+Yl9QHEBVI91NOhBw9qtUZfgooHKoyY1w4=";
            }; };

            # Add build-time environment variables
            RUSTFLAGS = "-C target-feature=+crt-static --cfg tracing_unstable";

            # Modify the test phase to only run library tests
            checkPhase = ''
              runHook preCheck
              echo "Running cargo test --lib"
              cargo test --lib
              runHook postCheck
            '';

            buildInputs = (with pkgs; [
              openssl
              pkg-config
              lld_19
              pythonEnv
              ruby
              maturin
              nodePackages.pnpm
              nodePackages.nodejs
              uv
            ]) ++ (if pkgs.stdenv.isDarwin then appleDeps else []);
            nativeBuildInputs = [
              pkgs.openssl
              pkgs.pkg-config
              pkgs.ruby
              pythonEnv
              pkgs.maturin
            ];
            PYTHON_SYS_EXECUTABLE="${pythonEnv}/bin/python3";
            LD_LIBRARY_PATH="${pythonEnv}/lib";
            PYTHONPATH="${pythonEnv}/${pythonEnv.sitePackages}";
            CC="${clang}/bin/clang";

          };
          devShell = pkgs.mkShell rec {
            buildInputs = with pkgs; [toolchain uv];
            PATH="${clang}/bin:$PATH";
            LIBCLANG_PATH = pkgs.libclang.lib + "/lib/";
          };
        }
    );
}
