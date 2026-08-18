{
  lib,
  packages,
  pkgs,
}:

{
  inherit (packages) baml baml-cli;

  baml-smoke =
    pkgs.runCommand "baml-wrapper-smoke"
      {
        nativeBuildInputs = [
          packages.baml
        ]
        ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.file ];
      }
      ''
        export HOME="$TMPDIR/home"
        export BAML_HOME="$TMPDIR/baml-home"
        export BAML_CACHE_DIR="$TMPDIR/baml-cache"
        mkdir -p "$HOME" "$BAML_HOME" "$BAML_CACHE_DIR"

        baml --version | tee wrapper-version.txt
        grep -F "baml wrapper ${packages.baml.version}" wrapper-version.txt
        baml toolchain list

        ${lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
          file "${packages.baml}/bin/baml" | grep -E "static(-pie|ally)? linked"
        ''}

        if baml self-update >self-update.txt 2>&1; then
          echo "baml self-update unexpectedly succeeded" >&2
          exit 1
        fi
        grep -F "self-update is disabled in this build" self-update.txt

        touch "$out"
      '';

  baml-cli-smoke =
    pkgs.runCommand "baml-cli-smoke"
      {
        nativeBuildInputs = [
          packages.baml-cli
        ]
        ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.file ];
      }
      ''
        export HOME="$TMPDIR/home"
        export BAML_HOME="$TMPDIR/baml-home"
        export BAML_CACHE_DIR="$TMPDIR/baml-cache"
        export BAML_CLI_ALLOW_DIRECT=1
        export BAML_TELEMETRY_DISABLED=1
        mkdir -p "$HOME" "$BAML_HOME" "$BAML_CACHE_DIR"

        baml-cli --version | tee cli-version.txt
        grep -F "baml-cli ${packages.baml-cli.version}" cli-version.txt
        baml-cli --help >/dev/null

        ${lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
          file "${packages.baml-cli}/bin/baml-cli" | grep -E "static(-pie|ally)? linked"
          file "${packages.baml-cli}/bin/baml-pack-host" | grep -E "static(-pie|ally)? linked"
        ''}

        project="$TMPDIR/project"
        mkdir -p "$project/baml_src"
        cat >"$project/baml.toml" <<'EOF'
        [package]
        name = "nix-smoke"

        [generator.typescript]
        output_type = "typescript/node"
        output_dir = "generated"
        naming_convention = "preserve-case"
        EOF
        cat >"$project/baml_src/main.baml" <<'EOF'
        function add(a: int, b: int) -> int {
          a + b
        }
        EOF

        baml-cli check --project "$project"
        baml-cli generate --project "$project"
        test -d "$project/generated"

        baml-cli pack add --project "$project" --output "$TMPDIR/add"
        ${lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
          file "$TMPDIR/add" | grep -E "static(-pie|ally)? linked"
        ''}
        "$TMPDIR/add" --help >/dev/null

        touch "$out"
      '';
}
