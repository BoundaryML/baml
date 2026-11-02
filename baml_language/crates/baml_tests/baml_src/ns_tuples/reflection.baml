// Tuple reflection: building tuple types and values with `reflect.Tuple`,
// rendering the view, and the reflection walkers (render validation,
// definition equivalence, open-interface checks, witness substitution) that
// must visit every slot of a tuple type. Source has no `()` syntax, so
// reflection is the only way to name a zero-length tuple.
//
// Names carry a `tr_` / `Tr` prefix to stay clear of the other namespaces in
// this one-project corpus.

// ─── Construction ────────────────────────────────────────────────────────────

test "tuple_new_builds_slots_in_order" {
    let pair = reflect.Tuple.new([int.type(), string.type()]);
    assert.is_true(pair.as_type() == reflect.Type.of<(int, string)>());
    assert.is_true(pair.as_type() != reflect.Type.of<(string, int)>());
    assert.equal(pair.element_types().length(), 2);
    assert.is_true(pair.element_types()[1] == string.type())
}

test "tuple_new_singleton_stays_a_tuple" {
    let single = reflect.Tuple.new([int.type()]);
    assert.is_true(single.as_type() == reflect.Type.of<(int,)>());
    assert.is_true(single.as_type() != int.type())
}

test "tuple_new_nests_and_accepts_views" {
    let inner = reflect.Tuple.new([int.type()]);
    let outer = reflect.Tuple.new([inner, reflect.Union.new([string.type(), int.type()])]);
    assert.is_true(outer.as_type() == reflect.Type.of<((int,), string | int)>())
}

test "tuple_new_accepts_an_empty_list" {
    let empty = reflect.Tuple.new([]);
    assert.equal(empty.element_types().length(), 0);
    assert.is_true(empty.as_type().as_tuple() != null);
    assert.is_true(empty.as_type() == reflect.Tuple.new([]).as_type());
    assert.is_true(empty.as_type() != reflect.Type.of<null>())
}

// ─── Rendering ───────────────────────────────────────────────────────────────

function tr_view_string() -> string {
    let view: reflect.Tuple = reflect.Type.of<(int, string)>().as_tuple() else {
        return "not a tuple view";
    };
    view.to_string()
}

test "tuple_view_to_string_renders_the_wrapped_type" {
    assert.equal(tr_view_string(), "(int, string)");
    assert.equal(reflect.Tuple.new([int.type()]).to_string(), "(int,)");
    assert.equal(reflect.Tuple.new([]).to_string(), "()");
    assert.equal(string.from(reflect.Tuple.new([int.type()])), "(int,)")
}

test "tuple_to_baml_spells_slots_in_order" {
    assert.equal(
        reflect.Tuple.new([int.type(), string.type().optional()]).to_baml(),
        "type RuntimeType = (int, string?)",
    );
    assert.equal(reflect.Tuple.new([int.type()]).to_baml(), "type RuntimeType = (int,)");
    assert.equal(
        reflect.Tuple.new([reflect.Union.new([int.type(), string.type()])]).array().to_baml(),
        "type RuntimeType = (int | string,)[]",
    )
}

test "tuple_to_baml_spells_nested_runtime_classes" {
    let point = reflect.Class.new("TrPoint", { "x": int.type() });
    let source = reflect.Tuple.new([point, reflect.Tuple.new([point.optional()])]).to_baml();
    assert.equal(source, "class TrPoint {\n  x int\n}\n\ntype RuntimeType = (TrPoint, (TrPoint?,))")
}

test "tuple_kind_matches_the_tuple_arm" {
    let arity = match (reflect.Tuple.new([int.type(), bool.type(), string.type()]).kind()) {
        let tuple: reflect.Tuple => tuple.element_types().length(),
        _ => -1,
    };
    assert.equal(arity, 3)
}

// ─── Values ──────────────────────────────────────────────────────────────────

function tr_instantiated_pair() -> (int, string) {
    reflect.Tuple.instantiate<(int, string)>([1, "a"])
}

test "tuple_instantiate_builds_a_value" {
    assert.is_true(tr_instantiated_pair() == (1, "a"));
    assert.is_true(
        reflect.Type.of_value(tr_instantiated_pair()) == reflect.Type.of<(int, string)>(),
    )
}

// The value carries `T`'s slot types, not ones reconstructed from the data.
function tr_instantiated_wide() -> (string | int,) {
    reflect.Tuple.instantiate<(string | int,)>(["x"])
}

test "tuple_instantiate_carries_the_slot_types" {
    assert.is_true(
        reflect.Type.of_value(tr_instantiated_wide()) == reflect.Type.of<(string | int,)>(),
    )
}

function tr_unit_type() -> reflect.Type {
    reflect.Tuple.new([]).as_type()
}

function tr_unit_rendered() -> string {
    let type Unit = tr_unit_type();
    string.from(reflect.Tuple.instantiate<Unit>([]))
}

function tr_unit_reflected() -> bool {
    let type Unit = tr_unit_type();
    let unit = reflect.Tuple.instantiate<Unit>([]);
    reflect.Type.of_value(unit) == tr_unit_type() && unit == reflect.Tuple.instantiate<Unit>([])
}

test "tuple_instantiate_builds_the_zero_length_tuple" {
    assert.equal(tr_unit_rendered(), "()");
    assert.is_true(tr_unit_reflected())
}

function tr_instantiate_error(values: unknown[]) -> string {
    reflect.Tuple.instantiate<(int, string)>(values) catch (e) {
        reflect.errors.InvalidArgument { message } => {
            return message;
        }
    };
    "no error"
}

