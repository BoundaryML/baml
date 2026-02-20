# @baml/codemirror-lang-baml — Lezer grammar + tsup build
#
# Built separately so fiddle-frontend and baml-vsix can depend on
# the pre-built dist/ without worrying about build ordering.
{
  pkgs,
  pkgs-unstable,
  version,
  src,
  pnpmDeps,
}:

let
  nodejs = pkgs-unstable.nodejs_20;

in
pkgs.stdenvNoCC.mkDerivation {
  pname = "codemirror-lang-baml";
  inherit version src;

  nativeBuildInputs = [
    pkgs.pnpm_9.configHook
    nodejs
  ];

  inherit pnpmDeps;

  buildPhase = ''
    runHook preBuild
    pnpm --filter @baml/codemirror-lang-baml build
    runHook postBuild
  '';

  installPhase = ''
    mkdir -p $out
    cp -r typescript/packages/codemirror-lang-baml/dist/* $out/
  '';

  meta = with pkgs.lib; {
    description = "BAML language support for CodeMirror (Lezer grammar)";
    homepage = "https://github.com/boundaryml/baml";
    license = licenses.mit;
  };
}
