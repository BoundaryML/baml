{
  lib,
  craneLib,
  commonArgs,
  version,
}:

let
  args = commonArgs // {
    pname = "baml-cli";
    inherit version;

    # Build the fixed CLI and its same-target pack host together, matching the
    # upstream toolchain release. Keeping the selection identical in both Crane
    # phases prevents Cargo from rebuilding missing workspace dependencies.
    cargoExtraArgs = "--locked -p baml_cli -p baml_pack_host --bins";
  };

  cargoArtifacts = craneLib.buildDepsOnly args;
in
craneLib.buildPackage (
  args
  // {
    inherit cargoArtifacts;
    doCheck = false;

    meta = {
      description = "Fixed BAML compiler, runtime, and SDK generator toolchain";
      longDescription = ''
        The Nix-managed BAML language toolchain for reproducible CI, containers,
        and direct use. It includes the same-target baml-pack-host so native
        baml pack operations do not need to download a release artifact.
      '';
      homepage = "https://github.com/BoundaryML/baml";
      license = lib.licenses.asl20;
      mainProgram = "baml-cli";
      platforms = lib.platforms.linux ++ lib.platforms.darwin;
    };
  }
)
