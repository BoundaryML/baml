let
  pkgs = import <nixpkgs> {};

  nodeEnv = pkgs.callPackage <nixpkgs/pkgs/development/node-packages/node-env.nix> {};
  fern = nodeEnv.buildNodePackage rec {
    name = "fern-api";
    packageName = "fern-api";
    version = "0.42.8";
    src = pkgs.fetchurl {
      url = "https://registry.npmjs.org/fern-api/-/fern-api-${version}.tgz";
      sha256 = "sha256-jaKXJsvgjRPpm2ojB6a2hkEAmk7NrfcTA28MLl3VjHg=";
    };
    dependencies = [];
  };

in
  pkgs.mkShell {
    buildInputs = with pkgs; [
      cargo
      rustc
      nodePackages.pnpm
      python3
      poetry
      rust-analyzer
      fern
    ];
    shellHook = ''
      export PROJECT_ROOT=/$(pwd)
      export PATH=/$PROJECT_ROOT/tools:$PATH
    '';
  }
