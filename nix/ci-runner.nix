# BAML's CI runner pool: which toolchains ride the job PATH and how big the
# pool is. Everything else (runner units, persistence, NixOS compat for the
# FHS toolchains below, platform workarounds) is the ix-maintained mechanism
# at github:indexable-inc/ix-runners; fixes arrive as a bump of that input.
#
# This path is load-bearing: the reconcile's staleness check watches nix/,
# flake.nix and flake.lock, so moving this file out of nix/ stops config
# changes from rolling the pool.
{
  config,
  lib,
  pkgs,
  # Which member of the pool this evaluation is. mkPool hands it to every
  # module through specialArgs (ix-runners/flake.nix:45-47), which is what
  # makes a per-member label set expressible at all. Defaulted so the module
  # still evaluates when imported outside mkPool.
  poolIndex ? 1,
  ...
}:
let
  # The module gives every slot its own 0700 HOME, so a HOME-relative value
  # has to be resolved here: systemd's Environment= expands neither $HOME nor
  # a per-user %h (for a system unit %h is the service MANAGER's home, /root,
  # whatever User= says - measured). Read back off the slot user the module
  # created rather than rebuilt from its naming scheme.
  slotHomes = lib.filter (lib.hasPrefix "/var/lib/ix-runner-home/") (
    map (u: u.home) (lib.attrValues config.users.users)
  );
  # One value for the whole pool member is only correct while it has one
  # slot; a second slot's job would get slot 1's home, which it cannot read.
  slotHome =
    if lib.length slotHomes == 1 then
      lib.head slotHomes
    else
      throw (
        "nix/ci-runner.nix: expected exactly one runner slot home under "
        + "/var/lib/ix-runner-home, found ${toString (lib.length slotHomes)}. "
        + "Per-slot job env belongs in ix-runners, not here."
      );

  # -- job-family affinity ----------------------------------------------------
  # GitHub has no soft affinity. `runs-on` is an AND over labels and a runner
  # either carries all of them or is ineligible, so "run this job on the
  # machine that already has its caches" is only expressible as a hard
  # partition of the pool: one label per job family, assigned per member.
  #
  # Why it is worth the labels: warmth is this pool's entire advantage over a
  # hosted runner, and it is per member. Measured on the same commit
  # (baml-ix-ci-ledger.html, 2026-08-16, cold VM vs warm repeat of the same
  # job): cargo test musl 1347s -> 540s, sdk tests cpp 912s -> 203s, cargo
  # test wasm 343s -> 42s, cargo build msrv 1208s -> 771s. Warm, several
  # lanes beat Blacksmith outright with no shared compile cache; cold, none
  # of them do. Unpartitioned, which of those two a job gets is a lottery
  # over 32 members, and the pool's measured speed is really a placement
  # draw.
  #
  # A family is drawn around what one job leaves behind that the next can
  # use - a target/ directory, a toolchain, a package-manager store - so a
  # member converges on being warm for its whole family, not just one job.
  poolSpec = builtins.fromTOML (builtins.readFile ./ix-pool.toml);

  # The family each member index serves, dealt ROUND-ROBIN across the pool
  # rather than in contiguous blocks. That is load-bearing rather than
  # cosmetic: the ix-runners autoscaler is label-blind and index-ordered - it
  # starts the lowest stopped index and stops the highest online one. Dealt
  # round-robin, that index order becomes a round-robin over the families, so
  # the members kept alive by min-warm cover every family and each scale-up
  # wakes one more of each in turn. Dealt in blocks, the high-numbered
  # families would be the first switched off and cold on every wave - exactly
  # the lottery this is meant to end. ix-pool.toml pins min-warm to the family
  # count for the same reason: one member of every family survives any lull.
  #
  # Sizes are the measured wave, not a guess: a family gets at least as many
  # members as it has jobs in one full wave, so affinity costs no concurrency
  # at a single wave, and the surplus goes to the families whose jobs are
  # longest (gnu carries the 20-minute snapshot lane) or most expensive to
  # re-warm (msrv's 1.91.1 toolchain shares sccache with nothing else).
  memberFamily = [
    #     1        2        3        4        5       6       7        8         9
    "msrv"    "musl"   "gnu"    "sdk"    "cross" "web"   "wasm"   "light"  "general"
    #    10       11       12       13       14      15      16       17        18
    "msrv"    "musl"   "gnu"    "sdk"    "cross" "web"   "wasm"   "light"  "general"
    #    19       20       21       22       23
    "gnu"     "sdk"    "cross"  "web"    "light"
    #    24       25       26       27       28
    "gnu"     "sdk"    "cross"  "gnu"    "sdk"
    #    29       30       31       32
    "cross"   "sdk"    "sdk"    "sdk"
  ];

  family =
    if poolIndex >= 1 && poolIndex <= lib.length memberFamily then
      lib.elemAt memberFamily (poolIndex - 1)
    else
      throw (
        "nix/ci-runner.nix: poolIndex ${toString poolIndex} is outside the "
        + "${toString (lib.length memberFamily)}-entry affinity table."
      );

  # A "general" member carries no family label. It is the slack that absorbs
  # the jobs still routed at the bare `ix` label - the cross-platform release
  # legs, miri, the profiling ring - without diluting any family's warmth.
  familyLabels = lib.optional (family != "general") "ix-${family}";
