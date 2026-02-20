# BAML Python bindings (wheel via maturin + pip-installable package)
{
  bamlRustPackage,
  pkgs,
  pythonEnv,
  version,
}:

let
  pyLib = bamlRustPackage {
    pname = "baml-cli";
    buildType = "release";
    nativeBuildInputsExtra = [
      pkgs.maturin
      pythonEnv
    ];
    buildPhase = ''
      # Unset conflicting environment variable for macOS SDK
      echo "Unsetting DEVELOPER_DIR_FOR_TARGET"
      unset DEVELOPER_DIR_FOR_TARGET

      cargo build $CARGO_RELEASE_FLAG
      cd language_client_python
      maturin build --offline $CARGO_RELEASE_FLAG --target-dir ../target --interpreter ${pythonEnv}/bin/python3
    '';
    installPhase = ''
      mkdir -p $out/lib
      ls ../target/wheels
      wheel_path=$(find ../target/wheels -maxdepth 1 -type f -name 'baml_py-*.whl' | head -n1)
      if [ -z "$wheel_path" ]; then
        echo "No wheel produced by maturin build" >&2
        exit 1
      fi
      # Preserve the actual wheel filename with platform tags
      cp "$wheel_path" "$out/lib/"
      echo "$wheel_path" > $out/wheel-name.txt
    '';
  };

  pyLibDebug = bamlRustPackage {
    pname = "baml-cli";
    buildType = "debug";
    nativeBuildInputsExtra = [
      pkgs.maturin
      pythonEnv
    ];
    buildPhase = ''
      # Unset conflicting environment variable for macOS SDK
      echo "Unsetting DEVELOPER_DIR_FOR_TARGET"
      unset DEVELOPER_DIR_FOR_TARGET

      cargo build
      cd language_client_python
      maturin build --offline --target-dir ../target --interpreter ${pythonEnv}/bin/python3
    '';
    installPhase = ''
      mkdir -p $out/lib
      ls ../target/wheels
      wheel_path=$(find ../target/wheels -maxdepth 1 -type f -name 'baml_py-*.whl' | head -n1)
      if [ -z "$wheel_path" ]; then
        echo "No wheel produced by maturin build" >&2
        exit 1
      fi
      # Preserve the actual wheel filename with platform tags
      cp "$wheel_path" "$out/lib/"
      echo "$wheel_path" > $out/wheel-name.txt
    '';
  };

  baml-py = pkgs.python3Packages.buildPythonPackage {
    pname = "baml-py";
    inherit version;
    format = "wheel";

    # Find the actual wheel file with platform tags
    src =
      let
        wheelDir = "${pyLib}/lib";
        wheelFile = builtins.head (builtins.attrNames (builtins.readDir wheelDir));
      in
      "${wheelDir}/${wheelFile}";

    propagatedBuildInputs = with pkgs.python3.pkgs; [
      pydantic
      typing-extensions
    ];

    pythonImportsCheck = [ "baml_py" ];
    doCheck = false;

    meta = with pkgs.lib; {
      description = "Python bindings for BAML";
      homepage = "https://github.com/boundaryml/baml";
      license = licenses.mit;
      platforms = platforms.unix;
    };
  };

in
{
  inherit pyLib pyLibDebug baml-py;
}
