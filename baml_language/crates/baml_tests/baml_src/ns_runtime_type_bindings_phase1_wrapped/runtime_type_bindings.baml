// Native ports of Phase-1 Rust integration tests.

function source_1() -> string {
    `
        function main() -> bool {
            let pkg = reflect.Package.compile({ "items.baml": \`
                class Item { value string }
                function Items() -> Item[] { [Item { value: "bound" }] }
            \` })
            let item_ct = pkg.get_class("root.Item") ?? throw "missing Item"
            let binding_evaluations: int = 0
            let operand = () -> {
                binding_evaluations += 1
                item_ct.as_type()
            }

            let escaped: unknown = {
                type T = unreflect(operand());
                if (reflect.Type.of<T>() != item_ct.as_type()) {
                    throw "reflect.Type.of<T>() did not preserve the bound value"
                }
                let get_items = pkg.get_function<() -> T[]>("root.Items")
                    ?? throw "missing root.Items"
                let items: T[] = get_items()
                let item: T = items[0]
                if (reflect.Type.of_value(item) != reflect.Type.of<T>()) {
                    throw "typed result did not retain T"
                }
                item
            }

            binding_evaluations == 1
                && reflect.Type.of_value(escaped) == item_ct.as_type()
        }
    `
}

function source_2() -> string {
    `
        function main() -> bool {
            let string_t = reflect.Type.of<string>()
            let int_t = reflect.Type.of<int>()
            let check = (bound: reflect.Type) -> {
                type T = unreflect(bound);
                reflect.Type.of<T>() == bound
            }

            type T = unreflect(string_t);
            let inner = {
                type T = unreflect(int_t);
                reflect.Type.of<T>() == int_t
            }
            reflect.Type.of<T>() == string_t && inner && check(int_t)
        }
    `
}

function source_3() -> string {
    `
        interface Parser<Out> {
            function parse_it(self, text: string) -> Out throws unknown
            function describe(self) -> string throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Parser<T> {
                function parse_it(self, text: string) -> T {
                    baml.sap.parse<T>(text)
                }

                function describe(self) -> string throws never {
                    reflect.Type.of<T>().to_string()
                }
            }
        }

        function main() -> string {
            let output_type = reflect.class.new("RuntimeOutput", {
                "name": reflect.Type.of<string>(),
            }).as_type()
            type Out = unreflect(output_type)

            let holder = Holder<Out>.new()
            let parsed = holder.parse_it(\`{"name":"Pixel"}\`)
            \`\${holder.describe()}|\${reflect.class.get_field<string>(parsed, "name")}\`
        }
    `
}

function source_4() -> string {
    `
        client DefaultClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        class ProbeClient {
            reply: string,
            requests: string[],

            implements ai.Client {
                function id(self) -> string {
                    "probe"
                }

                function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
                    let prompt = input.prompt(ai.internal.build_output_format(input.output_type)).text();
                    let _ = self.requests.push(prompt);
                    baml.http.Request { method: "POST", url: "https://probe.invalid", headers: {}, body: prompt }
                }

                function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
                    let _ = self.render(input);
                    ai.ModelTurn {
                        calls: [],
                        content: [ai.content.Text { text: self.reply }],
                        stop_reason: ai.content.StopReason.Complete,
                        usage: null,
                    }
                }
            }
        }

        function DynamicOutput<T>() -> T {
            client: DefaultClient
            prompt: \`\${ctx.output_format()}\`
        }

        function main() -> string {
            let output_type = reflect.class.new("RuntimeOutput", {
                "name": reflect.Type.of<string>(),
            }).as_type()
            type Out = unreflect(output_type)

            // Control: the direct SAP call has always worked.
            let direct = baml.sap.parse<Out>(\`{"name":"Pixel"}\`)

            let run = ai.Agent.new(
                client = ProbeClient { reply: \`{"name":"Pixel"}\`, requests: [] },
            ).run(DynamicOutput@spec<Out>())

            let direct_name = reflect.class.get_field<string>(direct, "name")
            let agent_name = reflect.class.get_field<string>(run.value, "name")
            \`\${direct_name}|\${agent_name}\`
        }
    `
}

