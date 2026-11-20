function long_header(total: int?, cached: int?, cache_write: int? = null) -> int? throws never {
    1
}

function union_return(x: int) -> LongTypeNameAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA | LongTypeNameBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB {
    x
}

function commented(x: int) -> // c
    A | B {
    x
}

type FT = (a: int) -> int throws LongExceptionNameNumberOne | LongExceptionNameNumberTwo | LongExceptionNameThree;

function f(items: int[], parsed: map<string, int>?, y: int) -> int {
    let g = (x: int) -> LongTypeNameAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA | LongTypeNameBBBBBBBBBBBBBBBBBBBBBBBBBBBB { x };
    if let w: int = items.find((entry) -> bool { entry > 1000000000 && entry < 2000000000 && entry != 1500000000 }) {
        1;
    }
    if let x = match (y) { 1 => 2, _ => 3 } {
        1;
    }
    if let value: aws.internal.ReasoningContentBlockDelta = e.delta?.reasoningContent?.text?.first {
        1;
    }
    let object: map<string, baml.json.json_value_with_a_long_name_xxxxxx> = parsed_value_longer else {
        return 1;
    };
    let type U extends Foo & Bar = LongTypeNameAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA | LongTypeNameBBBBBBBBBBBBB;
    1
}