in
{
  system.stateVersion = "25.05";

  assertions = [
    {
      assertion = lib.length memberFamily == poolSpec."pool-size";
      message =
        "nix/ci-runner.nix: the affinity table has "
        + "${toString (lib.length memberFamily)} entries but ix-pool.toml sets "
        + "pool-size = ${toString poolSpec."pool-size"}. A member past the end of "
        + "the table cannot be evaluated at all, and a table entry past the end of "
        + "the pool names a family no runner advertises - a job routed there queues "
        + "until it is cancelled. Keep the two equal.";
    }
  ];

  # The pool substitutes through ix's public binary cache. cache.ix.dev is a
  # pull-through cache (ncps in front of the ix fleet cache and
  # cache.nixos.org): any path one VM pulls is cached fleet-side for the rest
  # of the pool, so a config-rev roll re-warms from datacenter bandwidth
  # instead of upstream registries. Pull is anonymous; there is no push from
  # these VMs. This must live in the image: the runner slot users are
  # untrusted nix clients (asserted by the ix-runners module), so nothing a
  # job passes at runtime can add a substituter.
  nix.settings = {
    substituters = [
      "https://cache.ix.dev"
      "https://cache.nixos.org/"
    ];
    trusted-public-keys = [
      # ix fleet cache key (narinfos signed server-side) + the ncps front's
      # own re-signing key, then the nixpkgs default. Setting this option
      # replaces the default list, so cache.nixos.org-1 must be restated.
      "ix-workspace:JuAaeOPfR3GL3nUICpEz/88/+S3BzGF3L6bPYFy0GwI="
      "hil-stor-2:UYyDQcJ/iepiePK/ptHRqR2t98okIpsfOVqE0Pm5CwY="
      "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
    ];
    # Warm `nix develop` closures must survive collection or every GC
    # cold-starts the CI shell on the next job.
    keep-outputs = true;
    keep-derivations = true;
    # Reactive headroom: the ix-runners module only schedules a weekly
    # time-based GC, which a busy store can outrun. Collect down to 100 GiB
    # free whenever free space drops under 50 GiB.
    min-free = 50 * 1024 * 1024 * 1024;
    max-free = 100 * 1024 * 1024 * 1024;
    # nix's upstream default caches a MISSING narinfo for 3600s. On a
    # persistent runner that is an hour of blindness per root: an L2 probe
    # that runs before the builder has published answers "miss", and every
    # later job landing on the same member by family affinity re-reads that
    # negative entry instead of asking the cache, so rerunning the job once
    # the artifact exists cannot recover it. Measured on the pool: the
    # second of two consecutive msrv jobs on baml-r10-1 probed after the
    # push had landed and still missed, in 13s, with no network round trip.
    #
    # 60s matches what the ix fleet already pins for the same reason
    # (ix nix/modules/base/nix-settings.nix:187, which has a deploy-surface
    # check behind it); the pool image simply never inherited it. This only
    # reaches VMs at a pool roll, so the probe passes
    # --narinfo-cache-negative-ttl 0 explicitly as well - that is what makes
    # it correct on the members running today, and this makes the default
    # sane for everything else that substitutes here.
    narinfo-cache-negative-ttl = 60;
  };

  # Prefer IPv4 for every destination: some regions' guests hold global v6
  # addresses whose upstream gateway does not yet answer NDP, so any
  # AAAA-bearing destination dies with EHOSTUNREACH before the client falls
  # back (this killed the cargo-xwin lanes via download.visualstudio's AAAA).
  # glibc-level so every client is covered; remove once v6 delivery lands.
  environment.etc."gai.conf".text = ''
    precedence ::ffff:0:0/96 100
  '';

  services.ix-runner = {
    enable = true;
    url = "https://github.com/boundaryml/baml";
    # One job per VM: co-tenancy missed upstream wall-clock test bounds
    # (details: PR body). Pool size lives in flake.nix's mkPool.
    slots = 1;
    labels = [
      # Every member advertises this one. It is the pool's wake signal
      # (ix-pool.toml's runner-label, which mkPool asserts is present) and the
      # route for every job not yet moved onto a family label.
      "ix"
      "ix-linux-x64"
    ]
    ++ familyLabels;

    # Ubuntu-image parity: what BAML's jobs expect preinstalled.
    extraPackages = with pkgs; [
      rustup # jobs run `rustup show` to pull the pinned toolchain
      gcc
      gnumake
      cmake
      ninja
      pkg-config
      python3
      openssl
      git-lfs
      glibc.bin # mise-action probes `ldd` to pick its binary
      ruby # release-metadata packaging tests
      go # sdkgen_go's build script shells out to gofmt
      nodejs_22 # pyright runs on the PATH node
      # musl leg: the full cross gcc, under the name setup-musl-cross probes
      # for (the thin musl libc wrapper links broken static-PIE binaries).
      (writeShellScriptBin "musl-gcc" ''
        exec ${pkgsCross.musl64.stdenv.cc}/bin/x86_64-unknown-linux-musl-gcc "$@"
      '')
    ];

    jobEnvironment = {
      PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS = "true";
      PYRIGHT_PYTHON_GLOBAL_NODE = "1";
      # mise would source-build node/python on NixOS; prebuilts run fine.
      MISE_NODE_COMPILE = "0";
      MISE_PYTHON_COMPILE = "0";
      # openssl-sys (baml_language workspace) probes these; nix splits the
      # outputs Ubuntu ships together.
      OPENSSL_LIB_DIR = "${lib.getLib pkgs.openssl}/lib";
      OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
      PKG_CONFIG_PATH = lib.makeSearchPath "lib/pkgconfig" [ pkgs.openssl.dev ];
      # setup-dotnet defaults to /usr/share/dotnet, read-only here; the slot
      # HOME also keeps the runtime warm across jobs.
      DOTNET_INSTALL_DIR = "${slotHome}/.dotnet";
      # Test parallelism pinned to the 16-vCPU envelope these suites were
      # tuned on.
      NEXTEST_TEST_THREADS = "16";
      RUST_TEST_THREADS = "16";
      # Builds too, now that the pool runs many members at once: cargo
      # defaults -j to the guest's full core count, and every concurrent
      # member doing that oversubscribes the host it shares. Measured 1.19x
      # slower at full pool concurrency than with this pinned.
      CARGO_BUILD_JOBS = "16";
      # vitest sizes its worker pool off availableParallelism() - 1, and
      # vitest-pool-workers spawns one workerd process per worker: 63 here
      # (64-vCPU elastic ceiling) vs 15 on 16-vCPU Blacksmith. Measured: sdk
      # tests (typescript-web) died "Worker exited unexpectedly" 7/7 runs
      # unpinned, import/transform time 13x wall clock; upstream passes the
      # same fixture in 48s. 4, measured: the A/B on the pool ran default(63)
      # FAIL 2/2, 16 FAIL 2/2, 4 PASS 2/2 (cgroup oom_kill deltas +6..+13 on
      # every failing arm, +0 on both passes); 12 was then field-tested once
      # and SIGKILLed the same way. The binding constraint is memory, not
      # cores: virtio-mem guests can sit at ~6 GiB MemTotal when the worker
      # spawn burst lands. Note this variable also overrides the browser
      # pool's upstream min(12, cores-1) cap (vitest #7871) verbatim, so it
      # lowers the Playwright leg to 4 as well - accepted, that leg passes
      # comfortably and correctness beats parallelism here.
      VITEST_MAX_WORKERS = "4";
    };
  };
}