function source_5() -> string {
    `
        client DefaultClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        class ProbeClient {
            reply: string,
            requests: string[],

            implements ai.Client {
                function id(self) -> string {
                    "probe"
                }

                function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
                    let prompt = input.prompt(ai.internal.build_output_format(input.output_type)).text();
                    let _ = self.requests.push(prompt);
                    baml.http.Request { method: "POST", url: "https://probe.invalid", headers: {}, body: prompt }
                }

                function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
                    let _ = self.render(input);
                    ai.ModelTurn {
                        calls: [],
                        content: [ai.content.Text { text: self.reply }],
                        stop_reason: ai.content.StopReason.Complete,
                        usage: null,
                    }
                }
            }
        }

        function DynamicOutput<T>() -> T {
            client: DefaultClient
            prompt: \`\${ctx.output_format()}\`
        }

        function main() -> string {
            let minted_class = reflect.class.new("MintedRecord", {
                "minted_field": reflect.Type.of<string>().meta(
                    docstring = "Minted runner field docs",
                ),
            }).as_type()
            type MintedRecord = unreflect(minted_class)
            let class_client = ProbeClient { reply: \`{"minted_field":"ok"}\`, requests: [] }
            let _ = ai.Agent.new(
                client = class_client,
            ).run(DynamicOutput@spec<MintedRecord>())

            let minted_enum = reflect.enum.new(
                "MintedChoice",
                ["PENDING", "RUNNING"],
            ).as_type()
            type MintedChoice = unreflect(minted_enum)
            let enum_client = ProbeClient { reply: \`"RUNNING"\`, requests: [] }
            let _ = ai.Agent.new(
                client = enum_client,
            ).run(DynamicOutput@spec<MintedChoice>())

            let alpha = reflect.class.new("AlphaShape", {
                "alpha_only": reflect.Type.of<string>(),
            }).as_type()
            let beta = reflect.class.new("BetaShape", {
                "beta_only": reflect.Type.of<int>(),
            }).as_type()
            let union = reflect.union.new([alpha, beta]).as_type()
            type MintedUnion = unreflect(union)
            let union_client = ProbeClient { reply: \`{"alpha_only":"ok"}\`, requests: [] }
            let _ = ai.Agent.new(
                client = union_client,
            ).run(DynamicOutput@spec<MintedUnion>())

            let package = reflect.Package.compile({ "compiled.baml": \`
                class CompiledRecord { compiled_field string }
            \` })
            let compiled = (package.get_class("root.CompiledRecord")
                ?? throw "missing CompiledRecord").as_type()
            type CompiledRecord = unreflect(compiled)
            let compiled_client = ProbeClient { reply: \`{"compiled_field":"ok"}\`, requests: [] }
            let _ = ai.Agent.new(
                client = compiled_client,
            ).run(DynamicOutput@spec<CompiledRecord>())

            let class_prompt = class_client.requests.at(0) ?? throw "missing class request"
            let enum_prompt = enum_client.requests.at(0) ?? throw "missing enum request"
            let union_prompt = union_client.requests.at(0) ?? throw "missing union request"
            let compiled_prompt = compiled_client.requests.at(0) ?? throw "missing compiled request"
            \`\${class_prompt}~~\${enum_prompt}~~\${union_prompt}~~\${compiled_prompt}\`
        }
    `
}

function source_6() -> string {
    `
        interface Probe<Out> {
            function same(self, t: reflect.Type) -> bool throws never
            function same_from_default(self, t: reflect.Type) -> bool throws never {
                reflect.Type.of<Out>() == t
            }
        }

        interface Relay<Out> {
            function relay(self, t: reflect.Type) -> bool throws unknown
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function same(self, t: reflect.Type) -> bool throws never {
                    reflect.Type.of<T>() == t
                }
            }

            implements Relay<T> {
                function relay(self, t: reflect.Type) -> bool {
                    // Two-hop: the second interface operand is materialized
                    // inside this frame, so it has to carry the identity on.
                    Holder<T>.new().same(t)
                }
            }
        }

        function main() -> string {
            let output_type = reflect.class.new("RuntimeOutput", {
                "name": reflect.Type.of<string>(),
            }).as_type()
            type Out = unreflect(output_type)

            let holder = Holder<Out>.new()
            let impl_method = holder.same(output_type)
            let default_method = holder.same_from_default(output_type)
            let two_hop = holder.relay(output_type)
            \`\${impl_method}|\${default_method}|\${two_hop}\`
        }
    `
}

