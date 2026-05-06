# Hand-rolled stand-in for the `baml generate`-emitted inlined source.
# Phase 3 sim only; phase 7 replaces this with the real generator output.

FILES: dict[str, str] = {
    "ns_lorem/root.baml": (
        "class MyLorem {\n"
        "  a: int\n"
        "}\n"
        "\n"
        "function add_three_to_field_a(input_lorem: MyLorem) -> MyLorem {\n"
        "  MyLorem { a: input_lorem.a + 3 }\n"
        "}\n"
    ),
}
