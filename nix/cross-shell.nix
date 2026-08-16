# Linux -> Darwin/Windows cross-compile shell for CI (`nix develop .#cross`).
#
# Darwin: zig cc is the C compiler/linker with a pinned macOS SDK sysroot
# (cargo-zigbuild); maturin has first-class `--zig`, napi-rs cross-compiles
# with zig the same way. Windows MSVC: cargo-xwin (fetches the MSVC CRT +
# Windows SDK; point XWIN_CACHE_DIR at persistent disk).
#
# CI entrypoint: `cross-build <target> [cargo args...]` - it owns the builder
# choice and every per-target flag quirk, so workflows carry none of them.
#
# `pkgs` must carry the rust-overlay overlay; the toolchain pin matches the
# repo-root rust-toolchain.toml (channel 1.93.0) plus every triple the cross
# lane emits.
{
  pkgs,
  # Repackaged SDK (same pin the ix/index cross lane uses); zig cc consumes it
  # as -isysroot for both the C compile and the Rust link. Apple licenses the
  # SDK for use on Apple hardware, so a caller wanting a stricter posture
  # supplies its own:
  #   import ./nix/cross-shell.nix {inherit pkgs; macosSdk = <your sdk>;}
  macosSdk ?
    let
      tarball = pkgs.fetchurl {
        url = "https://github.com/joseluisq/macosx-sdks/releases/download/15.4/MacOSX15.4.sdk.tar.xz";
        hash = "sha256-oLe2aRKsDaDkWzBKMyus2+WMoXIiCCDUJe2yghOWL4E=";
      };
    in
      pkgs.runCommand "MacOSX15.4.sdk" {} ''
        mkdir -p "$out"
        tar xf ${tarball} --strip-components=1 -C "$out"
      '',
}: let
  rustToolchain = pkgs.rust-bin.stable."1.93.0".default.override {
    extensions = ["rustfmt" "clippy" "rust-src" "llvm-tools-preview"];
    targets = [
      "wasm32-unknown-unknown"
      "x86_64-unknown-linux-gnu"
      "x86_64-unknown-linux-musl"
      "aarch64-unknown-linux-gnu"
      "aarch64-unknown-linux-musl"
      "aarch64-apple-darwin"
      "x86_64-apple-darwin"
      "x86_64-pc-windows-msvc"
      "aarch64-pc-windows-msvc"
    ];
  };

  crossBuild = pkgs.writeShellApplication {
    name = "cross-build";
    text = ''
      target=$1
      shift
      case "$target" in
        *-apple-darwin)
          # cargo-zigbuild 0.20.1 passes ROOT-relative framework paths
          # expecting zig to join them onto --sysroot; zig does not, and the
          # darwin link dies "unable to locate framework ... searched paths:
          # <empty>" after a full compile. Inject the ABSOLUTE SDK paths.
          tgt=$(echo "$target" | tr 'a-z-' 'A-Z_')
          exec env "CARGO_TARGET_''${tgt}_RUSTFLAGS=-C link-arg=-F$SDKROOT/System/Library/Frameworks -C link-arg=-L$SDKROOT/usr/lib" \
            cargo zigbuild --release --bin baml-cli --target "$target" "$@"
          ;;
        *-pc-windows-msvc)
          # Plain CFLAGS is the only user-flag channel cargo-xwin honors (it
          # overwrites CFLAGS_<target>); the clang builtin include dir
          # supplies stdalign.h & friends the MSVC CRT lacks. Never set
          # CFLAGS shell-wide: it would poison the zig legs (zig ships its
          # own clang headers).
          export CFLAGS="-I$CLANG_BUILTIN_INCLUDE"
          # Cold-start VMs pulling the CRT/SDK concurrently have hit
          # "timeout: global" from Microsoft's CDN; be persistent.
          export XWIN_HTTP_RETRIES="''${XWIN_HTTP_RETRIES:-8}"
          case "$target" in
            # blake3's NEON C compiles through a GNU-mode driver that reads
            # clang-cl-style /imsvc flags as filenames; the plain-clang cross
            # compiler keeps every sub-build's flag dialect consistent.
            aarch64-*) export XWIN_CROSS_COMPILER=clang ;;
            *) export XWIN_CROSS_COMPILER=clang-cl ;;
          esac
          exec cargo xwin build --release --bin baml-cli --target "$target" "$@"
          ;;
        *)
          echo "cross-build: unsupported target $target" >&2
          exit 2
          ;;
      esac
    '';
  };
in
  pkgs.mkShell {
    packages = [
      rustToolchain
      crossBuild
      # zig 0.14, NOT default 0.15: cargo-zigbuild 0.20.1 passes --sysroot
      # unconditionally (zig.rs:396) and zig 0.15 resolves framework paths
      # relative to it - the darwin link fails after a full compile (observed
      # live). Upstream cargo-zigbuild main handles 0.15; until that lands in
      # the pin, 0.14 is the proven pairing.
      pkgs.zig_0_14
      pkgs.cargo-xwin
      pkgs.cargo-zigbuild
      pkgs.pkg-config
      pkgs.file
      # cargo-xwin drives the MSVC targets through clang-cl + lld-link +
      # llvm-lib (ring's build.rs probes for clang-cl explicitly).
      pkgs.llvmPackages.clang-unwrapped
      pkgs.llvmPackages.lld
      pkgs.llvmPackages.llvm
    ];
    env.SDKROOT = "${macosSdk}";
    # Clang BUILTIN headers (stdalign.h etc - the MSVC CRT does not ship
    # them); consumed per-leg by cross-build, never exported shell-wide.
    env.CLANG_BUILTIN_INCLUDE = "${pkgs.llvmPackages.clang-unwrapped.lib}/lib/clang/${pkgs.lib.versions.major pkgs.llvmPackages.clang-unwrapped.version}/include";
  }
