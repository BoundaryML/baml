// Schema-aligned parsing of a list.
//
// A list the model wrote holds only values of its element type. An item that
// does not fit fails the parse, and the error names the item and says why. A
// shorter list, or an empty one, would read as a good reply: an agent whose
// model named a tool outside the allowed ones would get `actions: []`.
//
// Four cases still leave an item out without an error: an item that is still
// arriving (a partial parse), values the parser gathered from the text itself
// when the model wrote no list, `null` where a list is expected, and a list of
// media (media never comes from text).
//
// Per the corpus convention every case body is a top-level `function` and the
// `test` block only asserts.

class SapListSearch {
    kind: "search",
    query: string,
}

class SapListCalculate {
    kind: "calculate",
    expression: string,
}

class SapListAction {
    reason: string,
    tool: SapListSearch | SapListCalculate,
}

class SapListReply {
    actions: SapListAction[],
}

class SapListPoint {
    x: int,
}

class SapListTagged {
    tags: string[],
    scores: int[],
}

/// `text` parsed as `int[]` and written as JSON, or the parse error's message.
function sap_list_ints(text: string) -> string {
    baml.json.to_string(baml.sap.parse<int[]>(text)) catch (e) {
        baml.errors.ParseError => `ParseError: ${e.message}`,
    }
}

function sap_list_nested_ints(text: string) -> string {
    baml.json.to_string(baml.sap.parse<int[][]>(text)) catch (e) {
        baml.errors.ParseError => `ParseError: ${e.message}`,
    }
}

function sap_list_points(text: string) -> string {
    baml.json.to_string(baml.sap.parse<SapListPoint[]>(text)) catch (e) {
        baml.errors.ParseError => `ParseError: ${e.message}`,
    }
}

function sap_list_tagged(text: string) -> string {
    baml.json.to_string(baml.sap.parse<SapListTagged>(text)) catch (e) {
        baml.errors.ParseError => `ParseError: ${e.message}`,
    }
}

/// The tools of the actions in `text`, or the parse error's message.
function sap_list_reply_tools(text: string) -> string {
    let reply = baml.sap.parse<SapListReply>(text) catch (e) {
        baml.errors.ParseError => {
            return `ParseError: ${e.message}`;
        },
    };
    reply.actions
        .map((action) -> string {
            match (action.tool) {
                let search: SapListSearch => `search ${search.query}`,
                let calculate: SapListCalculate => `calculate ${calculate.expression}`,
            }
        })
        .join(",")
}

/// What a stream shows for the part of a reply it has received: the partial
/// list, or `no yield` when that part does not parse.
function sap_list_partial_ints(received: string) -> string {
    let cache = baml.sap._new_parse_cache<int[]>();
    let parsed: int[] | baml.sap._NoYield = cache._parse_partial(received);
    match (parsed) {
        baml.sap._NoYield => "no yield",
        let items: int[] => baml.json.to_string(items),
    }
}

test "a list whose items all fit parses" {
    assert.equal(sap_list_ints(`[1, 2, 3]`), "[1,2,3]");
    assert.equal(sap_list_ints(`[]`), "[]")
}

test "an item that does not fit its element type is a parse error that names it" {
    let failed = sap_list_ints(`[1, "two", 3]`);
    assert.is_true(failed.starts_with("ParseError: "));
    assert.is_true(failed.includes("Failed to parse list item 1 as int"));
    assert.is_true(failed.includes("Expected int, got String(\"two\""));
    assert.is_true(sap_list_ints(`[1, null, 3]`).includes("Failed to parse list item 1 as int"))
}

test "an item of a nested list that does not fit fails the outer list" {
    assert.equal(sap_list_nested_ints(`[[1, 2], [3]]`), "[[1,2],[3]]");
    let failed = sap_list_nested_ints(`[[1, 2], [3, "x"]]`);
    assert.is_true(failed.includes("Failed to parse list item 1 as int[]"));
    assert.is_true(failed.includes("Failed to parse list item 1 as int"))
}

test "an action for a tool outside the union fails the reply, it is not dropped" {
    let known = `{"reason": "look it up", "tool": {"kind": "search", "query": "weather"}}`;
    let sum = `{"reason": "add", "tool": {"kind": "calculate", "expression": "1 + 1"}}`;
    let outside = `{"reason": "open it", "tool": {"kind": "browse", "url": "https://example.com"}}`;
    assert.equal(sap_list_reply_tools(`{"actions": [${known}, ${sum}]}`), "search weather,calculate 1 + 1");
    assert.equal(sap_list_reply_tools(`{"actions": []}`), "");
    let only = sap_list_reply_tools(`{"actions": [${outside}]}`);
    assert.is_true(only.starts_with("ParseError: "));
    assert.is_true(only.includes("actions: Failed to parse list item 0 as"));
    let second = sap_list_reply_tools(`{"actions": [${known}, ${outside}]}`);
    assert.is_true(second.includes("actions: Failed to parse list item 1 as"));
    assert.is_true(second.includes("Expected literal[\"search\"], got String(\"browse\""))
}

test "one value where a list is expected is a list of it only when it fits" {
    let known = `{"reason": "look it up", "tool": {"kind": "search", "query": "weather"}}`;
    let outside = `{"reason": "open it", "tool": {"kind": "browse", "url": "https://example.com"}}`;
    assert.equal(sap_list_ints(`3`), "[3]");
    assert.equal(sap_list_reply_tools(`{"actions": ${known}}`), "search weather");
    let word = sap_list_ints(`two`);
    assert.is_true(word.starts_with("ParseError: "));
    assert.is_true(word.includes("Expected int[], got String(\"two\""));
    assert.is_true(sap_list_reply_tools(`{"actions": ${outside}}`).starts_with("ParseError: "))
}

test "null and a missing field read as an empty list" {
    assert.equal(sap_list_ints(`null`), "[]");
    assert.equal(sap_list_tagged(`{"tags": null}`), `{"tags":[],"scores":[]}`);
    assert.is_true(sap_list_tagged(`{"tags": ["a"], "scores": [1, "x"]}`).includes("scores: Failed to parse list item 1 as int"))
}

test "values the parser gathers from the text keep what fits" {
    assert.equal(
        sap_list_points(`One: {"x": 1} and two: {"x": 2} and another thing: {"y": 3}`),
        `[{"x":1},{"x":2}]`,
    );
    assert.is_true(sap_list_points(`[{"x": 1}, {"x": 2}, {"y": 3}]`).includes("Failed to parse list item 2 as"))
}

test "a partial parse leaves out the item that is still arriving and fails on a complete one" {
    assert.equal(sap_list_partial_ints(`[1, 2`), "[1]");
    assert.equal(sap_list_partial_ints(`[1, 2, `), "[1,2]");
    assert.equal(sap_list_partial_ints(`[1, "two", 3`), "no yield")
}
