test "enumerate is lazy, restarts its counter, and preserves nullable slots" {
    let values: string?[] = ["a", null];
    let enumerated = values.enumerate();
    values.push("c");
    let pairs = enumerated.collect();
    assert.equal(pairs, [(0, "a"), (1, null), (2, "c")]);
    assert.is_true(reflect.Type.of_value(pairs[0]) == reflect.Type.of<(int, string?)>());
    assert.equal(values.enumerate().take(1).collect(), [(0, "a")]);
    assert.equal("ab".enumerate().collect(), [(0, "a"), (1, "b")]);
    let cursor = [4, 5, 6].iter();
    cursor.next();
    assert.equal(cursor.enumerate().collect(), [(0, 5), (1, 6)]);
    let empty: int[] = [];
    assert.equal(empty.enumerate().collect().length(), 0)
}

test "enumerate errors do not spend a position" {
    let cursor = FlakyIterator.new([10, 20], 1).enumerate();
    assert.equal(cursor.next(), (0, 10));
    let caught = false;
    {
        cursor.next();
    } catch (e) {
        baml.errors.InvalidArgument => {
            caught = true;
        }
    };
    assert.is_true(caught);
    assert.equal(cursor.collect(), [(1, 20)])
}

test "zip is lazy and stops at the shorter side" {
    let left = [1, 2, 3];
    let right: string?[] = ["a", null];
    let pairs = left.zip(right);
    left[0] = 10;
    assert.equal(pairs.collect(), [(10, "a"), (2, null)]);
    let empty: int[] = [];
    assert.equal(empty.zip(right).collect().length(), 0);
    assert.equal(left.zip(empty).collect().length(), 0);
    assert.equal([1].zip(["a", "b"]).collect(), [(1, "a")])
}

test "zip retains its left element across a right error" {
    let left = [1, 2, 3].iter();
    let pairs = left.zip(FlakyIterator.new([10, 20], 0));
    let caught = false;
    {
        pairs.next();
    } catch (e) {
        baml.errors.InvalidArgument => {
            caught = true;
        }
    };
    assert.is_true(caught);
    assert.equal(pairs.collect(), [(1, 10), (2, 20)]);
    assert.is_true(left.next() is baml.iter.Done)
}

test "zip propagates left errors before pulling the right side" {
    let right = [10, 20].iter();
    let pairs = FlakyIterator.new([1, 2], 0).zip(right);
    let caught = false;
    {
        pairs.next();
    } catch (e) {
        baml.errors.InvalidArgument => {
            caught = true;
        }
    };
    assert.is_true(caught);
    assert.equal(pairs.collect(), [(1, 10), (2, 20)])
}

test "zip stays done after either non-fused source ends" {
    let left_ends = ResumableIterator.new([1, 2], 1).zip([10, 20]);
    assert.equal(left_ends.next(), (1, 10));
    assert.is_true(left_ends.next() is baml.iter.Done);
    assert.is_true(left_ends.next() is baml.iter.Done);
    let right_ends = [1, 2, 3].zip(ResumableIterator.new([10, 20], 1));
    assert.equal(right_ends.next(), (1, 10));
    assert.is_true(right_ends.next() is baml.iter.Done);
    assert.is_true(right_ends.next() is baml.iter.Done)
}

test "unzip preserves order, empty types, and nullable values" {
    let pairs: (string, int?)[] = [("a", null), ("b", 2)];
    let (let keys, let values) = baml.iter.unzip(pairs);
    assert.equal(keys, ["a", "b"]);
    assert.equal(values, [null, 2]);
    assert.is_true(reflect.Type.of_value(values) == reflect.Type.of<int?[]>());
    assert.equal(baml.iter.unzip([1, 2].zip(["a", "b"])), ([1, 2], ["a", "b"]));
    let empty: (string, int)[] = [];
    let result = baml.iter.unzip(empty);
    assert.equal(result.0.length(), 0);
    assert.is_true(reflect.Type.of_value(result) == reflect.Type.of<(string[], int[])>())
}

test "partition calls its predicate once and preserves order and nullable types" {
    let values: int?[] = [1, null, 2, null, 3];
    let calls = 0;
    let (let accepted, let rejected) = values.iter().partition((value: int?) -> bool {
        calls += 1;
        value != null
    });
    assert.equal(accepted, [1, 2, 3]);
    assert.equal(rejected, [null, null]);
    assert.equal(calls, 5);
    assert.is_true(reflect.Type.of_value(accepted) == reflect.Type.of<int?[]>());
    let empty: int[] = [];
    assert.equal(
        empty.iter().partition((x: int) -> bool {
            true
        }),
        ([], []),
    )
}

test "tuple consumers propagate source and predicate errors" {
    let caught = 0;
    {
        baml.iter.unzip(FlakyIterator.new([1], 0).enumerate());
    } catch (e) {
        baml.errors.InvalidArgument => {
            caught += 1;
        }
    };
    {
        FlakyIterator.new([1], 0).partition((x: int) -> bool {
            true
        });
    } catch (e) {
        baml.errors.InvalidArgument => {
            caught += 1;
        }
    };
    {
        [1].iter().partition((x: int) -> bool throws baml.errors.InvalidArgument {
            throw baml.errors.InvalidArgument { message: "predicate" }
        });
    } catch (e) {
        baml.errors.InvalidArgument => {
            caught += 1;
        }
    };
    assert.equal(caught, 3)
}

test "tuple item inference normalizes inherited associated types" {
    root.compiler.AssertCompiles(
        {
            "inherited.baml": `
                interface Source { type Item; }
                interface Derived requires Source<Item = Self.Item> { type Item; }
                function take_pair<A, B>(source: Source<Item = (A, B)>) -> int throws never { 0 }
                function existential(source: Derived<Item = (int, string)>) -> int throws never {
                    take_pair(source)
                }
                function bounded<S extends Derived<Item = (int, string)>>(source: S) -> int throws never {
                    take_pair(source)
                }
            `,
        },
    )
}

test "zip retains nullable left slots and both source error types" {
    let left: int?[] = [null, 1];
    let pairs = left.zip(["a", "b"]).collect();
    assert.equal(pairs, [(null, "a"), (1, "b")]);
    assert.is_true(reflect.Type.of_value(pairs[0]) == reflect.Type.of<(int?, string)>());
    let errors = [1].iter()
        .map((x: int) -> int throws string {
            throw "left"
        })
        .zip(FlakyIterator.new([10], 0));
    let caught = false;
    {
        errors.next();
    } catch (e) {
        string => {
            caught = true;
        },
        baml.errors.InvalidArgument => {},
    };
    assert.is_true(caught)
}