function source_7() -> string {
    `
        class Entry {
            key: reflect.Type,
            label: string,
        }

        interface Named<Out> {
            function label(self, first: Entry, second: Entry) -> string throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Named<T> {
                function label(self, first: Entry, second: Entry) -> string throws never {
                    let wanted = reflect.Type.of<T>()
                    if (first.key == wanted) {
                        first.label
                    } else if (second.key == wanted) {
                        second.label
                    } else {
                        "missing"
                    }
                }
            }
        }

        function main() -> string {
            let first_type = reflect.class.new("Shape", {
                "name": reflect.Type.of<string>(),
            }).as_type()
            let second_type = reflect.class.new("Shape", {
                "name": reflect.Type.of<string>(),
            }).as_type()
            type First = unreflect(first_type)
            type Second = unreflect(second_type)

            let first_entry = Entry { key: first_type, label: "first" }
            let second_entry = Entry { key: second_type, label: "second" }

            let first_hit = Holder<First>.new().label(first_entry, second_entry)
            let second_hit = Holder<Second>.new().label(first_entry, second_entry)
            \`\${first_hit}|\${second_hit}\`
        }
    `
}

function source_8() -> string {
    `
        interface Probe<Out> {
            function same(self, t: reflect.Type) -> bool throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function same(self, t: reflect.Type) -> bool throws never {
                    reflect.Type.of<T>() == t
                }
            }
        }

        function main() -> string {
            let mine = reflect.class.new("Shape", {
                "name": reflect.Type.of<string>(),
            }).as_type()
            let other = reflect.class.new("Shape", {
                "name": reflect.Type.of<string>(),
            }).as_type()
            type Mine = unreflect(mine)

            let holder = Holder<Mine>.new()
            let own_mint = holder.same(mine)
            let foreign_mint = holder.same(other)

            let static_holder = Holder<string>.new()
            let static_match = static_holder.same(reflect.Type.of<string>())
            let static_miss = static_holder.same(reflect.Type.of<int>())
            \`\${own_mint}|\${foreign_mint}|\${static_match}|\${static_miss}\`
        }
    `
}

function source_9() -> string {
    `
        interface Probe<Out> {
            function same(self, t: reflect.Type) -> bool throws never
            function name(self) -> string throws never {
                reflect.Type.of<Out>().to_string()
            }
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function same(self, t: reflect.Type) -> bool throws never {
                    reflect.Type.of<T>() == t
                }
            }
        }

        function main() -> string {
            let pkg = reflect.Package.compile({ "items.baml": \`
                class Item { value string }
            \` })
            let item_type = (pkg.get_class("root.Item") ?? throw "missing Item").as_type()
            type Item = unreflect(item_type)

            let holder = Holder<Item>.new()
            \`\${holder.same(item_type)}|\${holder.name()}\`
        }
    `
}

function source_10() -> string {
    `
        class Item {
            value: string,
        }

        interface Probe<Out> {
            function same(self, t: reflect.Type) -> bool throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function same(self, t: reflect.Type) -> bool throws never {
                    reflect.Type.of<T>() == t
                }
            }
        }

        function main() -> string {
            let pkg = reflect.Package.compile({ "items.baml": \`
                class Item { value string }
            \` })
            let runtime_item = (pkg.get_class("root.Item") ?? throw "missing Item").as_type()
            // Binding it merges \`user.Item\` into this frame's overlay, which is
            // what every type materialized here from now on carries.
            type Shadow = unreflect(runtime_item)
            let _shadow_holder = Holder<Shadow>.new()

            let holder = Holder<Item>.new()
            \`\${holder.same(reflect.Type.of<Item>())}|\${holder.same(runtime_item)}\`
        }
    `
}

function source_11() -> string {
    `
        interface Probe<Out> {
            function same(self, t: reflect.Type) -> bool throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function same(self, t: reflect.Type) -> bool throws never {
                    reflect.Type.of<T>() == t
                }
            }
        }

        function main() -> string {
            let first = reflect.Package.compile({ "a.baml": \`
                class Item { value string }
            \` })
            let second = reflect.Package.compile({ "b.baml": \`
                class Item { value string }
            \` })
            let first_item = (first.get_class("root.Item") ?? throw "missing A").as_type()
            let second_item = (second.get_class("root.Item") ?? throw "missing B").as_type()
            type First = unreflect(first_item)
            type Second = unreflect(second_item)

            let _first_holder = Holder<First>.new()
            let holder = Holder<Second>.new()
            // The holder's own declaration answers true — the slot carries its
            // head. The failure this guards is the SECOND value being \`true\`:
            // answering with a same-named foreign package's identity.
            \`\${holder.same(second_item)}|\${holder.same(first_item)}\`
        }
    `
}

