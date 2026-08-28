package baml_bridge;

/** Semantic projection of an authored BAML function. */
public enum BamlFunctionOperation {
    DIRECT(0),
    SPEC(1),
    STREAM(2);

    private final int wireValue;

    BamlFunctionOperation(int wireValue) {
        this.wireValue = wireValue;
    }

    public int wireValue() {
        return wireValue;
    }
}
