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

in
{
  inherit commonArgs buildInputs nativeBuildInputs appleDeps;
}
