# The toolchain surface BAML's CI jobs consume, as one dev shell per Rust
# toolchain (devShells.ci pins rust-toolchain.toml, devShells.ci-msrv pins the
# workspace MSRV). Jobs on the ix runner pool enter a shell here instead of
# installing toolchains imperatively (rustup show + mise); the Blacksmith
# fork-PR fallback keeps the imperative path because those images have no
# nix. The seam is .github/actions/setup-ci-shell.
#
# Multi-toolchain jobs never mix rustup and this shell: the recorded breakage
# (.envrc, "the flake devshell's cargo shadowing rustup's shim breaks `cargo
# +1.91.1`") is why each toolchain gets its own shell attribute and the
# converted jobs drop `+toolchain` syntax entirely.
{
  pkgs,
  pkgs-unstable,
  toolchain,
  protocGenGo,
}:
let
  # Prints the shell's environment in GITHUB_ENV `KEY=value` form. A job runs
  #   BAML_CI_BASE_PATH="$PATH" nix develop .#ci -c ci-env >> "$GITHUB_ENV"
  # once, and every later step sees the shell's tools as plain commands. The
  # list is curated: exporting the whole builder env would clobber job-level
  # vars the runner unit owns (HOME, TMPDIR, NEXTEST_TEST_THREADS). The base
  # PATH is appended so runner-provided tools (git-lfs, rustup for the
  # unconverted jobs) stay reachable.
  ciEnv = pkgs.writeShellScriptBin "ci-env" ''
    printf 'PATH=%s\n' "$PATH''${BAML_CI_BASE_PATH:+:$BAML_CI_BASE_PATH}"
    for var in LIBCLANG_PATH BINDGEN_EXTRA_CLANG_ARGS; do
      printf '%s=%s\n' "$var" "''${!var}"
    done
    # openssl-sys binaries link a nix-store libssl with no rpath, so link
    # view and loader view must agree or nextest dies at --list with
    # "libssl.so.3: cannot open shared object file" (proven live, PR #7
    # round 2). Point the build at the shell's openssl and give the loader
    # the same directory; it holds only libssl/libcrypto, and either link
    # source (shell or runner-unit openssl) resolves against it (ABI 3.x).
    printf 'OPENSSL_LIB_DIR=%s\n' "${pkgs.lib.getLib pkgs.openssl}/lib"
    printf 'OPENSSL_INCLUDE_DIR=%s\n' "${pkgs.openssl.dev}/include"
    printf 'PKG_CONFIG_PATH=%s\n' "${pkgs.openssl.dev}/lib/pkgconfig"
    printf 'LD_LIBRARY_PATH=%s\n' "${pkgs.lib.getLib pkgs.openssl}/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  '';
  # setup-musl-cross probes for a `musl-gcc` that can link static-PIE
  # binaries; same wrapper the runner image carries in nix/ci-runner.nix.
  muslGcc = pkgs.writeShellScriptBin "musl-gcc" ''
    exec ${pkgs.pkgsCross.musl64.stdenv.cc}/bin/x86_64-unknown-linux-musl-gcc "$@"
  '';
in
pkgs.mkShell {
  packages = [
    toolchain
    ciEnv
    muslGcc

    # Tools the converted jobs invoke directly; the fallback arm installs the
    # same set through setup-mise's install_args.
    pkgs.cargo-nextest
    pkgs.cargo-insta
    pkgs.sccache
    pkgs.direnv
    pkgs.wasm-pack

    # Native build deps of the baml_language workspace.
    pkgs.cmake
    pkgs.ninja
    pkgs.pkg-config
    pkgs.openssl
    pkgs.perl
    pkgs.gcc
    pkgs.lld
    pkgs.go # sdkgen_go's build script shells out to gofmt
    pkgs.protobuf
    protocGenGo

    # snapshot-tests fixtures run python through uv.
    pkgs.python313
    pkgs-unstable.uv
  ];

  env = {
    LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
    # Same derivation as the flake devshell: clang's resource dir is named by
    # MAJOR version only since LLVM 16.
    BINDGEN_EXTRA_CLANG_ARGS = "-isystem ${pkgs.llvmPackages.libclang.lib}/lib/clang/${pkgs.lib.versions.major pkgs.llvmPackages.libclang.version}/include -isystem ${pkgs.llvmPackages.libclang.lib}/include -isystem ${pkgs.glibc.dev}/include";
  };
}
