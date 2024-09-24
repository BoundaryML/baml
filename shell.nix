let
  pkgs = import <nixpkgs> {};
in
  pkgs.mkShell {
    buildInputs = with pkgs; [
      cargo
      rustc
      nodePackages.pnpm
      python3
      poetry
      rust-analyzer
    ];
    shellHook = ''
      export PROJECT_ROOT=/$(pwd)
      export PATH=/$PROJECT_ROOT/tools:$PATH
    '';
  }
