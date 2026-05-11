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
        "\n"
        "function default_score(query: string, max_results: int = 10, filter: string? = null) -> int {\n"
        "  if (filter == null) {\n"
        "    max_results\n"
        "  } else {\n"
        "    max_results + 100\n"
        "  }\n"
        "}\n"
        "\n"
        "function mutate_default(items: int[] = []) -> int {\n"
        "  items.push(1)\n"
        "  items.length()\n"
        "}\n"
    ),
}
