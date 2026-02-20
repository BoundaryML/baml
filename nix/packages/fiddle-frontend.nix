# Fiddle frontend — Next.js standalone web app
#
# Builds @baml/fiddle-web-app, the standalone browser-based BAML playground.
# Run with: `nix build .#fiddle-frontend && ./result/bin/fiddle-frontend`
#
# Pre-built dependencies (WASM, codemirror-lang-baml) are injected into the
# workspace. next build handles the rest (including fetching Google Fonts).
#
# To update pnpmDeps hash: change any pnpm-lock.yaml dependency, then run
# `nix build .#fiddle-frontend`. Nix will error with the correct hash.
{
  pkgs,
  pkgs-unstable,
  version,
  src,
  pnpmDeps,
  baml-schema-wasm,
  codemirror-lang-baml,
}:

let
  nodejs = pkgs-unstable.nodejs_20;

in
pkgs.stdenvNoCC.mkDerivation {
  pname = "fiddle-frontend";
  inherit version src;

  nativeBuildInputs = [
    pkgs.pnpm_9.configHook
    nodejs
    pkgs.makeWrapper
    pkgs.cacert
  ];

  inherit pnpmDeps;

  # CA certs for next build to fetch Google Fonts; telemetry off.
  SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
  NODE_EXTRA_CA_CERTS = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
  NEXT_TELEMETRY_DISABLED = "1";

  preBuild = ''
    # Place pre-built WASM output where playground-common expects it
    mkdir -p engine/baml-schema-wasm/web/dist
    cp -r ${baml-schema-wasm}/pkg/* engine/baml-schema-wasm/web/dist/

    # Place pre-built codemirror-lang-baml dist
    mkdir -p typescript/packages/codemirror-lang-baml/dist
    cp -r ${codemirror-lang-baml}/* typescript/packages/codemirror-lang-baml/dist/
  '';

  buildPhase = ''
    runHook preBuild

    # Build remaining transitive workspace deps (playground-common),
    # then the fiddle-web-app itself.
    # codemirror-lang-baml is already pre-built above, so skip it.
    pnpm --filter @baml/playground-common build
    pnpm --filter @baml/fiddle-web-app build

    runHook postBuild
  '';

  installPhase = ''
    appDir=typescript/apps/fiddle-web-app

    mkdir -p $out/app
    cp -r $appDir/.next $out/app/.next
    cp $appDir/package.json $out/app/
    cp -r $appDir/public $out/app/public 2>/dev/null || true

    # Copy node_modules (needed by next start).
    cp -r node_modules $out/app/node_modules

    # Remove dangling workspace symlinks.  pnpm hoists workspace package
    # links into node_modules/.pnpm/node_modules/ (e.g. @baml/engine →
    # ../../engine).  These resolve in the build tree but dangle in $out
    # since we only copied node_modules.  Scope the cleanup to that single
    # directory to avoid breaking .bin/ or other valid symlink chains.
    find $out/app/node_modules/.pnpm/node_modules -xtype l -delete

    # Wrapper script to run the app
    mkdir -p $out/bin
    makeWrapper ${nodejs}/bin/node $out/bin/fiddle-frontend \
      --add-flags "$out/app/node_modules/.bin/next" \
      --add-flags "start" \
      --add-flags "--port" \
      --add-flags "3000" \
      --chdir "$out/app"
  '';

  meta = with pkgs.lib; {
    description = "BAML interactive playground (Next.js web app)";
    homepage = "https://github.com/boundaryml/baml";
    license = licenses.mit;
  };
}
