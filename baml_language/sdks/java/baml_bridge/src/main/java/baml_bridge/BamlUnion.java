package baml_bridge;

/**
 * Marker for the generic union family ({@link Union2} … {@link Union10}).
 * Every arm record wraps exactly one {@code value()} component; the inbound
 * encoder unwraps any {@code BamlUnion} instance to that bare inner value
 * (the wire carries no union envelope inbound).
 */
public interface BamlUnion {}
