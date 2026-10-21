function canThrow(n: int) -> void throws int[] {
    throw [];
}

function returnEmpty() -> int[] {
    return [];
}

test "throw_context_empty_array" {
    let size = canThrow(0) catch (e) {
        int[] => e.length(),
    };
    assert.equal(size, returnEmpty().length())
}

function throwNested() -> void throws map<string, int[]> {
    throw {"empty": []};
}

function throwEmptyMap() -> void throws map<string, int> {
    throw {};
}

test "throw_context_maps" {
    let size = throwNested() catch (e) {
        map<string, int[]> => e["empty"].length(),
    };
    assert.equal(size, 0)
    let caught = throwEmptyMap() catch (e) {
        map<string, int> => true,
    };
    assert.equal(caught, true)
}

type Errors = int[];

function throwAlias() -> void throws Errors {
    throw [];
}

function throwBranch(n: int) -> void throws int[] | string {
    throw if (n == 0) { [] } else { [n] };
}

test "throw_context_alias_union_and_branches" {
    let size = throwBranch(0) catch (e) {
        int[] => e.length(),
        string => -1,
    };
    assert.equal(size, 0)
    let alias = throwAlias() catch (e) {
        int[] => e.length(),
    };
    assert.equal(alias, 0)
}

function throwGeneric<T>() -> void throws T[] {
    throw [];
}

test "throw_context_generic" {
    let size = throwGeneric<string>() catch (e) {
        string[] => e.length(),
    };
    assert.equal(size, 0)
}

test "throw_context_lambdas" {
    let annotated = () -> void throws int[] { throw []; };
    let contextual: () -> void throws string[] = () -> { throw []; };
    let first = annotated() catch (e) {
        int[] => e.length(),
    };
    let second = contextual() catch (e) {
        string[] => e.length(),
    };
    assert.equal(first, 0)
    assert.equal(second, 0)
}

function throwFromCatch() -> void throws int[] {
    { throw ["local"]; } catch (e) {
        string[] => throw [],
    };
}

function throwFromNestedLambda() -> void throws int[] {
    let local = () -> void throws string[] { throw []; };
    local() catch (e) {
        string[] => throw [],
    };
}

test "throw_context_scope_boundaries" {
    let first = throwFromCatch() catch (e) {
        int[] => e.length(),
    };
    let second = throwFromNestedLambda() catch (e) {
        int[] => e.length(),
    };
    assert.equal(first, 0)
    assert.equal(second, 0)
}

function throwLiteral() -> void throws string {
    throw "precise";
}

function throwInferred() -> void {
    throw "precise";
}

function throwOpen() -> void throws int[] | _ {
    throw ["other"];
}

test "throw_context_preserves_inferred_and_open_effects" {
    let literal = throwInferred() catch (e) {
        "precise" => true,
    };
    assert.equal(literal, true)
    let declared = throwLiteral() catch (e) {
        string => e,
    };
    assert.equal(declared, "precise")
    let open = throwOpen() catch (e) {
        string[] => e[0],
        int[] => "wrong",
    };
    assert.equal(open, "other")
}