function tr_instantiate_non_tuple() -> string {
    reflect.Tuple.instantiate<int[]>([1]) catch (e) {
        reflect.errors.InvalidArgument { message } => {
            return message;
        }
    };
    "no error"
}

test "tuple_instantiate_rejects_bad_values" {
    assert.equal(
        tr_instantiate_error([1]),
        "reflect.Tuple.instantiate: `(int, string)` has 2 slots, got 1 values",
    );
    assert.equal(
        tr_instantiate_error([1, 2]),
        "reflect.Tuple.instantiate: slot 1 of `(int, string)` expects `string`, got `int`",
    );
    assert.equal(
        tr_instantiate_non_tuple(),
        "reflect.Tuple.instantiate expects a tuple type, got `int[]`",
    )
}

// A tuple's type is carried, so reflecting it never walks its elements: type
// values held in slots stay values, and a cycle through a mutable list
// terminates.
function tr_holds_types() -> bool {
    let held = (int.type(), (reflect.Tuple.new([]).as_type(),));
    reflect.Type.of_value(held) == reflect.Type.of<(reflect.Type, (reflect.Type,))>()
        && held[1][0] == reflect.Tuple.new([]).as_type()
}

function tr_cyclic() -> bool {
    let items: unknown[] = [];
    let wrapped = (items, 1);
    items.push(wrapped);
    reflect.Type.of_value(wrapped) == reflect.Type.of<(unknown[], int)>()
}

test "tuple_value_reflection_does_not_walk_slots" {
    assert.is_true(tr_holds_types());
    assert.is_true(tr_cyclic())
}

// ─── Interface witnesses ─────────────────────────────────────────────────────

interface TrPair<T> {
    pair: (T, int),
}

interface TrNestedPair<T> {
    pair: ((T,), T[]),
}

test "witness_substitutes_type_parameters_inside_tuple_fields" {
    let witness = reflect.Interface.implementation<TrPair<string>>().field("pair");
    let holder = reflect.Class.new(
        "TrPairHolder",
        { "pair": reflect.Tuple.new([string.type(), int.type()]) },
        implementations = [witness],
    );
    assert.is_true(holder.implements(reflect.Type.of<TrPair<string>>()));

    let nested = reflect.Interface.implementation<TrNestedPair<int>>().field("pair");
    let nested_holder = reflect.Class.new(
        "TrNestedHolder",
        { "pair": reflect.Type.of<((int,), int[])>() },
        implementations = [nested],
    );
    assert.is_true(nested_holder.implements(reflect.Type.of<TrNestedPair<int>>()))
}

test "witness_rejects_a_mismatched_tuple_field" {
    let witness = reflect.Interface.implementation<TrPair<string>>().field("pair");
    let result = reflect.Class.new(
        "TrPairMismatch",
        { "pair": reflect.Tuple.new([int.type(), int.type()]) },
        implementations = [witness],
    ) catch (e) {
        reflect.errors.CompilationError => {
            null
        }
    };
    assert.is_true(result == null)
}

// ─── Render validation through tuple slots ───────────────────────────────────

client TrClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

function TrRender<T>() -> T {
    client: TrClient
    prompt: `${ctx.output_format()}`
}

test "non_equivalent_same_name_classes_in_tuple_slots_fail_before_render" {
    let left = reflect.Class.new("TrCollision", { "left": int.type() });
    let right = reflect.Class.new("TrCollision", { "right": string.type() });
    let type Combined = reflect.Tuple.new([left, reflect.Tuple.new([right])]);
    assert.equal(
        TrRender@render_prompt<Combined>().text() catch (e) {
            reflect.errors.CompilationError => {
                `${e.diagnostics[0].code}|${e.diagnostics[0].message}`
            },
            _ => "wrong render error",
        },
        "E0162|type `TrCollision` has non-equivalent definitions in the same LLM render context",
    )
}

test "equivalent_same_name_classes_behind_tuple_fields_still_render" {
    let left_inner = reflect.Class.new("TrInner", { "value": int.type() });
    let right_inner = reflect.Class.new("TrInner", { "value": int.type() });
    let left = reflect.Class.new("TrOuter", { "pair": reflect.Tuple.new([left_inner, int.type()]) });
    let right = reflect.Class.new("TrOuter", { "pair": reflect.Tuple.new([right_inner, int.type()]) });
    let type Both = reflect.Union.new([left, right]);
    let rendered = TrRender@render_prompt<Both>().text() catch (e) {
        reflect.errors.CompilationError => {
            `${e.diagnostics[0].code}|${e.diagnostics[0].message}`
        },
        _ => "wrong render error",
    };
    assert.is_true(rendered.includes("pair"));
    assert.is_true(!rendered.includes("E0162"))
}

interface TrOpen {
    value: string,
}

test "open_interface_in_a_tuple_slot_is_rejected" {
    let holder = reflect.Class.new(
        "TrOpenHolder",
        { "pair": reflect.Tuple.new([int.type(), reflect.Tuple.new([reflect.Type.of<TrOpen>()])]) },
    );
    let type Holder = holder;
    let rendered = TrRender@render_prompt<Holder>().text() catch (e) {
        reflect.errors.CompilationError => {
            `${e.diagnostics[0].code}|${e.diagnostics[0].message}`
        },
        _ => "wrong render error",
    };
    assert.is_true(
        rendered.starts_with("E0161|field `TrOpenHolder.pair` has open interface type `"),
    );
    assert.is_true(rendered.includes("TrOpen`, which cannot be rendered"))
}
