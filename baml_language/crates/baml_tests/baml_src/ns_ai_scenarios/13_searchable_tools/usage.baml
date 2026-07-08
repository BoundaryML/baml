// scenario 13 — searchable tools: ../../../../../llm-provider/ideas/scenarios/13-searchable-tools/README.md
// A catalog + a resolver that pages in matching tools per turn.

class ToolCatalog {
    tools: baml.ai.Tool[],
}

function search_tools(cat: ToolCatalog, query: string) -> baml.ai.Tool[] throws never {
    cat.tools
        .filter(
            (t: baml.ai.Tool) -> bool {
                t.name.includes(query) || t.description.includes(query)
            },
        )
}

test "13: the catalog resolver pages in only matching tools" {
    let cat = ToolCatalog {
        tools: [
            baml.ai.Tool { name: "get_weather", description: "weather lookups", parameters: "{}" },
            baml.ai.Tool { name: "search_web", description: "web search", parameters: "{}" },
            baml.ai.Tool { name: "get_stock", description: "stock quotes", parameters: "{}" },
        ],
    };
    let hits = search_tools(cat, "weather");
    assert.equal(hits.length(), 1)
    assert.equal(hits[0].name, "get_weather")
}
