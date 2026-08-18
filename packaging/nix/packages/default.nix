{
  cmake,
  craneLib,
  lib,
}:

let
  repoRoot = ../../..;

  src = lib.fileset.toSource {
    root = repoRoot;
    fileset = lib.fileset.unions [
      (repoRoot + "/baml_language")
      (repoRoot + "/release/platforms.json")
    ];
  };

  languageRelease = builtins.fromTOML (builtins.readFile (repoRoot + "/baml_language/release.toml"));
  languageVersion = languageRelease.release.canary_version;
  canonicalVersionLine = lib.findFirst (lib.hasPrefix "pub const CANONICAL_VERSION") null (
    lib.splitString "\n" (
      builtins.readFile (repoRoot + "/baml_language/crates/baml_version/src/lib.rs")
    )
  );
  canonicalVersionMatch =
    if canonicalVersionLine == null then
      null
    else
      builtins.match ''pub const CANONICAL_VERSION: &str = "([^"]+)";'' canonicalVersionLine;
  canonicalVersion =
    if canonicalVersionMatch == null then
      throw "Unable to read BAML CANONICAL_VERSION"
    else
      builtins.head canonicalVersionMatch;

  wrapperManifest = builtins.fromTOML (
    builtins.readFile (repoRoot + "/baml_language/crates/baml/Cargo.toml")
  );

  commonArgs = {
    inherit src;
    strictDeps = true;
    doCheck = false;

    cargoLock = repoRoot + "/baml_language/Cargo.lock";
    cargoToml = repoRoot + "/baml_language/Cargo.toml";

    nativeBuildInputs = [ cmake ];

    postUnpack = ''
      cd "$sourceRoot/baml_language"
      sourceRoot="."
    '';
  };

  baml = import ./baml.nix {
    inherit
      commonArgs
      craneLib
      lib
      ;
    version = wrapperManifest.package.version;
  };

  bamlCli =
    assert languageVersion == canonicalVersion;
    import ./baml-cli.nix {
      inherit
        commonArgs
        craneLib
        lib
        ;
      version = languageVersion;
    };
in
{
  inherit baml;
  baml-cli = bamlCli;
  default = baml;
}
