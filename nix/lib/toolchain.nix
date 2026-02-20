# Fenix toolchain construction
# Reads rust-toolchain.toml and provides native + wasm toolchains.
{ fenix, system }:

let
  toolchain = fenix.packages.${system}.fromToolchainFile {
    file = ../../rust-toolchain.toml;
    sha256 = "sha256-vra6TkHITpwRyA5oBKAHSX0Mi6CBDNQD+ryPSpxFsfg=";
  };
in
{
  inherit toolchain;

  # For making a rustPlatform (used by devShell RUST_SRC_PATH)
  minimal = fenix.packages.${system}.minimal;
  latest = fenix.packages.${system}.latest;
}
