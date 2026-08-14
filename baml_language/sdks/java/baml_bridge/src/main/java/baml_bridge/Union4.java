package baml_bridge;

/**
 * Generic union arity family (TEAM DECISION 2026-07-16): an anonymous
 * multi-arm BAML union of <em>4</em> arms renders as {@code Union4}, a
 * sealed interface with one nested generic {@code record Arm{i}} per positional
 * arm in BAML declaration order (post-normalization, the {@code null} arm
 * stripped). Every arm record declares <em>all</em> 4 type parameters and
 * wraps only its own positional arm's type, so Java&nbsp;21+ consumers get an
 * exhaustive {@code switch} with record patterns
 * (e.g. {@code case Union4.Arm0(var x) -> ...}); Java&nbsp;17 consumers use
 * {@code instanceof}. Sealed interfaces and records are Java&nbsp;17 features,
 * so this compiles under {@code --release 17}; only the <em>consumer's</em>
 * pattern {@code switch} needs 21, and this library never uses one.
 *
 * <p>Decode is type-directed: the runtime matches the wire value against the
 * declared arm list in source order (see {@code baml_bridge.internal.ProtoReader}),
 * then reflectively constructs the matching {@code Arm{i}}. Arity&nbsp;&gt;&nbsp;10
 * is a codegen error until the threshold/alias policy lands.
 */

public sealed interface Union4<T0, T1, T2, T3> extends BamlUnion permits Union4.Arm0, Union4.Arm1, Union4.Arm2, Union4.Arm3 {
    record Arm0<T0, T1, T2, T3>(T0 value) implements Union4<T0, T1, T2, T3> {}
    record Arm1<T0, T1, T2, T3>(T1 value) implements Union4<T0, T1, T2, T3> {}
    record Arm2<T0, T1, T2, T3>(T2 value) implements Union4<T0, T1, T2, T3> {}
    record Arm3<T0, T1, T2, T3>(T3 value) implements Union4<T0, T1, T2, T3> {}
}