function source_12() -> string {
    `
        interface Probe<Out> {
            function pair<M>(self, own: reflect.Type, method: reflect.Type) -> string throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function pair<M>(self, own: reflect.Type, method: reflect.Type) -> string throws never {
                    \`\${reflect.Type.of<T>() == own}|\${reflect.Type.of<M>() == method}|\${reflect.Type.of<T>() == method}\`
                }
            }
        }

        function main() -> string {
            let owner_type = reflect.class.new("Owner", {
                "name": reflect.Type.of<string>(),
            }).as_type()
            let method_type = reflect.class.new("Method", {
                "name": reflect.Type.of<string>(),
            }).as_type()
            type Owner = unreflect(owner_type)
            type Method = unreflect(method_type)

            Holder<Owner>.new().pair<Method>(owner_type, method_type)
        }
    `
}

function source_13() -> string {
    `
        interface Probe<Out> {
            function same(self, t: reflect.Type) -> bool throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function same(self, t: reflect.Type) -> bool throws never {
                    reflect.Type.of<T>() == t
                }
            }
        }

        function main() -> string {
            let choice = reflect.enum.new("Choice", ["FIRST", "SECOND"]).as_type()
            let other = reflect.enum.new("Choice", ["FIRST", "SECOND"]).as_type()
            type Choice = unreflect(choice)

            let holder = Holder<Choice>.new()
            \`\${holder.same(choice)}|\${holder.same(other)}\`
        }
    `
}

function source_14() -> string {
    `
        client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        function Render<T>() -> T {
            client: TestClient
            prompt: \`\${ctx.output_format()}\`
        }

        function main() -> string {
            // \`next\` makes \`Item\` recursive, which forces the LLM schema to
            // hoist it under a *name* rather than inline its shape.
            let pkg = reflect.Package.compile({ "items.baml": \`
                class Item { value string, next Item? }
                function Items() -> Item[] { [Item { value: "bound", next: null }] }
            \` })
            let item_type = (pkg.get_class("root.Item") ?? throw "missing Item").as_type()
            type Item = unreflect(item_type)

            // A diagnostic that has to name the type it rejected.
            let contract = pkg.get_function<() -> Item>("root.Items") catch (e) {
                reflect.errors.CompilationError => e.diagnostics[0].message,
                _ => "wrong error",
            }
            let diagnostic = if contract is string { contract } else { "no diagnostic" }

            // \`~~\` and not \`|\`: \`to_baml()\` and the LLM schema both spell a
            // union with \`|\`, so a \`|\` join could split *inside* a surface and
            // leave the assertions below silently comparing the wrong text.
            let schema = Render@render_prompt<Item[]>()
            \`\${item_type.to_string()}~~\${item_type.to_baml()}~~\${schema}~~\${diagnostic}\`
        }
    `
}

function source_15() -> string {
    `
        function main() -> string {
            let first = reflect.Package.compile({ "a.baml": \`
                class Item { value string }
                function Make() -> Item { Item { value: "a" } }
            \` })
            let second = reflect.Package.compile({ "b.baml": \`
                class Item { value string }
            \` })
            type First = unreflect((first.get_class("root.Item") ?? throw "missing A").as_type())
            type Second = unreflect((second.get_class("root.Item") ?? throw "missing B").as_type())

            let make = first.get_function<() -> First>("root.Make") ?? throw "missing root.Make"
            let value: unknown = make()
            let own = if value is First { "yes" } else { "no" }
            let foreign = if value is Second { "yes" } else { "no" }
            \`\${own}|\${foreign}\`
        }
    `
}

function source_16() -> string {
    `
        client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        function Render<T>() -> T {
            client: TestClient
            prompt: \`\${ctx.output_format()}\`
        }

        function main() -> string {
            // \`next\` makes each \`Item\` recursive, which forces the schema to
            // hoist it under a name rather than inline its shape.
            let first = reflect.Package.compile({ "a.baml": \`
                class Item { alpha string, next Item? }
            \` })
            let second = reflect.Package.compile({ "b.baml": \`
                class Item { beta int, next Item? }
            \` })
            type First = unreflect((first.get_class("root.Item") ?? throw "missing A").as_type())
            type Second = unreflect((second.get_class("root.Item") ?? throw "missing B").as_type())

            \`\${Render@render_prompt<First[]>()}~\${Render@render_prompt<Second[]>()}\`
        }
    `
}

test "scoped_type_binding_evaluates_once_types_contracts_and_leaves_as_unknown" {
    let result = root.mixed_phase1_support.call(source_1(), "root.main");
    assert.equal(root.mixed_phase1_support.bool_result(result), true)
}

