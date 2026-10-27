package baml_bridge.internal;

import baml_bridge.BamlHostCallable;
import java.util.List;
import java.util.Map;
import java.util.function.BiConsumer;
import java.util.function.BiFunction;
import java.util.function.Consumer;
import java.util.function.Function;
import java.util.function.Supplier;

/**
 * The Java shapes a host callable takes, and how the bridge calls each: a
 * generated {@link BamlHostCallable} interface takes the arguments as the
 * engine passes them (its generated {@code __bamlDispatch} reshapes them into
 * its SAM), and a {@code java.util.function} shape, which the generator uses
 * only for a callable whose parameters are all positional and at most two,
 * takes exactly its arity.
 */
public enum HostAdapter {
    HOST_CALLABLE(-1, "generated callable interface"),
    FUNCTION(1, "java.util.function.Function"),
    BI_FUNCTION(2, "java.util.function.BiFunction"),
    SUPPLIER(0, "java.util.function.Supplier"),
    CONSUMER(1, "java.util.function.Consumer"),
    BI_CONSUMER(2, "java.util.function.BiConsumer"),
    RUNNABLE(0, "java.lang.Runnable");

    /** The positional arguments the shape takes, or {@code -1} for a generated interface, which takes any. */
    private final int arity;
    private final String javaName;

    HostAdapter(int arity, String javaName) {
        this.arity = arity;
        this.javaName = javaName;
    }

    /**
     * The shape {@code callable} is called through, or {@code null} when it is
     * no host callable. A value of several shapes is called through the first.
     */
    public static HostAdapter of(Object callable) {
        if (callable instanceof BamlHostCallable) {
            return HOST_CALLABLE;
        }
        if (callable instanceof Function<?, ?>) {
            return FUNCTION;
        }
        if (callable instanceof BiFunction<?, ?, ?>) {
            return BI_FUNCTION;
        }
        if (callable instanceof Supplier<?>) {
            return SUPPLIER;
        }
        if (callable instanceof Consumer<?>) {
            return CONSUMER;
        }
        if (callable instanceof BiConsumer<?, ?>) {
            return BI_CONSUMER;
        }
        if (callable instanceof Runnable) {
            return RUNNABLE;
        }
        return null;
    }

    /**
     * Why this shape cannot take a call passing {@code positional} arguments to
     * a callable whose type has the optional parameters {@code optionalNames};
     * {@code null} when it can.
     */
    public String refusal(int positional, String[] optionalNames) {
        if (arity >= 0 && optionalNames.length != 0) {
            return "a " + javaName + " takes no optional arguments, but its type has "
                    + optionalNames.length;
        }
        if (arity >= 0 && arity != positional) {
            return "a " + javaName + " takes " + arity + " arguments, but the engine passed " + positional;
        }
        return null;
    }

    /** Call {@code callable}, of this shape, with the bound arguments. */
    @SuppressWarnings("unchecked")
    public Object invoke(Object callable, List<Object> positional, Map<String, Object> optional) {
        return switch (this) {
            case HOST_CALLABLE -> ((BamlHostCallable) callable).__bamlDispatch(positional, optional);
            case FUNCTION -> ((Function<Object, Object>) callable).apply(positional.get(0));
            case BI_FUNCTION -> ((BiFunction<Object, Object, Object>) callable)
                    .apply(positional.get(0), positional.get(1));
            case SUPPLIER -> ((Supplier<Object>) callable).get();
            case CONSUMER -> {
                ((Consumer<Object>) callable).accept(positional.get(0));
                yield null;
            }
            case BI_CONSUMER -> {
                ((BiConsumer<Object, Object>) callable).accept(positional.get(0), positional.get(1));
                yield null;
            }
            case RUNNABLE -> {
                ((Runnable) callable).run();
                yield null;
            }
        };
    }
}
