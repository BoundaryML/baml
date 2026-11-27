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

class BaseError {
    code: string,
}

class LimitError {
    code: string,
}

type SendError = BaseError | LimitError;

function throwAliasMembers(x: int) -> int throws SendError {
    if (x < 0) {
        throw BaseError { code: "negative" };
    }
    if (x > 9) {
        throw LimitError { code: "limit" };
    }
    x
}

function rethrowAliasMembers(x: int) -> int throws SendError {
    if (x > 99) {
        throw LimitError { code: "far" };
    }
    throwAliasMembers(x) catch (e) {
        _ => {
            throw e;
        },
    }
}

function throwOneAliasMember() -> int throws SendError {
    throw LimitError { code: "one" };
}

// Throws nothing the alias stands for, so E0097 names the alias.
function throwNoAliasMember() -> int throws SendError {
    1
}

test "throw_context_union_alias_members" {
    let base = throwAliasMembers(-1) catch (e) {
        BaseError => e.code,
        LimitError => "wrong",
    };
    assert.equal(base, "negative")
    let limit = rethrowAliasMembers(10) catch (e) {
        BaseError => "wrong",
        LimitError => e.code,
    };
    assert.equal(limit, "limit")
    let far = rethrowAliasMembers(100) catch (e) {
        BaseError => "wrong",
        LimitError => e.code,
    };
    assert.equal(far, "far")
    let one = throwOneAliasMember() catch (e) {
        BaseError => "wrong",
        LimitError => e.code,
    };
    assert.equal(one, "one")
    assert.equal(throwNoAliasMember(), 1)
}
