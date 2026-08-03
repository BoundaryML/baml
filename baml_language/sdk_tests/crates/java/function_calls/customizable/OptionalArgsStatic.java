// Compile-surface (type-check) analog of optional_args_static.py.
//
// The Python original is a NON-test module: each line is a call the type
// checker (pyright) must reject, tagged `# pyright: ignore[...]`. The
// TypeScript analog (optional_args.static.ts) uses `// @ts-expect-error`.
//
// java-port note: Java has no `@javac-expect-error` mechanism — a file that
// contained the rejected calls uncommented would fail to compile the whole
// test source set. So this class documents each rejected call as a
// commented-out line with the reason it must NOT compile. It is deliberately
// not a @Test class; its value is the surface it pins for a human (and,
// eventually, a negative-compile harness) to verify.
//
// Against the generated API shape (ref-java-codegen-conventions.md):
//   * required `arg0` is a positional Java parameter (cannot be omitted);
//   * optionals are only reachable through the trailing configurator's
//     setters (`o.opt1(..)` / `o.opt2(..)`) — never positionally;
//   * unknown option keys have no setter;
//   * `arg0` is typed `long`, and `opt1` is typed `Long`.
final class OptionalArgsStatic {

    private OptionalArgsStatic() {}

    static void surface() {
        // Each line below is the Java equivalent of a `# pyright: ignore` line.
        // They are intentionally commented out; uncommenting any one must fail
        // javac (that is the assertion this file encodes).

        // Fns.optional_args_probe();
        //   -> required `arg0` cannot be omitted (no zero-arg overload).
        //   (Python: optional_args_probe()  # reportCallIssue)

        // Fns.optional_args_probe(1L, 8L);
        //   -> defaulted args are only reachable via the configurator, never
        //      as a second positional argument.
        //   (Python: optional_args_probe(1, 8)  # reportCallIssue)

        // Fns.optional_args_probe(1L, o -> o.opt3(1L));
        //   -> unknown option key: the generated configurator has no `opt3`.
        //   (Python: optional_args_probe(1, opt3=1)  # reportCallIssue)

        // Fns.optional_args_probe("x");
        //   -> wrong required type: `arg0` is `long`, not `String`.
        //   (Python: optional_args_probe("x")  # reportArgumentType)

        // Fns.optional_args_probe(1L, o -> o.opt1("x"));
        //   -> wrong option value type: `opt1` is `Long`, not `String`.
        //   (Python: optional_args_probe(1, opt1="x")  # reportArgumentType)
    }
}
