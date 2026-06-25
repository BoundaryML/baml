// Namespace root.build — the project's build command.
//
//   baml run build.run
//
// Shells out to `baml pack` for every distributable entry point, writing the
// binaries into dist/ (gitignored — rebuild any time with this command).

class Target {
    entry: string,
    out: string,
}

// Two toolchain-0.11.x workarounds in the loop below:
// - `while` + index instead of `for (let t in targets)`, and `cmd` passed to
//   baml.sys.shell as its ONLY use. If a concat-built string is consumed by
//   another builtin first (e.g. println("→ " + cmd)), sys.shell receives an
//   empty command and silently no-ops (exit 0, no output, no side effects).
//   So the progress line is built from the Target fields directly.
// - `baml pack` won't create the parent directory itself, hence mkdir -p.
//
// throws: our own failure message, plus what baml.sys.shell itself can throw.
function run() -> string throws string | baml.errors.Io | baml.errors.Timeout {
    let targets = [
        Target { entry: "agent.main", out: "dist/bamlcode" },
        Target { entry: "server.serve", out: "dist/server" },
    ];

    let i = 0;
    while (i < targets.length()) {
        let t = targets[i];
        i = i + 1;
        baml.io.println("→ baml pack " + t.entry + " -o " + t.out);
        let cmd = "mkdir -p dist && baml pack " + t.entry + " --output-format debug -o " + t.out;
        let res = baml.sys.shell(cmd, null);
        if (res.exit_code != 0) {
            throw "pack failed for " + t.entry + ":\n" + res.stderr.to_string();
        };
        if (!baml.fs.exists(t.out)) {
            throw "pack reported success but " + t.out + " was not created";
        };
        baml.io.println("  ✓ " + t.out);
    }

    "packed " + baml.unstable.string(targets.length()) + " binaries into dist/"
}
