# BAML VS Code extension (.vsix package)
#
# Builds the baml-extension VSIX which bundles:
#   - The language server extension (tsup-compiled)
#   - The playground webview (Vite-built fiddle-frontend)
#   - The baml-cli binary
#
# To update pnpmDeps hash: same as fiddle-frontend.
{
  pkgs,
  pkgs-unstable,
  version,
  src,
  pnpmDeps,
  baml-schema-wasm,
  baml-cli,
  codemirror-lang-baml,
}:

pkgs.stdenvNoCC.mkDerivation {
  pname = "baml-vsix";
  inherit version src;

  nativeBuildInputs = [
    pkgs.pnpm_9.configHook
    pkgs-unstable.nodejs_20
    pkgs.vsce
  ];

  inherit pnpmDeps;

  preBuild = ''
    # Place WASM output where vite expects it
    mkdir -p engine/baml-schema-wasm/web/dist
    cp -r ${baml-schema-wasm}/pkg/* engine/baml-schema-wasm/web/dist/

    # Place pre-built codemirror-lang-baml
    mkdir -p typescript/packages/codemirror-lang-baml/dist
    cp -r ${codemirror-lang-baml}/* typescript/packages/codemirror-lang-baml/dist/
  '';

  buildPhase = ''
    runHook preBuild

    # Build @baml/playground and all its transitive workspace dependencies
    pnpm --filter "...@baml/playground" build

    # Place playground dist into vscode-ext/dist/playground
    mkdir -p typescript/apps/vscode-ext/dist/playground
    cp -r typescript/apps/playground/dist/* typescript/apps/vscode-ext/dist/playground/

    # Place CLI binary
    mkdir -p typescript/apps/vscode-ext/dist
    cp ${baml-cli}/bin/baml-cli typescript/apps/vscode-ext/dist/baml-cli

    # Build the extension with tsup
    cd typescript/apps/vscode-ext
    CI=true pnpm run build

    # Package as VSIX
    vsce package --no-dependencies

    runHook postBuild
  '';

  installPhase = ''
    mkdir -p $out
    cp *.vsix $out/
  '';

  meta = with pkgs.lib; {
    description = "BAML VS Code extension (.vsix)";
    homepage = "https://github.com/boundaryml/baml";
    license = licenses.mit;
  };
}
