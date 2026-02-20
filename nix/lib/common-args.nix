# Shared build arguments for all crane builds
# Environment variables, build inputs, and native build inputs.
{
  pkgs,
  pkgs-unstable,
  src,
  version,
  protocGenGo,
  pythonEnv,
  toolchain,
}:

let
  appleDeps = pkgs.lib.optionals pkgs.stdenv.isDarwin (
    with pkgs.darwin;
    [
      libiconv
    ]
  );

  clang = pkgs.llvmPackages.clang;

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
      vsce
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

  # toolchain is needed in buildInputs above but isn't available here as a
  # direct parameter — it gets passed through buildInputs from the caller.
  # We re-declare the subset needed for nativeBuildInputs.
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

  commonArgs = {
    inherit
      src
      version
      buildInputs
      nativeBuildInputs
      ;
    strictDeps = true;

    LIBCLANG_PATH = pkgs.libclang.lib + "/lib/";
    # LLVM 16+ uses just the major version for the resource directory
    # (e.g. lib/clang/19/include, not lib/clang/19.1.7/include).
    BINDGEN_EXTRA_CLANG_ARGS =
      let clangMajor = pkgs.lib.versions.major pkgs.llvmPackages.libclang.version; in
      if pkgs.stdenv.isDarwin then
        "-I${pkgs.llvmPackages.libclang.lib}/lib/clang/${clangMajor}/include "
      else
        "-isystem ${pkgs.llvmPackages.libclang.lib}/lib/clang/${clangMajor}/include -isystem ${pkgs.glibc.dev}/include";
    # Note: +crt-static is NOT set here — it breaks proc-macro builds
    # (e.g. askama_derive) on Linux because proc-macros are dylibs.
    # The musl build in flake.nix sets its own CARGO_BUILD_RUSTFLAGS
    # with +crt-static where it's actually needed.
    RUSTFLAGS = "--cfg tracing_unstable";
    OPENSSL_STATIC = "1";
    OPENSSL_DIR = "${pkgs.openssl.dev}";
    OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
    OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
    PROTOC_GEN_GO_PATH = "${protocGenGo}/bin/protoc-gen-go";
    SKIP_BAML_VALIDATION = "1";
  };

in
{
  inherit commonArgs buildInputs nativeBuildInputs appleDeps;
}
