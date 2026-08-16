{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/7df7ff7d8e00218376575f0acdcc5d66741351ee";
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # Pins the exact rust-toolchain.toml channel with extra target triples for
    # the Linux -> Darwin/Windows cross lane (devShells.cross).
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs-unstable";
    };
    # Fresh nixpkgs for the CI runner VMs only (the repo's own nixpkgs pins
    # are untouched): the GitHub Actions runner package must stay current or
    # registered runners are refused. Deliberately not `follows
    # nixpkgs-unstable` despite the same URL - this one has to track fresh
    # nixos-unstable on its own, while the repo's other pins stay stable.
    nixpkgs-ci.url = "github:NixOS/nixpkgs/nixos-unstable";
    # ix-maintained runner mechanism; nix/ci-runner.nix here is only policy.
    # Main-rev pin; bump deliberately (the reconcile workflow pins the SAME
    # rev in its `uses:` - move both together).
    # MAIN REVS ONLY: a branch pin reverts every fix main has that the
    # branch lacks (2026-08-16: an app-auth branch pin time-traveled past
    # the region fix and recreated pool members in the wrong region).
    ix-runners.url = "github:indexable-inc/ix-runners/275f844b869476bde794f3d691ebd946f20a890d";
    # `lib.cargoUnitFor`, the per-rustc-unit cargo derivation graph behind
    # packages.msrv-check.
    #
    # The repo is public and this path is reachable anonymously - measured,
    # with `access-tokens` cleared and internal inputs 404ing as positive
    # controls. That matters more than it sounds: pool guests hold no GitHub
    # credential for nix fetches at all, so every guest fetch of this input
    # is anonymous, on canary exactly as much as on a fork PR. Thirteen of
    # index's inputs have `internal` visibility; none is on this path, which
    # is why the rev is PINNED rather than tracking main - a future input
    # change must not be able to silently break fork PRs.
    #
    # This rev (mirror publish of 2026-08-16, carrying forge ea527d85eabe)
    # is the first with the whole-workspace nextest metadata export the
    # musl and gnu lanes run their prebuilt test binaries through.
    # Anonymous evaluability re-verified at THIS rev, same protocol as the
    # original pin (access-tokens cleared, internal inputs 404ing as
    # positive controls). Bumping past it requires the same two rituals:
    # re-verify anonymous eval, and re-read the policy schema (see the
    # l2Policy note below for the trap this pin already stepped around).
    index.url = "github:indexable-inc/index/b8652df400e424c02a24f233d52bd8bdcbffdf80";
    crane = {
      url = "github:ipetkov/crane";
    };
    flake-compat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      nixpkgs-unstable,
      flake-utils,
      fenix,
      crane,
      rust-overlay,
      nixpkgs-ci,
      ix-runners,
      index,
      ...
    }:

    flake-utils.lib.eachDefaultSystem (
      system:

      let
        pkgs = nixpkgs.legacyPackages.${system};
        pkgs-unstable = nixpkgs-unstable.legacyPackages.${system};
        clang = pkgs.llvmPackages.clang;
        pythonEnv = pkgs.python3.withPackages (ps: [ ]);

        toolchain = fenix.packages.${system}.fromToolchainFile {
          file = ./rust-toolchain.toml;
          sha256 = "sha256-vra6TkHITpwRyA5oBKAHSX0Mi6CBDNQD+ryPSpxFsfg=";
        };

        version = (builtins.fromTOML (builtins.readFile ./engine/Cargo.toml)).workspace.package.version;

        appleDeps = pkgs.lib.optionals pkgs.stdenv.isDarwin (
          with pkgs.darwin;
          [
            libiconv
          ]
        );

        rustPlatform = pkgs.makeRustPlatform {
          inherit (fenix.packages.${system}.minimal) cargo rustc;
          inherit (fenix.packages.${system}.latest) rust-std;
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

        # Parameterized over nixpkgs because the msrv unit graph builds
        # against index's nixpkgs, not this repo's pin (see idxPkgs), and a
        # unit graph must not mix two nixpkgs' native inputs.
        mkProtocGenGo =
          p:
          p.buildGoModule rec {
            pname = "protoc-gen-go";
            version = "1.34.1";

            src = p.fetchFromGitHub {
              owner = "protocolbuffers";
              repo = "protobuf-go";
              rev = "v${version}";
              hash = "sha256-xbfqN/t6q5dFpg1CkxwxAQkUs8obfckMDqytYzuDwF4=";
            };

            vendorHash = "sha256-nGI/Bd6eMEoY0sBwWEtyhFowHVvwLKjbT4yfzFz6Z3E=";

            subPackages = [ "cmd/protoc-gen-go" ];

            meta = with p.lib; {
              description = "Go support for Google's protocol buffers";
              mainProgram = "protoc-gen-go";
              homepage = "https://google.golang.org/protobuf";
              license = licenses.bsd3;
              maintainers = with maintainers; [ jojosch ];
            };
          };
        protocGenGo = mkProtocGenGo pkgs;

        # The rust-overlay view of nixpkgs, and the one place the MSRV is
        # read. devShells.ci-msrv and packages.msrv-check both take the
        # toolchain from here, so the shell, the unit graph, and the
        # cargo-build-msrv gate cannot drift from the workspace manifest.
        rustOverlayPkgs = import nixpkgs-unstable {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        msrv =
          (builtins.fromTOML (builtins.readFile ./baml_language/Cargo.toml)).workspace.package.rust-version;
        # Closure curve for msrv-check, all measured 2026-08-16 on one box by
        # one method, because every number here is profile qualified and the
        # bare figure has already misled once:
        #
        #     4.58 GiB  --release            (what this graph used to build,
        #                                     and semantically the WRONG arm -
        #                                     see `profile` below)
        #    93.29 GiB  dev, debuginfo on    (correct arm, unshippable: a
        #                                     cache HIT is a substitution)
        #    10.23 GiB  dev, debug=0         (what it builds today)
        #
        # The residual 2.2x over release is not waste to chase here: release
        # was opt-level="s" + fat LTO + strip="symbols", and this graph is
        # deliberately none of those, because the cargo arm it has to match is
        # none of those. The lever that does cut further is the TODO below.
        #
        # TODO(l2): give the unit graph `rust-bin.stable.${msrv}.minimal` and
        # leave devShells.ci-msrv on `.default`. Of the 10.23 GiB,
        # rust-default-1.91.1 accounts for 1,750 MiB - rust-docs 576 MiB,
        # clippy-preview 443 MiB, rustfmt-preview 432 MiB. Those component
        # figures are profile independent and did not move across any row of
        # the curve above. No guest needs any of those three; they ride in
        # through baml_type_macros, because a proc-macro is a dylib rustc
        # loads and so keeps the whole toolchain in its runtime closure.
        # Cutting them is roughly 14% off what every guest substitutes - it
        # was worth 31% against the smaller release closure, so the case got
        # weaker even as the closure got bigger. Deferred rather than done
        # because it changes the toolchain id, so it is a full graph rebuild
        # to validate, and it splits the graph's toolchain from the shell's -
        # the MSRV version stays single-source either way, only the component
        # set differs.
        msrvToolchain = rustOverlayPkgs.rust-bin.stable.${msrv}.default;

        # L2: the cargo-build-msrv lane's work as a per-rustc-unit derivation
        # graph. `nix-cargo-unit` transcribes `cargo --unit-graph` into one
        # derivation per rustc invocation, keyed by a recursive hash over
        # package identity, features, profile, mode, sorted dependency
        # hashes, and the toolchain id. Source is sliced per crate, so a PR
        # editing one of the workspace's 90 members perturbs that crate's
        # units and its reverse-deps only; everything else substitutes.
        #
        # The msrv lane is the v1 cut because it is build-only
        # (`cargo test --no-run`), so ~100% of it is compile, and because its
        # 1.91.1 toolchain is unique to it: nothing else in CI warms its
        # compile cache, which is why it is the worst lane on the board.
        # The graph is instantiated against INDEX's nixpkgs, not this repo's
        # pin, and that is a cost decision rather than a correctness one -
        # both were measured working anonymously. `cargoUnitFor` builds the
        # renderer against whichever pkgs it is handed: index's nixpkgs makes
        # it a cache.ix.dev hit (eval ~6 s), this repo's pin makes it a
        # derivation no cache has ever seen (~49 s of source build, paid by
        # every cold guest at EVALUATION time, before one compile unit is
        # considered). The price is index's nixpkgs in baml's lock, and it
        # buys a single nixpkgs for the whole unit graph, so native inputs
        # below come from here too rather than straddling two.
        idxPkgs = import index.inputs.nixpkgs { inherit system; };
        cargoUnit = index.lib.cargoUnitFor idxPkgs;

        # The one policy every graph here uses: pureBuild (no clippy/audit/
        # machete units - BAML's lint gates are their own jobs) plus the one
        # override that keeps stable toolchains compiling.
        #
        # compiler.embedMetadata defaults FALSE at this pin and renders
        # `-Zembed-metadata=no` into every unit. That flag is nightly-only,
        # and all four graphs pin stable rust-overlay toolchains, so the
        # default kills all 5000+ units with "the option `Z` is only
        # accepted on the nightly compiler". The machinery's own guard reads
        # `rustToolchain.ixRustChannel`, which rust-overlay toolchains do
        # not carry, and a null channel is an accept arm - the guard
        # structurally cannot fire for an external consumer, so the failure
        # would land as thousands of broken compiles instead of one eval
        # error. index hit the same wall on its own stable graphs
        # (ENG-12992).
        #
        # recursiveUpdate, not `//`: the preset is a partial policy resolved
        # through evalModules. Today it carries no `compiler` key, so `//`
        # would work - until the preset grows one, at which point `//`
        # silently clobbers every sibling under it. Merge deep, always.
        l2Policy = nixpkgs.lib.recursiveUpdate cargoUnit.policyPresets.pureBuild {
          compiler.embedMetadata = true;
        };

        msrvWorkspace = cargoUnit.buildWorkspace {
          src = ./baml_language;
          workspaceRoot = ./baml_language;
          rustToolchain = msrvToolchain;

          # Not a preference. cache.ix.dev is atticd behind ncps, which
          # serves narinfos and 404s /realisations; a floating-CA output has
          # no eval-time path, so substituting one needs exactly that build
          # trace. CA units would be unsubstitutable through the only cache
          # the pool guests can read. Costs early cutoff, buys substitution.
          contentAddressed = false;

          # pureBuild + the stable-toolchain embedMetadata override; the full
          # trap writeup lives on l2Policy above. The old HAZARD note here
          # predicted exactly this: the pin moved past the commit that grew
          # the option, and the override is now mandatory, not optional.
          policy = l2Policy;

          # Plan the same cargo execution the lane runs. Feature unification
          # is then reproduced by construction instead of modeled, which
          # matters here: --all-features turns on both aws-crypto and
          # ring-crypto, and native-tls is the sole reason openssl enters
          # the graph at all (no crate depends on it directly).
          cargoTargets = [
            [
              "--workspace"
              "--all-features"
              "--tests"
            ]
          ];
          cargoTargetNames = [ "msrv" ];

          # The two arms must compile the SAME program, and the machinery
          # defaults to release (`profile = rawArgs.profile or "release"`,
          # index lib/rust/cargo-unit.nix:397) while the cargo arm this
          # replaces runs a bare `cargo test --no-run`, i.e. dev + test.
          # Left at the default the arms differ on `debug_assertions`, so
          # every `#[cfg(debug_assertions)]` block is uncompiled on the nix
          # arm: an error inside one passes the lane on a cache hit and
          # fails it on a miss, which is the worst shape a gate can have.
          # `"dev"` renders no profile flag at all, which is cargo's own
          # default and therefore exactly what the cargo arm does.
          profile = "dev";

          env = {
            # baml_language/.cargo/config.toml sets this, and cargo-unit parses
            # only rustflags out of cargo config -- its doc comment says the
            # [env] table is explicitly not honored. Without it the in-process
            # compiler tests stack-overflow.
            RUST_MIN_STACK = "67108864";

            # The other half of arm equivalence. cargo-tests.reusable.yaml
            # sets both of these workflow-wide (lines 68-69), so the cargo
            # arm's dev and test profiles are opt-level 1, not cargo's
            # default 0. `env` folds into the planner IFD's environment
            # (cargo-unit.nix:448), so cargo resolves the override while
            # emitting `--unit-graph` and the renderer reads the opt level
            # off the graph's profile fields -- the units carry it without
            # this flake having to model a profile table.
            CARGO_PROFILE_DEV_OPT_LEVEL = "1";
            CARGO_PROFILE_TEST_OPT_LEVEL = "1";

            # Debuginfo is the one dev-profile default this graph does NOT
            # want, and dropping it is not a reprise of the bug above.
            # `debug_assertions` and `debug` are independent knobs: the first
            # decides which code exists and so has to match the cargo arm, the
            # second only decorates the artifact. Debuginfo cannot hide a
            # compile error and cannot create one, and this lane is
            # `--no-run`, so nothing here is ever executed, unwound or
            # debugged -- the binaries exist to prove they link and are then
            # thrown away.
            #
            # Measured, and the reason this is not a micro-optimisation: with
            # dev's default debuginfo the closure is 93.29 GiB; at debug=0 it
            # is 10.23 GiB. A cache HIT makes every guest substitute that, so
            # the 83 GiB is the difference between L2 being worth taking on
            # this lane and being slower than just running cargo.
            #
            # Do NOT copy this to the gnu/musl lanes when they convert. Those
            # RUN their test binaries, so debug=0 there costs backtrace
            # quality on real failures - a genuine trade-off, not free.
            CARGO_PROFILE_DEV_DEBUG = "0";
            CARGO_PROFILE_TEST_DEBUG = "0";
          };

          nativeBuildInputs = graphNativeBuildInputs;
          packageBuildEnv = graphPackageBuildEnv;
        };

        # Tools the workspace's build scripts shell out to. These fold into
        # every unit, which is right for toolchain constants and is anyway
        # the only hatch: the machinery has no per-package build-inputs
        # table, only per-package env and rustc args.
        #
        # Shared by every graph below. They are the same set per graph on
        # purpose: which build scripts run is a property of the package
        # selection, and a tool that no selected build script invokes costs
        # only its store path, never a compile.
        graphNativeBuildInputs = [
          idxPkgs.cmake
          idxPkgs.ninja
          idxPkgs.pkg-config
          idxPkgs.perl
          idxPkgs.go # sdkgen_go's build script shells out to gofmt
          idxPkgs.protobuf
          (mkProtocGenGo idxPkgs)
          # openssl is here for the LINK, not for openssl-sys' build
          # script (that one reads OPENSSL_LIB_DIR below). Units link
          # independently, so the `rustc-link-search` openssl-sys emits
          # does not reach the dependent binaries the way it does under
          # one cargo invocation, and every test binary that pulls
          # native-tls dies "mold: fatal: library not found: ssl".
          # Putting it here lets the stdenv wrapper add -L to every link.
          # It costs nothing in closure terms on the msrv graph, whose
          # --all-features already drags openssl in.
          idxPkgs.openssl
        ];

        # Values, unlike tools, are scoped per package. `env` folds into
        # every unit, so exporting the openssl and libclang variables
        # workspace-wide would make every unit in the graph depend on
        # openssl -- wrong, and expensive. index learned this the costly
        # way (ENG-10488).
        #
        # Keys are checked against Cargo.lock, not against the graph's
        # package selection, so one table serves every graph: an entry for a
        # package a given graph does not build is inert, and dropping
        # entries per graph would only invite the silent-no-op class the
        # machinery's default-deny exists to prevent.
        graphPackageBuildEnv = {
          openssl-sys = {
            OPENSSL_LIB_DIR = "${idxPkgs.lib.getLib idxPkgs.openssl}/lib";
            OPENSSL_INCLUDE_DIR = "${idxPkgs.openssl.dev}/include";
            OPENSSL_NO_VENDOR = "1";
          };
          # bindgen consumers: aws-lc-sys is pulled in by --all-features'
          # aws-crypto, bridge_cffi runs bindgen in its own build script.
          aws-lc-sys = graphBindgenEnv;
          bridge_cffi = graphBindgenEnv;
          # sdkgen_cpp gzips committed protobuf headers that live in the
          # sibling bridge_cpp crate, which its own slice does not contain.
          sdkgen_cpp.BAML_BRIDGE_CPP_PB_DIR =
            "${./baml_language/sdks/cpp/bridge_cpp/pb/baml_bridge/cffi/v1}";
          # A machinery gap, not a BAML one: cargo defines
          # CARGO_TARGET_TMPDIR at compile time for integration-test and
          # bench targets, and cargo-unit does not, so baml_cli's
          # tests/common/mod.rs fails to compile on `env!`. A literal is
          # all the compile needs; running those tests under nix would
          # additionally need this path writable.
          baml_cli.CARGO_TARGET_TMPDIR = "/tmp/baml-cli-target-tmp";
        }
        # The msrv lane is `--workspace` and there are no default-members, so
        # it builds the whole sdk_tests tree, whose nine generators all read a
        # fixture corpus that sits above every one of them. The generators
        # are pure Rust (no foreign toolchain at build time), so handing
        # them the corpus is all they need.
        // sdkTestFixtureEnv;

        # The shared sdk_tests fixture corpus, handed to each generator that
        # reads it. Scoped per package rather than put in workspace-wide
        # `env` so the corpus is an input of nine units, not of all 5,247.
        sdkTestFixtureEnv =
          let
            fixtures = "${./baml_language/sdk_tests/fixtures}";
          in
          nixpkgs.lib.genAttrs [
            "sdk_test_cpp"
            "sdk_test_csharp"
            "sdk_test_go"
            "sdk_test_java"
            "sdk_test_python_pydantic2"
            "sdk_test_rust"
            "sdk_test_swift"
            "sdk_test_typescript"
            "sdk_test_typescript_web"
          ] (_: { BAML_SDK_TEST_FIXTURES = fixtures; })
          // {
            # This one also reads the canonical TypeScript sources out of its
            # sibling sdk_test_typescript crate.
            sdk_test_typescript_web = {
              BAML_SDK_TEST_FIXTURES = fixtures;
              BAML_SDK_TEST_TYPESCRIPT_SOURCES = "${./baml_language/sdk_tests/crates/typescript}";
            };
          };

        # Same shape as nix/ci-shell.nix's, but resolved against the unit
        # graph's own nixpkgs: clang's resource dir is named by MAJOR version
        # only since LLVM 16.
        graphBindgenEnv = {
          LIBCLANG_PATH = "${idxPkgs.libclang.lib}/lib";
          BINDGEN_EXTRA_CLANG_ARGS = "-isystem ${idxPkgs.llvmPackages.libclang.lib}/lib/clang/${idxPkgs.lib.versions.major idxPkgs.llvmPackages.libclang.version}/include -isystem ${idxPkgs.llvmPackages.libclang.lib}/include -isystem ${idxPkgs.glibc.dev}/include";
        };

        # ------------------------------------------------------------------
        # L2, the wasm lane
        # ------------------------------------------------------------------
        #
        # Which other Linux rust lanes are NOT here, and why. Each of these
        # was attempted against the pinned index rev and hit a specific
        # missing capability, not a difficulty:
        #
        #   cargo-test-linux (gnu) and cargo-test-linux-musl. These lanes
        #     COMPILE and then RUN. The graph can produce the test binaries,
        #     but nothing can hand them to nextest: the machinery synthesizes
        #     a `--binaries-metadata` JSON per test target inside a throwaway
        #     single-package workspace in $TMPDIR and never installs it, so
        #     there is no whole-workspace metadata file to reach from the
        #     flake. Converting only the compile half is not neutral, it is a
        #     regression: `cargo nextest run` with an empty target/ recompiles
        #     everything the nix arm just built, so the job pays for both.
        #     Running the tests inside the derivations instead would work
        #     mechanically -- the packages these two lanes select carry no
        #     nextest setup scripts and no serialization groups -- but it
        #     makes the two arms semantically different, and the arm is
        #     chosen by cache state. A test that passes in the runner
        #     environment and fails in the nix sandbox (or the reverse) would
        #     then make the verdict depend on whether the builder had caught
        #     up, which is not a property a merge gate may have. Compile-only
        #     lanes are immune to this, which is why msrv and wasm are the
        #     two that converted. The unblocker is a whole-workspace
        #     binaries-metadata export, upstream in the machinery.
        #
        #   cargo-doc. The machinery has no rustdoc-HTML path at this pin:
        #     rustdoc is invoked only as `rustdoc --test` for doctests, there
        #     is no `--no-deps` or doc-output install, and RUSTDOCFLAGS
        #     appears nowhere in the tree -- so `RUSTDOCFLAGS=-D warnings`,
        #     which is the entire point of the lane, has nowhere to go. The
        #     available workaround, running cargo doc against `vendorDir` in
        #     one coarse derivation, is a whole-workspace blob that any
        #     source edit invalidates. That is worse than sccache, not
        #     better, because the PR delta is the whole mechanism.
        #
        #   size-gate linux and wasm. Blocked in the size-gate tool, not in
        #     nix: `cargo size-gate check` shells out to `cargo build
        #     --release -p <pkg>` itself at run time and then executes the
        #     CLI it just built. A release graph would produce those binaries
        #     cheaply -- `profile` is a plain argument -- but the tool has no
        #     way to be handed a prebuilt artifact. The unblocker is a
        #     `--prebuilt` hatch in crates/tools_size_gate/src/measure.rs,
        #     which is a change to that tool and not to this file.

        # The cargo-test-wasm lane despite its name runs no tests: it is
        # `cargo build --target wasm32-unknown-unknown --release`. That makes
        # it the second clean conversion after msrv, and the only lane in the
        # record with a measured build-step decomposition (107-120 s of a
        # 120-136 s cold job, n=3) -- so almost all of it is compile, and
        # almost all of that is substitutable.
        #
        # It is a second `buildWorkspace` rather than a second cargoTargets
        # entry because target triple, profile and toolchain are all folded
        # into every unit hash. Nothing here shares a single unit with the
        # msrv graph, by construction.

        # The lane discovers its package set at run time from cargo metadata:
        # every workspace member whose `[package.metadata.ci] wasm_support`
        # is absent or true. Reproduced here because the graph needs the
        # selection at EVALUATION time, and reproduced as a predicate over
        # the real manifests rather than a copied list so a new crate is
        # picked up the same way the lane picks it up.
        #
        # The one thing this cannot track is a change to the `members` globs
        # themselves. That is what packages.wasm-package-list and the job's
        # drift check are for: a mismatch routes the job to the cargo arm
        # with a warning instead of silently building a different set.
        wasmPackages =
          let
            root = ./baml_language;
            # Cargo's members list, transcribed. Globs are expanded against
            # the tree so `crates/*` stays self-maintaining.
            globbed =
              rel:
              let
                entries = builtins.readDir (root + "/${rel}");
              in
              map (name: "${rel}/${name}") (
                builtins.filter (
                  name: entries.${name} == "directory" && builtins.pathExists (root + "/${rel}/${name}/Cargo.toml")
                ) (builtins.attrNames entries)
              );
            explicitMembers = [
              "sdks/cpp/sdkgen_cpp"
              "sdks/csharp/sdkgen_csharp"
              "sdks/go/sdkgen_go"
              "sdks/java/bridge_java"
              "sdks/java/sdkgen_java"
              "sdks/typescript/bridge_typescript"
              "sdks/typescript/bridge_typescript_web"
              "sdks/typescript/sdkgen_typescript_shared"
              "sdk_tests/harness_setup"
              "sdk_tests/harness_runner"
              "sdk_tests/harness/llm_recordings"
              "sdk_tests/crates/java"
              "sdk_tests/crates/cpp"
              "sdk_tests/crates/csharp"
              "sdk_tests/crates/python_pydantic2"
              "sdk_tests/crates/swift"
              "sdk_tests/crates/go"
              "sdk_tests/crates/typescript"
              "sdk_tests/crates/typescript_web"
              "sdk_tests/crates/rust"
              "tools_sccache"
            ];
            # [workspace].exclude: directories the globs would otherwise
            # sweep up but that are not members.
            notMembers = [
              "forks/aws-config-systest"
              "forks/google-cloud-auth-systest"
              "sdks/rust/verify"
            ];
            memberDirs = builtins.filter (dir: !(builtins.elem dir notMembers)) (
              globbed "crates"
              ++ globbed "forks"
              ++ globbed "sdks/python/rust"
              ++ globbed "sdks/swift/rust"
              ++ globbed "sdks/rust"
              ++ explicitMembers
            );
            manifestOf = dir: builtins.fromTOML (builtins.readFile (root + "/${dir}/Cargo.toml"));
            # Absent means supported: the lane's jq predicate defaults the
            # flag to true, so only an explicit `false` opts a crate out.
            supportsWasm = manifest: manifest.package.metadata.ci.wasm_support or true;
          in
          nixpkgs.lib.sort (a: b: a < b) (
            map (manifest: manifest.package.name) (builtins.filter supportsWasm (map manifestOf memberDirs))
          );

        # Read from the same rust-toolchain.toml devShells.ci pins, so the
        # graph and the shell cannot name different compilers.
        #
        # `.minimal` rather than `.default`: this is the cut the msrv TODO
        # above describes, taken here because a new graph costs nothing to
        # start lean. minimal is rustc + cargo + rust-std, which is
        # everything a unit graph invokes -- cargo only for the planning IFD,
        # rustc for every unit -- while `.default` would add rust-docs,
        # clippy and rustfmt to what every guest substitutes. policy
        # pureBuild already means no clippy unit exists to want the
        # component.
        ciRustChannel = (builtins.fromTOML (builtins.readFile ./rust-toolchain.toml)).toolchain.channel;
        wasmToolchain = rustOverlayPkgs.rust-bin.stable.${ciRustChannel}.minimal.override {
          targets = [ "wasm32-unknown-unknown" ];
        };

        wasmWorkspace = cargoUnit.buildWorkspace {
          src = ./baml_language;
          workspaceRoot = ./baml_language;
          rustToolchain = wasmToolchain;

          # Same reason as the msrv graph: cache.ix.dev 404s /realisations,
          # so a floating-CA output is unsubstitutable through the only cache
          # the pool guests can read.
          contentAddressed = false;
          # pureBuild + the stable-toolchain embedMetadata override (see
          # l2Policy above for the trap).
          policy = l2Policy;

          # The triple is a first-class argument, not a flag inside
          # cargoTargets: the machinery threads it into the planner AND into
          # its nextest wiring, and a per-entry --target would key those two
          # differently. One graph per triple is the machinery's own rule.
          target = "wasm32-unknown-unknown";

          # The lane builds --release, and this graph must agree with it
          # exactly: profile fields (opt-level "s", lto "fat",
          # codegen-units 1, panic "abort", strip "symbols") are all folded
          # into the unit hash. Stated explicitly even though "release" is
          # the machinery's default, because the default is the wrong one to
          # inherit silently -- see the note on the msrv graph.
          profile = "release";

          cargoTargets = [
            (
              [ "--no-default-features" ]
              ++ builtins.concatMap (name: [
                "-p"
                name
              ]) wasmPackages
            )
          ];
          cargoTargetNames = [ "wasm" ];

          # baml_language/.cargo/config.toml carries
          #   [target.wasm32-unknown-unknown]
          #   rustflags = [ "--cfg", 'getrandom_backend="wasm_js"' ]
          # and cargo applies it to every wasm32 unit. cargo-unit drives
          # rustc directly and so honors cargo config only when asked; left
          # off, this graph compiles getrandom (and anything else reading
          # that cfg) differently from the lane it stands in for -- two arms
          # of one job building two different programs. Measured on the
          # first build of this graph: the emitted rustc argv carried no
          # --cfg at all.
          #
          # The flag is per-triple, so it is inert on host units (build
          # scripts, proc-macros): there is no [target.x86_64-unknown-linux
          # -gnu] section, which is also why the msrv graph does not set it.
          cargoConfigRustflags = true;

          # baml_language/.cargo/config.toml sets this and cargo-unit does
          # not read the [env] table. It is not the in-process compiler
          # tests that need it here -- this graph builds no tests -- but
          # proc-macro expansion runs on the host under the same setting the
          # lane runs under, and matching the lane is the whole point.
          env.RUST_MIN_STACK = "67108864";

          nativeBuildInputs = graphNativeBuildInputs;
          packageBuildEnv = graphPackageBuildEnv;
        };

        # ------------------------------------------------------------------
        # L2, the musl lane
        # ------------------------------------------------------------------
        #
        # The first lane that RUNS what it builds. The graph produces the
        # test binaries; the job hands them to cargo-nextest via the
        # machinery's whole-workspace `nextestExport` (binaries-metadata +
        # cargo-metadata whose interpolated store paths pin every test
        # binary into one substitutable closure), and nextest executes them
        # in the real checkout with --workspace-remap. The tests run in the
        # same runner environment on both arms, so arm equivalence holds the
        # same way it does for the compile-only lanes: the only thing a
        # cache hit changes is who compiled.
        #
        # Same toolchain file as devShells.ci and the wasm graph, with the
        # musl target added: baml_language/rust-toolchain.toml lists only
        # wasm32, so the target is added here exactly the way the job's
        # no-nix fallback does with `rustup target add`.
        muslToolchain = rustOverlayPkgs.rust-bin.stable.${ciRustChannel}.minimal.override {
          targets = [ "x86_64-unknown-linux-musl" ];
        };

        # The full musl cross gcc, NOT nixpkgs' thin `musl-gcc` libc wrapper:
        # the wrapper links broken static-PIE binaries. Same fix, same
        # reasoning as nix/ci-shell.nix's musl-gcc and the pool image's --
        # this one just resolves against the graph's own nixpkgs.
        muslCc = idxPkgs.pkgsCross.musl64.stdenv.cc;

        muslWorkspace = cargoUnit.buildWorkspace {
          src = ./baml_language;
          workspaceRoot = ./baml_language;
          rustToolchain = muslToolchain;

          # Same reason as the msrv graph: cache.ix.dev 404s /realisations,
          # so a floating-CA output is unsubstitutable through the only cache
          # the pool guests can read.
          contentAddressed = false;
          # pureBuild + the stable-toolchain embedMetadata override (see
          # l2Policy above for the trap).
          policy = l2Policy;

          # One graph per triple is the machinery's own rule -- the triple
          # is a first-class argument, same as the wasm graph.
          target = "x86_64-unknown-linux-musl";

          # The lane runs bare `cargo test --no-run` (dev + test); stated
          # explicitly for the same debug_assertions reason as the msrv
          # graph.
          profile = "dev";

          # The lane's exact selection: NO --all-features, deliberately --
          # every feature turns on the optional native-tls backend, which
          # needs a target-specific OpenSSL build; default features match
          # the rustls-based musl artifacts shipped (the workflow says the
          # same above its build step). Exclusions mirror the job's cargo
          # line verbatim.
          cargoTargets = [
            [
              "--workspace"
              "--tests"
              "--exclude"
              "baml_tests"
              "--exclude"
              "sdk_test_*"
              "--exclude"
              "baml_bridge"
            ]
          ];
          cargoTargetNames = [ "musl" ];

          env = {
            # Same two reasons as the msrv graph: cargo-unit does not read
            # cargo config's [env] table, and the workflow pins opt-level 1
            # on both profiles workflow-wide.
            RUST_MIN_STACK = "67108864";
            CARGO_PROFILE_DEV_OPT_LEVEL = "1";
            CARGO_PROFILE_TEST_OPT_LEVEL = "1";

            # NO debug=0 here, per the msrv graph's own warning: this lane
            # RUNS its binaries, so dropping debuginfo costs backtrace
            # quality on real failures. The closure consequence is real
            # (msrv measured 93.29 GiB at full dev debuginfo vs 10.23 GiB at
            # debug=0) and is why nix/l2-roots.txt demands measuring this
            # graph's closure BEFORE the builder picks it up. If it blows
            # the builder's 30 GiB refusal, the mitigation is
            # `line-tables-only` set on BOTH arms in the same commit (the
            # workflow env and here), never on one.

            # The linker seam the job wires via setup-musl-cross: without
            # it, the musl target links with the default glibc gcc and dies
            # on `-ldl` (musl folds libdl into libc).
            CC_x86_64_unknown_linux_musl = "${muslCc}/bin/x86_64-unknown-linux-musl-gcc";
            CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = "${muslCc}/bin/x86_64-unknown-linux-musl-gcc";
          };

          # Scoped envs for packages this graph excludes are inert, and one
          # table serving every graph is the point (see the msrv note).
          nativeBuildInputs = graphNativeBuildInputs;
          packageBuildEnv = graphPackageBuildEnv;
        };

        # ------------------------------------------------------------------
        # L2, the gnu lane
        # ------------------------------------------------------------------
        #
        # Same shape as the musl graph, host triple. The lane's selection is
        # broader-featured and narrower-cratered than musl's: --all-features
        # (so openssl enters via native-tls; graphNativeBuildInputs already
        # carries it for the LINK) and three more exclusions - baml_tests,
        # baml_cli and baml_lsp2_actions run in the snapshot-tests job, and
        # excluding them here skips the heaviest links in the workspace.
        #
        # .minimal for the same closure reason as the wasm graph; host
        # target needs no `targets` override.
        gnuToolchain = rustOverlayPkgs.rust-bin.stable.${ciRustChannel}.minimal;

        gnuWorkspace = cargoUnit.buildWorkspace {
          src = ./baml_language;
          workspaceRoot = ./baml_language;
          rustToolchain = gnuToolchain;

          # Same reason as the msrv graph: cache.ix.dev 404s /realisations,
          # so a floating-CA output is unsubstitutable through the only cache
          # the pool guests can read.
          contentAddressed = false;
          # pureBuild + the stable-toolchain embedMetadata override (see
          # l2Policy above for the trap).
          policy = l2Policy;

          # Host triple: no `target` argument, same as the msrv graph.

          # The lane runs bare `cargo test --no-run` (dev + test); stated
          # explicitly for the same debug_assertions reason as the msrv
          # graph.
          profile = "dev";

          # The lane's exact cargo selection, exclusions verbatim from the
          # job's build step.
          cargoTargets = [
            [
              "--workspace"
              "--all-features"
              "--tests"
              "--exclude"
              "baml_tests"
              "--exclude"
              "baml_cli"
              "--exclude"
              "baml_lsp2_actions"
              "--exclude"
              "sdk_test_*"
              "--exclude"
              "baml_bridge"
            ]
          ];
          cargoTargetNames = [ "gnu" ];

          env = {
            # Same two reasons as the msrv graph: cargo-unit does not read
            # cargo config's [env] table, and the workflow pins opt-level 1
            # on both profiles workflow-wide.
            RUST_MIN_STACK = "67108864";
            CARGO_PROFILE_DEV_OPT_LEVEL = "1";
            CARGO_PROFILE_TEST_OPT_LEVEL = "1";
            # NO debug=0: this lane RUNS its binaries (see the musl graph's
            # note; the closure-measure-first rule in nix/l2-roots.txt is
            # the enforcement).
          };

          nativeBuildInputs = graphNativeBuildInputs;
          packageBuildEnv = graphPackageBuildEnv;
        };

        # Common source filtering for crane
        src = pkgs.lib.cleanSourceWith {
          src = ./engine;
          filter =
            path: type:
            let
              baseName = baseNameOf path;
            in
            !pkgs.lib.hasInfix "target" path
            && !pkgs.lib.hasInfix ".git" path
            && !pkgs.lib.hasInfix ".jj" path
            && !pkgs.lib.hasInfix ".so" path
            && !pkgs.lib.hasInfix ".node" path
            && !pkgs.lib.hasInfix "node_modules" path
            && baseName != "result";
        };

        # Common arguments for all crane builds
        commonArgs = {
          inherit
            src
            version
            buildInputs
            nativeBuildInputs
            ;
          strictDeps = true;

          LIBCLANG_PATH = pkgs.libclang.lib + "/lib/";
          BINDGEN_EXTRA_CLANG_ARGS =
            if pkgs.stdenv.isDarwin then
              "-I${pkgs.llvmPackages.libclang.lib}/lib/clang/${pkgs.lib.versions.major pkgs.llvmPackages.libclang.version}/headers "
            else
              "-isystem ${pkgs.llvmPackages.libclang.lib}/lib/clang/${pkgs.lib.versions.major pkgs.llvmPackages.libclang.version}/include -isystem ${pkgs.llvmPackages.libclang.lib}/include -isystem ${pkgs.glibc.dev}/include";
          RUSTFLAGS =
            if pkgs.stdenv.isDarwin then
              "--cfg tracing_unstable"
            else
              "--cfg tracing_unstable -C target-feature=+crt-static";
          OPENSSL_STATIC = "1";
          OPENSSL_DIR = "${pkgs.openssl.dev}";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
          OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
          PROTOC_GEN_GO_PATH = "${protocGenGo}/bin/protoc-gen-go";
          SKIP_BAML_VALIDATION = "1";
        };

        # Build dependencies only (this will be cached separately)
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        devEnvInputs = (
          with pkgs;
          [
          ]
        );

        buildInputs =
          (with pkgs; [
            cmake
            git
            go
            gotools
            ruby
            ruby.devEnv
            mise
            openssl
            pkg-config
            lld
            pythonEnv
            maturin
            pnpm
            protocGenGo
            vsce # VSCode extension packaging tool
            toolchain
            pkgs-unstable.nodejs_20
            nodePackages.typescript
            pkgs-unstable.uv
            pkgs-unstable.flatbuffers
            wasm-pack
            pkgs.gcc
            napi-rs-cli
            wasm-bindgen-cli

            # For building the typescript client.
            pixman
            cairo
            pango
            libjpeg
            giflib
            librsvg
          ])
          ++ appleDeps;
        nativeBuildInputs = [
          pkgs.cmake
          pkgs.openssl
          pkgs.pkg-config
          pythonEnv
          pkgs.maturin
          pkgs.perl
          pkgs.ruby
        ]
        ++ pkgs.lib.optionals (!pkgs.stdenv.isDarwin) [
          pkgs.lld
          pkgs.gcc
        ];

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

            # Crane build function based on build type
            buildFn = if buildType == "release" then craneLib.buildPackage else craneLib.buildPackage;

            # Unset DEVELOPER_DIR_FOR_TARGET on macOS to avoid SDK conflicts
            preBuildWrapper = pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
              unset DEVELOPER_DIR_FOR_TARGET
            '';
          in
          buildFn (
            {
              inherit pname cargoArtifacts;
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

              # Set CARGO_PROFILE to control release vs debug builds
              CARGO_PROFILE = if buildType == "release" then "release" else "dev";

              # Prevent SDK conflicts on macOS
              preBuild = preBuildWrapper + (extraAttrs.preBuild or "");
            }
            // (if buildPhase != null then { inherit buildPhase; } else { })
            // (if installPhase != null then { inherit installPhase; } else { })
            // extraAttrs
          );
      in

      rec {

        # The cargo-build-msrv lane, as a nix build. Roots every test binary
        # `cargo test --no-run --all-features` produces; the derivation
        # itself is a marker, the work is in the units it depends on.
        #
        # The CI job probes cache.ix.dev for this path and only takes the nix
        # arm on a hit, because a unit-DAG miss is a regression rather than a
        # neutral outcome: sccache misses cost one compile, this costs the
        # whole graph with no incremental compilation and one sandbox per
        # rustc invocation.
        packages.msrv-check =
          if pkgs.stdenv.isDarwin then
            throw "packages.msrv-check builds the Linux CI lane's unit graph; there is no Darwin lane to mirror"
          else
            idxPkgs.runCommand "baml-msrv-check"
              {
                __structuredAttrs = true;
                strictDeps = true;
                testBinaries = map (target: target.binary) (
                  builtins.attrValues msrvWorkspace.tests
                );
              }
              ''
                set -euo pipefail
                mkdir -p "$out"
                printf '%s\n' "''${testBinaries[@]}" > "$out/test-binaries"
                echo "built ''${#testBinaries[@]} msrv test binaries" > "$out/result"
              '';

        # The graph's two IFD artifacts plus the vendor dir, as their own
        # root. An IFD is a build: a guest that cannot substitute these runs
        # a full cargo resolve over 928 packages and the renderer at
        # EVALUATION time, before a single compile unit is considered. They
        # are build-time deps of msrv-check, so they are absent from its
        # runtime closure and the builder has to push them deliberately.
        # index hit exactly this and fixed it the same way (crossIfdRoots,
        # index#1687).
        packages.msrv-eval-roots =
          if pkgs.stdenv.isDarwin then
            throw "packages.msrv-eval-roots belongs to the Linux msrv graph"
          else
            idxPkgs.runCommand "baml-msrv-eval-roots"
              {
                __structuredAttrs = true;
                strictDeps = true;
                roots = [
                  msrvWorkspace.unitsNix
                  msrvWorkspace.unitGraphJson
                  msrvWorkspace.vendorDir
                ];
              }
              ''
                set -euo pipefail
                mkdir -p "$out"
                printf '%s\n' "''${roots[@]}" > "$out/eval-roots"
              '';

        # The cargo-test-wasm lane, as a nix build. Roots every unit that
        # lane's `cargo build -p ... --target wasm32-unknown-unknown
        # --no-default-features --release` produces -- libs and bins of the
        # selected packages -- and copies the .wasm artifacts out so the job
        # can list them the way it lists target/'s today.
        packages.wasm-check =
          if pkgs.stdenv.isDarwin then
            throw "packages.wasm-check builds the Linux CI lane's unit graph; there is no Darwin lane to mirror"
          else
            idxPkgs.runCommand "baml-wasm-check"
              {
                __structuredAttrs = true;
                strictDeps = true;
                unitRoots = wasmWorkspace.roots;
              }
              ''
                set -euo pipefail
                mkdir -p "$out/wasm"
                printf '%s\n' "''${unitRoots[@]}" > "$out/unit-roots"
                # Collected rather than symlinked to the units so `ls -lh`
                # in the job reports the artifact sizes, which is the only
                # thing the lane's "List WASM artifacts" step is for. A unit
                # with no .wasm output (an rlib dependency root) is normal.
                #
                # Two naming differences from cargo, both measured against
                # the cargo arm's four .wasm files. cargo drops every wasm
                # artifact into one target directory as <name>.wasm; nix
                # splits them by kind, and installs cdylib outputs as
                # lib/<name>.wasm but bin outputs as bin/<name> with no
                # extension at all. A lib/*.wasm sweep finds three of the
                # four; adding bin/*.wasm still finds three, because the
                # [[bin]] crate (tools_sap_visualizer) has no suffix to
                # match.
                #
                # So: sweep both directories and decide by the file's magic
                # rather than by its path. A host-platform executable can
                # then never be filed as a wasm artifact, which a
                # path-shaped rule would do silently.
                for root in "''${unitRoots[@]}"; do
                  for artifact in "$root"/lib/*.wasm "$root"/bin/*; do
                    [ -f "$artifact" ] || continue
                    magic=$(head -c 4 "$artifact" | od -An -tx1 | tr -d ' \n')
                    [ "$magic" = "0061736d" ] || continue
                    cp -n "$artifact" "$out/wasm/$(basename "$artifact" .wasm).wasm"
                  done
                done
                echo "built ''${#unitRoots[@]} wasm unit roots" > "$out/result"
              '';

        packages.wasm-eval-roots =
          if pkgs.stdenv.isDarwin then
            throw "packages.wasm-eval-roots belongs to the Linux wasm graph"
          else
            idxPkgs.runCommand "baml-wasm-eval-roots"
              {
                __structuredAttrs = true;
                strictDeps = true;
                roots = [
                  wasmWorkspace.unitsNix
                  wasmWorkspace.unitGraphJson
                  wasmWorkspace.vendorDir
                ];
              }
              ''
                set -euo pipefail
                mkdir -p "$out"
                printf '%s\n' "''${roots[@]}" > "$out/eval-roots"
              '';

        # The package selection the wasm graph was built from, as a file the
        # job can diff against what `cargo metadata` reports on the runner.
        #
        # Deliberately independent of wasmWorkspace: it evaluates and builds
        # in milliseconds with no IFD, so the job can afford to check the
        # selection BEFORE committing to the nix arm. A mismatch means the
        # flake's transcription of Cargo's `members` has drifted from cargo's
        # own resolution, which would otherwise show up as the nix arm
        # quietly compiling a different set of crates than the lane does.
        packages.wasm-package-list = idxPkgs.writeText "baml-wasm-packages" (
          nixpkgs.lib.concatMapStrings (name: name + "\n") wasmPackages
        );

        # The musl lane's root: the machinery's whole-workspace nextest
        # export. Not a `-check` blob of binaries -- the export's two JSON
        # manifests interpolate every test binary's store path, so
        # substituting THIS one derivation substitutes the manifests and all
        # test binaries in one move, and the job hands them straight to
        # `cargo nextest run --binaries-metadata`. The name follows the
        # l2-roots contract (<lane>-<what it is>).
        packages.musl-test-export =
          if pkgs.stdenv.isDarwin then
            throw "packages.musl-test-export builds the Linux CI lane's unit graph; there is no Darwin lane to mirror"
          else
            muslWorkspace.nextestExport;

        packages.musl-eval-roots =
          if pkgs.stdenv.isDarwin then
            throw "packages.musl-eval-roots belongs to the Linux musl graph"
          else
            idxPkgs.runCommand "baml-musl-eval-roots"
              {
                __structuredAttrs = true;
                strictDeps = true;
                roots = [
                  muslWorkspace.unitsNix
                  muslWorkspace.unitGraphJson
                  muslWorkspace.vendorDir
                ];
              }
              ''
                set -euo pipefail
                mkdir -p "$out"
                printf '%s\n' "''${roots[@]}" > "$out/eval-roots"
              '';

        # The gnu lane's root: same whole-workspace nextest export shape as
        # musl's (see that note above).
        packages.gnu-test-export =
          if pkgs.stdenv.isDarwin then
            throw "packages.gnu-test-export builds the Linux CI lane's unit graph; there is no Darwin lane to mirror"
          else
            gnuWorkspace.nextestExport;

        packages.gnu-eval-roots =
          if pkgs.stdenv.isDarwin then
            throw "packages.gnu-eval-roots belongs to the Linux gnu graph"
          else
            idxPkgs.runCommand "baml-gnu-eval-roots"
              {
                __structuredAttrs = true;
                strictDeps = true;
                roots = [
                  gnuWorkspace.unitsNix
                  gnuWorkspace.unitGraphJson
                  gnuWorkspace.vendorDir
                ];
              }
              ''
                set -euo pipefail
                mkdir -p "$out"
                printf '%s\n' "''${roots[@]}" > "$out/eval-roots"
              '';

        packages.default = bamlRustPackage {
          pname = "baml-cli";
          buildType = "release";
          installPhase = ''
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
            strip $out/bin/baml-cli 2>/dev/null || true
            runHook postInstall
          '';
          extraAttrs = {
            PYTHON_SYS_EXECUTABLE = "${pythonEnv}/bin/python3";
            LD_LIBRARY_PATH = "${pythonEnv}/lib";
            PYTHONPATH = "${pythonEnv}/${pythonEnv.sitePackages}";
            # CC="${clang}/bin/clang"; # Temporarily commented out for linux testing.
          };
        };

        packages."baml-cli-musl" =
          if pkgs.stdenv.isDarwin then
            throw "musl builds are not supported on macOS - use the default package instead"
          else
            let
              muslPkgs = pkgs.pkgsStatic;

              muslCommonArgs = commonArgs // {
                buildInputs = (
                  with muslPkgs;
                  [
                    cmake
                    git
                    openssl
                    pkg-config
                    pythonEnv
                    gcc
                  ]
                );
                nativeBuildInputs = [
                  pkgs.cmake
                  muslPkgs.openssl
                  pkgs.pkg-config
                  pythonEnv
                  pkgs.perl
                ];
                CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
                CARGO_BUILD_RUSTFLAGS = "--cfg tracing_unstable -C target-feature=+crt-static";
                OPENSSL_STATIC = "1";
                OPENSSL_DIR = "${muslPkgs.openssl.dev}";
                OPENSSL_LIB_DIR = "${muslPkgs.openssl.out}/lib";
                OPENSSL_INCLUDE_DIR = "${muslPkgs.openssl.dev}/include";
              };
            in
            craneLib.buildPackage (
              muslCommonArgs
              // {
                pname = "baml-cli";
                cargoExtraArgs = "--target x86_64-unknown-linux-musl";

                installPhase = ''
                  runHook preInstall
                  build_root=''${CARGO_TARGET_DIR:-target}
                  mkdir -p $out/bin
                  BINARY_NAME="$build_root/x86_64-unknown-linux-musl/release/baml-cli"
                  if [ ! -x "$BINARY_NAME" ]; then
                    echo "Unable to locate the compiled musl CLI binary at $BINARY_NAME" >&2
                    exit 1
                  fi
                  cp "$BINARY_NAME" $out/bin/baml-cli
                  strip $out/bin/baml-cli 2>/dev/null || true
                  runHook postInstall
                '';
              }
            );

        packages."baml-cli-debug" = bamlRustPackage {
          pname = "baml-cli";
          buildType = "debug";
          installPhase = ''
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
            runHook postInstall
          '';
          extraAttrs = {
            PYTHON_SYS_EXECUTABLE = "${pythonEnv}/bin/python3";
            LD_LIBRARY_PATH = "${pythonEnv}/lib";
            PYTHONPATH = "${pythonEnv}/${pythonEnv.sitePackages}";
          };
        };

        packages.pyLib = bamlRustPackage {
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

        packages."baml-py-debug" = bamlRustPackage {
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

        packages.baml-py = pkgs.python3Packages.buildPythonPackage {
          pname = "baml-py";
          inherit version;
          format = "wheel";

          # Find the actual wheel file with platform tags
          src =
            let
              wheelDir = "${packages.pyLib}/lib";
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

        packages.tsLib = bamlRustPackage {
          pname = "baml-ts";
          buildType = "release";
          nativeBuildInputsExtra = [
            pkgs-unstable.nodejs_20
            pkgs.napi-rs-cli
            pkgs.pnpm
          ];
          buildPhase = ''
            # Unset conflicting environment variable for macOS SDK
            echo "Unsetting DEVELOPER_DIR_FOR_TARGET"
            unset DEVELOPER_DIR_FOR_TARGET

            # Build the CLI
            echo "Building the CLI"
            cargo build $CARGO_RELEASE_FLAG -p baml-cli

            # Build specifically the typescript FFI crate
            echo "Building the typescript FFI crate"
            cargo build $CARGO_RELEASE_FLAG -p baml-typescript-ffi

            # The build artifacts are in the crane-managed target directory
            echo "CARGO_TARGET_DIR is: ''${CARGO_TARGET_DIR:-target}"
            echo "Looking for build outputs..."
            find . -name "libbaml.*" -type f 2>/dev/null || true

            cd language_client_typescript

            echo "Listing current directory contents:"
            ls -la

            # Copy the built library to where napi expects it
            echo "Copying the built library to where napi expects it"
            build_root=''${CARGO_TARGET_DIR:-../target}
            cargo_lib_dir="$build_root"
            mkdir -p "$build_root/$CARGO_PROFILE_DIR"
            echo "Searching for shared libraries in $cargo_lib_dir:"
            find "$cargo_lib_dir" -name "*.so" -o -name "*.dylib" -o -name "*.dll" 2>/dev/null || true
            shared_lib=$(find "$cargo_lib_dir" -type f \( -name "libbaml.so" -o -name "libbaml.dylib" -o -name "libbaml.dll" \) 2>/dev/null | head -n1)
            if [ -z "$shared_lib" ]; then
              echo "Unable to locate built shared library" >&2
              echo "Trying absolute search from root of build..."
              find .. -name "libbaml.*" -type f 2>/dev/null || true
              exit 1
            fi
            lib_basename=$(basename "$shared_lib")
            case "$lib_basename" in
              *.so)
                cp "$shared_lib" "$build_root/$CARGO_PROFILE_DIR/libbaml_typescript_ffi.so"
                ;;
              *.dylib)
                cp "$shared_lib" "$build_root/$CARGO_PROFILE_DIR/libbaml_typescript_ffi.dylib"
                cp "$shared_lib" "$build_root/$CARGO_PROFILE_DIR/libbaml_typescript_ffi.so"
                ;;
              *.dll)
                cp "$shared_lib" "$build_root/$CARGO_PROFILE_DIR/libbaml_typescript_ffi.dll"
                ;;
            esac

            mkdir -p dist
            cp "$shared_lib" "dist/$lib_basename"

            # Only create symlink if it doesn't already exist with the right name
            ffi_lib=$(find "$cargo_lib_dir/$CARGO_PROFILE_DIR" -type f -name 'libbaml_typescript_ffi*.dylib' | head -n1)
            if [ -n "$ffi_lib" ] && [ "$(basename "$ffi_lib")" != "libbaml_typescript_ffi.dylib" ]; then
              mkdir -p "$build_root/$CARGO_PROFILE_DIR"
              ln -sf "$ffi_lib" "$build_root/$CARGO_PROFILE_DIR/libbaml_typescript_ffi.dylib"
            fi

            # Build the native module directly with release flag
            env -u DEVELOPER_DIR_FOR_TARGET napi build --platform $CARGO_RELEASE_FLAG --js ./native.js --dts ./native.d.ts

            # Compile TypeScript files using the Nix-provided TypeScript
            ${pkgs.nodePackages.typescript}/bin/tsc ./typescript_src/*.ts --outDir ./dist --module commonjs --allowJs --declaration true || true

            # Copy any pre-existing JavaScript files that might be needed
            cp *.js dist/ || true

            # Copy TypeScript declarations
            cp *.d.ts dist/ || true

            # Copy the native modules
            cp *.node dist/

            if [ "$(uname)" = "Darwin" ]; then
              echo "Fixing macOS Mach-O install names for bundled native modules"

              pending=1
              while [ "$pending" -eq 1 ]; do
                pending=0
                for bundle in dist/*.node dist/*.dylib dist/*.so; do
                  [ -e "$bundle" ] || continue
                  chmod +w "$bundle" 2>/dev/null || true

                  case "$(basename "$bundle")" in
                    *.dylib|*.so|*.node)
                      install_name_tool -id "@loader_path/$(basename "$bundle")" "$bundle"
                      ;;
                  esac

                  for dep in $(otool -L "$bundle" | tail -n +2 | awk '{print $1}'); do
                    [ -z "$dep" ] && continue
                    case "$dep" in
                      /System/*|@loader_path/*|@rpath/*)
                        ;;
                      /usr/lib/libiconv.2.dylib|/usr/lib/libcharset.1.dylib|/usr/lib/libSystem.B.dylib)
                        install_name_tool -change "$dep" "$dep" "$bundle"
                        ;;
                      *)
                        dep_name=$(basename "$dep")

                        case "$dep_name" in
                          libiconv.2.dylib)
                            install_name_tool -change "$dep" "/usr/lib/libiconv.2.dylib" "$bundle"
                            continue
                            ;;
                          libcharset.1.dylib)
                            install_name_tool -change "$dep" "/usr/lib/libcharset.1.dylib" "$bundle"
                            continue
                            ;;
                          libSystem.B.dylib)
                            install_name_tool -change "$dep" "/usr/lib/libSystem.B.dylib" "$bundle"
                            continue
                            ;;
                        esac

                        dest="dist/$dep_name"
                        if [ ! -e "$dest" ]; then
                          echo "  bundling $dep_name"
                          cp "$dep" "$dest"
                          chmod +w "$dest" 2>/dev/null || true
                          pending=1
                        fi
                        echo "    rewriting $(basename "$bundle") dependency $dep"
                        install_name_tool -change "$dep" "@loader_path/$dep_name" "$bundle"
                        ;;
                    esac
                  done
                done
              done
              strip -x dist/*.dylib 2>/dev/null || true
              strip -x dist/*.node 2>/dev/null || true
            else
              strip dist/*.so 2>/dev/null || true
            fi

            # Create minimal package.json and package-lock.json
            cat > dist/package.json << EOF
            {
              "name": "@boundaryml/baml",
              "version": "${version}",
              "bin": {
                "baml-cli": "./bin/baml-cli"
              },
              "files": [
                "*.js",
                "*.ts",
                "*.node",
                "*.dylib",
                "*.so",
                "*.dll",
                "bin/baml-cli"
              ],
              "dependencies": {},
              "os": ["linux", "darwin"],
              "cpu": ["x64", "arm64"]
            }
            EOF

            cat > dist/package-lock.json << EOF
            {
              "name": "@boundaryml/baml",
              "version": "${version}",
              "lockfileVersion": 2,
              "requires": true,
              "packages": {
                "": {
                  "name": "@boundaryml/baml",
                  "version": "${version}",
                  "dependencies": {},
                  "bin": {
                    "baml-cli": "bin/baml-cli"
                  }
                }
              }
            }
            EOF

            # Copy the CLI binary
            mkdir -p dist/bin
            cp "$cargo_lib_dir/$CARGO_PROFILE_DIR/baml-cli" dist/bin/baml-cli
            strip dist/bin/baml-cli 2>/dev/null || true
          '';
          installPhase = ''
            mkdir -p $out/lib
            cp -r dist/* $out/lib/
          '';
          extraAttrs = {
            SKIP_BAML_VALIDATION = "1";
          };
        };

        packages."tsLib-debug" = bamlRustPackage {
          pname = "baml-ts";
          buildType = "debug";
          nativeBuildInputsExtra = [
            pkgs-unstable.nodejs_20
            pkgs.napi-rs-cli
            pkgs.pnpm
          ];
          buildPhase = ''
            echo "Unsetting DEVELOPER_DIR_FOR_TARGET"
            unset DEVELOPER_DIR_FOR_TARGET

            echo "Building the CLI"
            cargo build -p baml-cli

            echo "Building the typescript FFI crate"
            cargo build -p baml-typescript-ffi

            echo "CARGO_TARGET_DIR is: ''${CARGO_TARGET_DIR:-target}"
            echo "Looking for build outputs..."
            find . -name "libbaml.*" -type f 2>/dev/null || true

            cd language_client_typescript

            echo "Listing current directory contents:"
            ls -la

            echo "Copying the built library to where napi expects it"
            build_root=''${CARGO_TARGET_DIR:-../target}
            cargo_lib_dir="$build_root"
            mkdir -p "$build_root/debug"
            echo "Searching for shared libraries in $cargo_lib_dir:"
            find "$cargo_lib_dir" -name "*.so" -o -name "*.dylib" -o -name "*.dll" 2>/dev/null || true
            shared_lib=$(find "$cargo_lib_dir" -type f \( -name "libbaml.so" -o -name "libbaml.dylib" -o -name "libbaml.dll" \) 2>/dev/null | head -n1)
            if [ -z "$shared_lib" ]; then
              echo "Unable to locate built shared library" >&2
              echo "Trying absolute search from root of build..."
              find .. -name "libbaml.*" -type f 2>/dev/null || true
              exit 1
            fi
            lib_basename=$(basename "$shared_lib")
            case "$lib_basename" in
              *.so)
                cp "$shared_lib" "$build_root/debug/libbaml_typescript_ffi.so"
                ;;
              *.dylib)
                cp "$shared_lib" "$build_root/debug/libbaml_typescript_ffi.dylib"
                cp "$shared_lib" "$build_root/debug/libbaml_typescript_ffi.so"
                ;;
              *.dll)
                cp "$shared_lib" "$build_root/debug/libbaml_typescript_ffi.dll"
                ;;
            esac

            mkdir -p dist
            cp "$shared_lib" "dist/$lib_basename"

            # Only create symlink if it doesn't already exist with the right name
            ffi_lib=$(find "$cargo_lib_dir/debug" -type f -name 'libbaml_typescript_ffi*.dylib' | head -n1)
            if [ -n "$ffi_lib" ] && [ "$(basename "$ffi_lib")" != "libbaml_typescript_ffi.dylib" ]; then
              mkdir -p "$build_root/debug"
              ln -sf "$ffi_lib" "$build_root/debug/libbaml_typescript_ffi.dylib"
            fi

            env -u DEVELOPER_DIR_FOR_TARGET napi build --platform --js ./native.js --dts ./native.d.ts

            ${pkgs.nodePackages.typescript}/bin/tsc ./typescript_src/*.ts --outDir ./dist --module commonjs --allowJs --declaration true || true

            cp *.js dist/ || true
            cp *.d.ts dist/ || true
            cp *.node dist/

            if [ "$(uname)" = "Darwin" ]; then
              echo "Fixing macOS Mach-O install names for bundled native modules"

              pending=1
              while [ "$pending" -eq 1 ]; do
                pending=0
                for bundle in dist/*.node dist/*.dylib dist/*.so; do
                  [ -e "$bundle" ] || continue
                  chmod +w "$bundle" 2>/dev/null || true

                  case "$(basename "$bundle")" in
                    *.dylib|*.so|*.node)
                      install_name_tool -id "@loader_path/$(basename "$bundle")" "$bundle"
                      ;;
                  esac

                  for dep in $(otool -L "$bundle" | tail -n +2 | awk '{print $1}'); do
                    [ -z "$dep" ] && continue
                    case "$dep" in
                      /System/*|/usr/lib/*|@loader_path/*|@rpath/*)
                        ;;
                      *)
                        dep_name=$(basename "$dep")
                        dest="dist/$dep_name"

                        case "$dep_name" in
                          libiconv.2.dylib)
                            echo "    remapping $dep_name to /usr/lib/libiconv.2.dylib"
                            install_name_tool -change "$dep" "/usr/lib/libiconv.2.dylib" "$bundle"
                            continue
                            ;;
                          libcharset.1.dylib)
                            echo "    remapping $dep_name to /usr/lib/libcharset.1.dylib"
                            install_name_tool -change "$dep" "/usr/lib/libcharset.1.dylib" "$bundle"
                            continue
                            ;;
                          libintl.8.dylib)
                            echo "    remapping $dep_name to /usr/local/lib/libintl.8.dylib"
                            install_name_tool -change "$dep" "/usr/local/lib/libintl.8.dylib" "$bundle"
                            continue
                            ;;
                        esac

                        if [ ! -e "$dest" ]; then
                          echo "  bundling $dep_name"
                          cp "$dep" "$dest"
                          chmod +w "$dest" 2>/dev/null || true
                          pending=1
                        fi
                        echo "    rewriting $(basename "$bundle") dependency $dep"
                        install_name_tool -change "$dep" "@loader_path/$dep_name" "$bundle"
                        ;;
                    esac
                  done
                done
              done
            fi

            mkdir -p dist/bin
            cp "$cargo_lib_dir/debug/baml-cli" dist/bin/baml-cli
          '';
          installPhase = ''
            mkdir -p $out/lib
            cp -r dist/* $out/lib/
          '';
          extraAttrs = {
            SKIP_BAML_VALIDATION = "1";
          };
        };

        packages.baml-ts =
          let
            # Create a source with files in the correct location
            npmSource = pkgs.runCommand "baml-ts-${version}-source" { } ''
              mkdir -p $out
              cp -r ${packages.tsLib}/lib/* $out/
            '';
          in
          pkgs.buildNpmPackage {
            pname = "baml";
            inherit version;

            src = npmSource;

            npmDepsHash = "sha256-6l5OwLGhW+c2mUhVUDwxH5rs5pzxd0+uTOrx14q04KY=";
            forceEmptyCache = true;

            buildInputs = [ pkgs-unstable.nodejs_20 ];

            # Configure npm to use temporary directories
            NPM_CONFIG_CACHE = "./tmp/npm";
            NPM_CONFIG_TMP = "./tmp/npm";
            NPM_CONFIG_PREFIX = "./tmp/npm";

            buildPhase = ''
              # Ensure temp directories exist
              mkdir -p tmp/npm
              npm pack
            '';

            installPhase = ''
              mkdir -p $out/lib
              touch $out/results.txt
              ls -lha
              ls -la  >> $out/results.txt
              cp boundaryml-baml-${version}.tgz $out/lib/
            '';
          };

        packages."baml-ts-debug" =
          let
            npmSource = pkgs.runCommand "baml-ts-${version}-debug-source" { } ''
              mkdir -p $out
              cp -r ${packages."tsLib-debug"}/lib/* $out/
            '';
          in
          pkgs.buildNpmPackage {
            pname = "baml";
            inherit version;

            src = npmSource;

            npmDepsHash = "sha256-6l5OwLGhW+c2mUhVUDwxH5rs5pzxd0+uTOrx14q04KY=";
            forceEmptyCache = true;

            buildInputs = [ pkgs-unstable.nodejs_20 ];

            NPM_CONFIG_CACHE = "./tmp/npm";
            NPM_CONFIG_TMP = "./tmp/npm";
            NPM_CONFIG_PREFIX = "./tmp/npm";

            buildPhase = ''
              mkdir -p tmp/npm
              npm pack
            '';

            installPhase = ''
              mkdir -p $out/lib
              cp boundaryml-baml-${version}.tgz $out/lib/
            '';
          };

        devShell = pkgs.mkShell rec {
          inherit buildInputs;
          PATH = "${clang}/bin:$PATH";
          RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc;
          LIBCLANG_PATH = pkgs.libclang.lib + "/lib/";
          # UV_PYTHON = "${pythonEnv}/bin/python3"; // This doesn't work with maturin.
          BINDGEN_EXTRA_CLANG_ARGS =
            if pkgs.stdenv.isDarwin then
              "" # Rely on default includes provided by stdenv.cc + libclang
            else
              # llvmPackages_17 was removed from nixpkgs; use the default LLVM
              # set and derive the clang resource dir from it (same pattern as
              # the build env above). Since LLVM 16 that directory is named by
              # MAJOR version only, so the full version would be a path that
              # does not exist and clang would silently ignore the -isystem.
              "-isystem ${pkgs.llvmPackages.libclang.lib}/lib/clang/${pkgs.lib.versions.major pkgs.llvmPackages.libclang.version}/include -isystem ${pkgs.llvmPackages.libclang.lib}/include -isystem ${pkgs.glibc.dev}/include";

          # Prevent SDK conflicts on macOS and configure CGO for Go
          shellHook = pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
            unset DEVELOPER_DIR_FOR_TARGET
            # Use native macOS SDK for Go instead of Nix SDK to avoid version mismatch
            # The Nix SDK (11.3) is too old for some Go packages that require macOS 12+ APIs
            if [ -d "/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk" ]; then
              export SDKROOT="/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk"
            elif [ -d "/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk" ]; then
              export SDKROOT="/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk"
            fi
            export CGO_ENABLED=1
            export CGO_LDFLAGS="-isysroot $SDKROOT"
          '';
        };

        # CI shells, Linux-only by construction; the legacy `devShell` above
        # stays the default shell.
        #   cross   - Linux -> Darwin/Windows cross builds: zig cc + pinned
        #             macOS SDK for the Apple triples, cargo-xwin for MSVC.
        #   ci      - the toolchain surface the Linux CI jobs consume
        #             (rust-toolchain.toml channel via fenix).
        #   ci-msrv - same shell pinned to the baml_language MSRV, read from
        #             the workspace manifest so the shell and the
        #             cargo-build-msrv gate cannot drift.
        #   ci-sdk  - the ci surface plus the sdk-test language toolchains
        #             (node/pnpm, temurin+gradle); one attr for the whole
        #             ix-sdk runner family.
        devShells =
          nixpkgs.lib.optionalAttrs (system == "x86_64-linux") {
            cross = import ./nix/cross-shell.nix {
              pkgs = rustOverlayPkgs;
            };
            ci = import ./nix/ci-shell.nix {
              inherit
                pkgs
                pkgs-unstable
                toolchain
                protocGenGo
                ;
            };
            ci-msrv = import ./nix/ci-shell.nix {
              inherit pkgs pkgs-unstable protocGenGo;
              toolchain = msrvToolchain;
            };
            # The sdk-tests matrix's shell: the ci surface plus the language
            # toolchains its lanes spawn from PATH. One shared attr (not one
            # per lane) because every Linux sdk lane runs on the same ix-sdk
            # runner family, so one closure warms all of them. Versions track
            # mise.toml's pins as closely as the pinned nixpkgs allows; the
            # accepted drift is listed in the converting PR. dotnet is NOT
            # here: the csharp lane's SDK comes from actions/setup-dotnet on
            # both arms, independent of mise, and stays that way.
            ci-sdk = import ./nix/ci-shell.nix {
              inherit
                pkgs
                pkgs-unstable
                toolchain
                protocGenGo
                ;
              extras = [
                # typescript / typescript-web (mise: node 22, pnpm)
                pkgs.nodejs_22
                pkgs.pnpm
                # java (mise: temurin-23 + gradle 8.14; the sdk_test_java
                # setup builds the bridge with `gradle` from PATH)
                pkgs.temurin-bin-23
                pkgs.gradle
              ];
              extraEnv = {
                # gradle resolves the JDK from JAVA_HOME; pin it to the same
                # temurin the fallback arm's mise config selects.
                JAVA_HOME = "${pkgs.temurin-bin-23}";
              };
            };
          };
      }
    )
    // {
      # Self-hosted CI runner pool on ix VMs; this repo carries only the
      # policy in nix/ci-runner.nix.
      nixosConfigurations = ix-runners.lib.mkPool {
        nixpkgs = nixpkgs-ci;
        configRev = self.rev or null;
        # The pool's definition, read here AND by the reconcile workflow, so
        # its size cannot be two different numbers in two files.
        spec = builtins.fromTOML (builtins.readFile ./nix/ix-pool.toml);
        # NOTE, measured 2026-08-15: do NOT bake the CI shell closures into
        # the image (system.extraDependencies) yet. The in-guest template
        # builder runs with base-image nix settings - it cannot substitute
        # through this config's own substituters (chicken-and-egg) - so the
        # bake turned every member create into a 30+ min toolchain compile,
        # once per (rev, attr) even though all attrs share one profile hash.
        # Re-land once the platform (a) lets template builds substitute via
        # cache.ix.dev and (b) keys the template cache by profile hash.
        modules = [ ./nix/ci-runner.nix ];
      };
    };

}
