# Development shell — extracted as-is from current flake.nix
{
  pkgs,
  pkgs-unstable,
  buildInputs,
  pythonEnv,
}:

let
  clang = pkgs.llvmPackages.clang;
in
pkgs.mkShell rec {
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

  # Prevent SDK conflicts on macOS and configure CGO for Go
  shellHook = pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
    unset DEVELOPER_DIR_FOR_TARGET
    # Use native macOS SDK for Go instead of Nix SDK to avoid version mismatch
    # The Nix SDK (11.3) is too old for some Go packages that require macOS 12+ APIs
    if [ -d "/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk" ]; then
      export SDKROOT="/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk"
    elif [ -d "/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk" ]; then
      export SDKROOT="/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk"
    fi
    export CGO_ENABLED=1
    export CGO_LDFLAGS="-isysroot $SDKROOT"
  '';
}