test "type_bindings_work_in_lambdas_and_nested_shadowing_uses_distinct_slots" {
    let result = root.mixed_phase1_support.call(source_2(), "root.main");
    assert.equal(root.mixed_phase1_support.bool_result(result), true)
}

test "interface_impl_methods_keep_runtime_type_definitions" {
    let result = root.mixed_phase1_support.call(source_3(), "root.main");
    assert.equal(root.mixed_phase1_support.string_result(result), "RuntimeOutput|Pixel")
}

test "agent_run_parses_a_reflected_output_type" {
    let result = root.mixed_phase1_support.call(source_4(), "root.main");
    assert.equal(root.mixed_phase1_support.string_result(result), "Pixel|Pixel")
}

test "agent_runner_renders_reflected_output_schemas_on_the_wire" {
    let result = root.mixed_phase1_support.call(source_5(), "root.main");
    let prompts = root.mixed_phase1_support.string_result(result).split("~~");
    assert.equal(prompts.length(), 4);
    assert.contains(prompts[0], "minted_field");
    assert.contains(prompts[0], "Minted runner field docs");
    assert.contains(prompts[1], "PENDING");
    assert.contains(prompts[1], "RUNNING");
    assert.contains(prompts[2], "alpha_only");
    assert.contains(prompts[2], "beta_only");
    assert.contains(prompts[3], "compiled_field")
}

test "minted_type_identity_survives_interface_dispatch" {
    let result = root.mixed_phase1_support.call(source_6(), "root.main");
    assert.equal(root.mixed_phase1_support.string_result(result), "true|true|true")
}

test "interface_impl_methods_look_up_a_type_keyed_registry" {
    let result = root.mixed_phase1_support.call(source_7(), "root.main");
    assert.equal(root.mixed_phase1_support.string_result(result), "first|second")
}

test "dispatch_identity_separates_distinct_mints_and_leaves_static_generics_alone" {
    let result = root.mixed_phase1_support.call(source_8(), "root.main");
    assert.equal(root.mixed_phase1_support.string_result(result), "true|false|true|false")
}

test "runtime_package_declarations_keep_definitions_and_identity" {
    let result = root.mixed_phase1_support.call(source_9(), "root.main");
    assert.equal(root.mixed_phase1_support.string_result(result), "true|Item")
}

test "static_class_slots_are_not_answered_from_a_same_named_runtime_definition" {
    let result = root.mixed_phase1_support.call(source_10(), "root.main");
    assert.equal(root.mixed_phase1_support.string_result(result), "true|false")
}

test "same_named_declarations_from_two_packages_keep_separate_identities" {
    let result = root.mixed_phase1_support.call(source_11(), "root.main");
    assert.equal(root.mixed_phase1_support.string_result(result), "true|false")
}

test "dispatch_identity_covers_owner_and_method_slots_together" {
    let result = root.mixed_phase1_support.call(source_12(), "root.main");
    assert.equal(root.mixed_phase1_support.string_result(result), "true|true|false")
}

test "dispatch_identity_covers_a_runtime_enum_slot" {
    let result = root.mixed_phase1_support.call(source_13(), "root.main");
    assert.equal(root.mixed_phase1_support.string_result(result), "true|false")
}

test "a_package_declarations_identity_never_reaches_rendered_output" {
    let result = root.mixed_phase1_support.call(source_14(), "root.main");
    let rendered = root.mixed_phase1_support.string_result(result);
    assert.is_true(!rendered.includes("$dyn"));
    let parts = rendered.split("~~");
    assert.equal(parts.length(), 4);
    assert.equal(parts[0], "Item");
    assert.equal(parts[1], "class Item {\n  value string\n  next Item?\n}");
    assert.contains(parts[2], "Item");
    assert.contains(parts[2], "value");
    assert.contains(parts[3], "Item")
}

test "a_runtime_type_test_does_not_match_another_packages_same_named_class" {
    let result = root.mixed_phase1_support.call(source_15(), "root.main");
    assert.equal(root.mixed_phase1_support.string_result(result), "yes|no")
}

test "an_output_format_schema_describes_each_packages_own_class" {
    let result = root.mixed_phase1_support.call(source_16(), "root.main");
    let rendered = root.mixed_phase1_support.string_result(result);
    assert.is_true(!rendered.includes("$dyn"));
    let parts = rendered.split("~");
    assert.equal(parts.length(), 2);
    assert.contains(parts[0], "alpha");
    assert.is_true(!parts[0].includes("beta"));
    assert.contains(parts[1], "beta");
    assert.is_true(!parts[1].includes("alpha"))
}
