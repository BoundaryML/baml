{
  lib,
  craneLib,
  commonArgs,
  version,
}:

let
  args = commonArgs // {
    pname = "baml";
    inherit version;

    cargoExtraArgs = "--locked -p baml --bin baml --features no-self-update";
  };

  cargoArtifacts = craneLib.buildDepsOnly args;
in
craneLib.buildPackage (
  args
  // {
    inherit cargoArtifacts;
    doCheck = false;

    meta = {
      description = "BAML toolchain manager";
      longDescription = ''
        The rustup-style BAML wrapper. Self-update is disabled because Nix owns
        the wrapper, while manifest resolution and toolchain management under
        BAML_HOME remain available at runtime.
      '';
      homepage = "https://github.com/BoundaryML/baml";
      license = lib.licenses.asl20;
      mainProgram = "baml";
      platforms = lib.platforms.linux ++ lib.platforms.darwin;
    };
  }
)
