function wrongElement() -> void throws int[] {
    throw ["wrong"];
}

function wrongEffect() -> void throws int[] {
    throw "wrong";
}

function ambiguous() -> void throws int[] | string[] {
    throw [];
}

function unannotated() -> void {
    throw [];
}

function openClause() -> void throws int[] | _ {
    throw [];
}

function isolatedCatch() -> void throws int[] {
    { throw []; } catch (e) {
        _ => throw [1],
    };
}

function isolatedLambda() -> void throws int[] {
    let f = () -> { throw []; };
    throw [1];
}

function wrongExplicitMapValue() -> void throws map<string, int> {
    throw map {value: "wrong"};
}

function wrongImplicitMapValue() -> void throws map<string, int> {
    throw {value: "wrong"};
}
