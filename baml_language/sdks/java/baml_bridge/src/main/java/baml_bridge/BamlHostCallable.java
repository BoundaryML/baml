package baml_bridge;

import java.util.List;
import java.util.Map;

/**
 * Marker + dispatch contract for a generated host-callable functional interface
 * (e.g. {@code baml_sdk.<ns>.IntOptCallback}). The Java SDK maps a BAML callable
 * parameter onto a {@code java.util.function.*} shape when its arity is &le; 2
 * with no optionals; a callable whose own type carries optional parameters (or
 * arity &gt; 2) has no {@code java.util.function} equivalent, so codegen emits a
 * {@code @FunctionalInterface} that extends this marker.
 *
 * <p>The generated interface keeps a single abstract SAM (so it stays a
 * {@code @FunctionalInterface} and a lambda can implement it) — e.g.
 * {@code Long apply(Long x, Opts $opts)} — and provides a {@code default}
 * override of {@link #__bamlDispatch} that reshapes the bridge's flat
 * declared-order argument list into that SAM call: required args come in
 * positionally, supplied optionals fold into the always-non-null {@code Opts}
 * bag (nullable fields for the optionals BAML omitted).
 *
 * <p>The bridge detects a host callable at encode time via {@code instanceof}
 * (this marker for generated interfaces, or a {@code java.util.function.*} type)
 * and invokes it through {@link #__bamlDispatch} on the dispatch executor. The
 * odd {@code __} name keeps it out of the way of user SAM names.
 */
public interface BamlHostCallable {

    /**
     * Invoke the underlying callable with the bridge's decoded argument list.
     *
     * @param positional required args in declared order (never null; may be
     *     empty)
     * @param optional supplied optionals keyed by BAML parameter name (never
     *     null; an omitted optional is simply absent, so the generated bag reads
     *     {@code null} for it)
     * @return the callable's result (a {@code CompletableFuture} is awaited by
     *     the dispatcher; anything else is encoded directly)
     */
    Object __bamlDispatch(List<Object> positional, Map<String, Object> optional);
}
