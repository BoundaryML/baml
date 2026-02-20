# BAML CLI binary packages (release + debug)
{
  craneLib,
  commonArgs,
  cargoArtifacts,
  cargoArtifactsDebug,
  nativeBuildInputs,
  pythonEnv,
  pkgs,
}:

let
  bamlRustPackage =
    {
      pname,
      buildPhase ? null,
      installPhase ? null,
      nativeBuildInputsExtra ? [ ],
      buildType,
      extraAttrs ? { },
    }:
    let
      cargoProfileDir = if buildType == "release" then "release" else "debug";
      releaseFlag = if buildType == "release" then "--release" else "";
      buildFn = craneLib.buildPackage;

      # Unset DEVELOPER_DIR_FOR_TARGET on macOS to avoid SDK conflicts
      preBuildWrapper = pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
        unset DEVELOPER_DIR_FOR_TARGET
      '';
    in
    buildFn (
      {
        inherit pname;
        cargoArtifacts = if buildType == "release" then cargoArtifacts else cargoArtifactsDebug;
        inherit (commonArgs) src version buildInputs;
        inherit (commonArgs) LIBCLANG_PATH BINDGEN_EXTRA_CLANG_ARGS RUSTFLAGS;
        inherit (commonArgs)
          OPENSSL_STATIC
          OPENSSL_DIR
          OPENSSL_LIB_DIR
          OPENSSL_INCLUDE_DIR
          ;
        inherit (commonArgs) PROTOC_GEN_GO_PATH SKIP_BAML_VALIDATION;

        CARGO_PROFILE_DIR = cargoProfileDir;
        CARGO_RELEASE_FLAG = releaseFlag;
        CARGO_BUILD_RUSTFLAGS = commonArgs.RUSTFLAGS;

        nativeBuildInputs = nativeBuildInputs ++ nativeBuildInputsExtra;
        doCheck = false;

        CARGO_PROFILE = if buildType == "release" then "release" else "dev";

        preBuild = preBuildWrapper + (extraAttrs.preBuild or "");
      }
      // (if buildPhase != null then { inherit buildPhase; } else { })
      // (if installPhase != null then { inherit installPhase; } else { })
      // extraAttrs
    );

  cliInstallPhase = { strip ? true }: ''
    runHook preInstall
    build_root=''${CARGO_TARGET_DIR:-target}
    profile_dir="$build_root/$CARGO_PROFILE_DIR"
    echo "Listing baml binaries under $profile_dir:"
    find "$profile_dir" -maxdepth 1 -type f -name "baml*" || true
    mkdir -p $out/bin
    BINARY_NAME="$profile_dir/baml-cli"
    if [ ! -x "$BINARY_NAME" ]; then
      echo "Unable to locate the compiled CLI binary at $BINARY_NAME" >&2
      exit 1
    fi
    echo "Found binary: $BINARY_NAME"
    cp "$BINARY_NAME" $out/bin/baml-cli
    ${if strip then "strip $out/bin/baml-cli 2>/dev/null || true" else ""}
    runHook postInstall
  '';

  pythonExtras = {
    PYTHON_SYS_EXECUTABLE = "${pythonEnv}/bin/python3";
    LD_LIBRARY_PATH = "${pythonEnv}/lib";
    PYTHONPATH = "${pythonEnv}/${pythonEnv.sitePackages}";
  };
in
{
  release = bamlRustPackage {
    pname = "baml-cli";
    buildType = "release";
    installPhase = cliInstallPhase { strip = true; };
    extraAttrs = pythonExtras;
  };

  debug = bamlRustPackage {
    pname = "baml-cli";
    buildType = "debug";
    installPhase = cliInstallPhase { strip = false; };
    extraAttrs = pythonExtras;
  };

  # Re-export for use by other packages (baml-py, baml-ts)
  inherit bamlRustPackage;
}
