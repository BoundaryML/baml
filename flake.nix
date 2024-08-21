{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
    nixpkgs.url = "nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  
  outputs = { self, nixpkgs, flake-utils , rust-overlay, naersk }:
  flake-utils.lib.eachDefaultSystem (system:
    let
      overlays = [ rust-overlay.overlays.default ];
      pkgs = import nixpkgs { inherit overlays system; };
      rust = (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml).override {
        targets = [ "wasm32-unknown-unknown" ];
        extensions = ["rust-src"];
      };
      apple = pkgs.darwin.apple_sdk.frameworks;
      apple-deps = [ apple.AudioUnit apple.CoreAudio apple.CoreFoundation apple.CoreServices apple.SystemConfiguration apple.Security apple.DiskArbitration apple.Foundation apple.AppKit apple.Cocoa ];
      linux-deps = [
          pkgs.udev pkgs.alsaLib pkgs.vulkan-loader
          pkgs.xorg.libX11 pkgs.xorg.libXcursor pkgs.xorg.libXi
          pkgs.xorg.libXrandr pkgs.libxkbcommon pkgs.wayland

      ];

      buildInputs = [
          pkgs.wasm-bindgen-cli
          pkgs.wasm-pack
          pkgs.which
          rust
          pkgs.rust-analyzer
          pkgs.curl
          pkgs.pnpm
          pkgs.autoconf
          pkgs.pkg-config
          pkgs.openssl
          pkgs.binaryen
          ] ++ (if system == "aarch64-darwin" then apple-deps else linux-deps);

      naersk' = pkgs.callPackage naersk {};

    in
    {

      defaultPackage = pkgs.rustPlatform.buildRustPackage {
        src = ./.;
        name = "baml";

        checkPhase = "echo 'Skipping tests'";

        nativeBuildInputs = buildInputs;
        buildInputs = buildInputs;
        PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
        COREAUDIO_SDK_PATH= if system == "aarch64-darwin" then "${pkgs.darwin.apple_sdk.MacOSX-SDK}" else "";
      };

      # packages.wasm-bindgen-cli = wasm-bindgen-cli;

      packages.wasm-build = pkgs.rustPlatform.buildRustPackage {

        src = ./.;
        name = "baml-wasm";

        buildPhase = ''
          HOME=$(mktemp -d fake-homeXXXX) RUSTFLAGS="--cfg=web_sys_unstable_apis" wasm-pack build --mode no-install --release --target web
        '';
        checkPhase = "echo 'Skipping tests'";
        installPhase = ''
          mkdir -p $out
          cp pkg/* $out/
        '';

        buildInputs = buildInputs;
        nativeBuildInputs = buildInputs;
        PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
        COREAUDIO_SDK_PATH= if system == "aarch64-darwin" then "${pkgs.darwin.apple_sdk.MacOSX-SDK}" else "";
        VERGEN_GIT_SHA=self.sourceInfo.lastModifiedDate;
      };


      devShell = pkgs.mkShell rec {
        # buildInputs = buildInputs;
        buildInputs = [
          # wasm-bindgen-cli
          rust
          pkgs.rust-analyzer
          pkgs.autoconf
          pkgs.wasm-bindgen-cli
          pkgs.pkg-config
          pkgs.openssl
          pkgs.sass
          pkgs.binaryen
          pkgs.wasm-pack
          pkgs.pnpm
          ] ++ (if system == "aarch64-darwin" then apple-deps else linux-deps);

        PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
        COREAUDIO_SDK_PATH= if system == "aarch64-darwin" then "${pkgs.darwin.apple_sdk.MacOSX-SDK}" else "";
        RUST_SRC_PATH = "${rust}/lib/rustlib/src/rust/library";
      };
    }
  );
}
