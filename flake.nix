{
  description = "baml: The programming language for agents";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    crane.url = "github:ipetkov/crane";

    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{
      crane,
      flake-parts,
      nixpkgs,
      rust-overlay,
      ...
    }:
    let
      inherit (nixpkgs) lib;

      muslCrossPackageSets = {
        x86_64-linux = "musl64";
        aarch64-linux = "aarch64-multiplatform-musl";
      };
      supportedSystems = builtins.attrNames muslCrossPackageSets ++ [ "aarch64-darwin" ];

      rustToolchainSpec = builtins.fromTOML (builtins.readFile ./rust-toolchain.toml);
    in
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = supportedSystems;

      perSystem =
        { pkgs, system, ... }:
        let
          useMuslCross = pkgs.stdenv.hostPlatform.isLinux;

          # The wrapper's build target determines which managed toolchains it downloads.
          packagePkgs = if useMuslCross then pkgs.pkgsCross.${muslCrossPackageSets.${system}} else pkgs;
          rustTarget = packagePkgs.stdenv.hostPlatform.rust.rustcTarget;

          configuredTargets = rustToolchainSpec.toolchain.targets;
          rustBin = rust-overlay.lib.mkRustBin { } pkgs;
          baseToolchain = rustBin.fromRustupToolchainFile ./rust-toolchain.toml;
          rustToolchain =
            if !useMuslCross || lib.elem rustTarget configuredTargets then
              baseToolchain
            else
              baseToolchain.override { targets = configuredTargets ++ [ rustTarget ]; };

          craneLib = (crane.mkLib packagePkgs).overrideToolchain (_: rustToolchain);

          # callPackage splices CMake from the native build platform.
          packageDefinitions = packagePkgs.callPackage ./packaging/nix/packages {
            inherit craneLib;
          };
          # Exclude callPackage's override helpers from flake package outputs.
          bamlPackages = lib.getAttrs [
            "baml"
            "baml-cli"
            "default"
          ] packageDefinitions;
        in
        {
          packages = bamlPackages;

          checks = import ./packaging/nix/checks.nix {
            inherit lib;
            packages = bamlPackages;
            inherit pkgs;
          };
        };
    };
}
