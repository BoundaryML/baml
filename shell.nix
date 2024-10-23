# TODO: Package jest

let
  pkgs = import <nixpkgs> { };

  nodeEnv =
    pkgs.callPackage <nixpkgs/pkgs/development/node-packages/node-env.nix> { };
  fern = nodeEnv.buildNodePackage rec {
    name = "fern-api";
    packageName = "fern-api";
    version = "0.42.8";
    src = pkgs.fetchurl {
      url = "https://registry.npmjs.org/fern-api/-/fern-api-${version}.tgz";
      sha256 = "sha256-jaKXJsvgjRPpm2ojB6a2hkEAmk7NrfcTA28MLl3VjHg=";
    };
    dependencies = [ ];
  };

  appleDeps = with pkgs.darwin.apple_sdk.frameworks; [
    CoreServices
    SystemConfiguration
    pkgs.libiconv-darwin
  ];

in pkgs.mkShell {

  buildInputs = with pkgs;
    [
      cargo
      cargo-watch
      rustc
      rustfmt
      maturin
      nodePackages.pnpm
      nodePackages.nodejs
      python3
      poetry
      rust-analyzer
      fern
      ruby
      nixfmt-classic
      swc
      lld_18
      wasm-pack
    ] ++ (if pkgs.stdenv.isDarwin then appleDeps else [ ]);

  LIBCLANG_PATH = pkgs.libclang.lib + "/lib/";
  BINDGEN_EXTRA_CLANG_ARGS = if pkgs.stdenv.isDarwin then
    "-I${pkgs.llvmPackages_18.libclang.lib}/lib/clang/18/headers "
  else
    "-isystem ${pkgs.llvmPackages_18.libclang.lib}/lib/clang/18/include -isystem ${pkgs.glibc.dev}/include";

  shellHook = ''
    export PROJECT_ROOT=/$(pwd)
    export PATH=/$PROJECT_ROOT/tools:$PROJECT_ROOT/integ-tests/typescript/node_modules/.bin:$PATH
  '';
}
