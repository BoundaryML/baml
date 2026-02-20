# Build wasm-bindgen-cli from source at the exact version matching Cargo.lock.
#
# The nixpkgs wasm-bindgen-cli (0.2.100) doesn't match the workspace's
# wasm-bindgen crate (=0.2.105). wasm-bindgen requires an exact version match
# between the CLI and the crate, so we build from source.
#
# To update: change `version`, then run `nix build .#baml-schema-wasm`.
# Nix will error twice with the correct hashes — update `hash` first, then `cargoHash`.
{ pkgs }:

pkgs.rustPlatform.buildRustPackage rec {
  pname = "wasm-bindgen-cli";
  version = "0.2.105";

  src = pkgs.fetchCrate {
    inherit pname version;
    hash = "sha256-zLPFFgnqAWq5R2KkaTGAYqVQswfBEYm9x3OPjx8DJRY=";
  };

  cargoHash = "sha256-a2X9bzwnMWNt0fTf30qAiJ4noal/ET1jEtf5fBFj5OU=";

  nativeBuildInputs = [ pkgs.pkg-config ];

  buildInputs = [ pkgs.openssl ]
    ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
      pkgs.curl
    ];

  doCheck = false;

  meta = with pkgs.lib; {
    description = "CLI tool for wasm-bindgen (pinned to match workspace Cargo.lock)";
    homepage = "https://rustwasm.github.io/wasm-bindgen/";
    license = with licenses; [ mit asl20 ];
  };
}
