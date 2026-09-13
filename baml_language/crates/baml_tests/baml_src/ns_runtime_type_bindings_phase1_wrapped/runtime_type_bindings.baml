// Native ports of Phase-1 Rust integration tests.

function source_1() -> string {
    "\nfunction main() -> bool {\n  let pkg = reflect.Package.compile({ \"items.baml\": `\nclass Item { value string }\nfunction Items() -> Item[] { [Item { value: \"bound\" }] }\n  ` })\n  let item_ct = pkg.get_class(\"root.Item\") ?? throw \"missing Item\"\n  let binding_evaluations: int = 0\n  let operand = () -> {\n    binding_evaluations += 1\n    item_ct.as_type()\n  }\n\n  let escaped: unknown = {\n    type T = unreflect(operand());\n    if (reflect.Type.of<T>() != item_ct.as_type()) {\n      throw \"reflect.Type.of<T>() did not preserve the bound value\"\n    }\n    let get_items = pkg.get_function<() -> T[]>(\"root.Items\")\n      ?? throw \"missing root.Items\"\n    let items: T[] = get_items()\n    let item: T = items[0]\n    if (reflect.Type.of_value(item) != reflect.Type.of<T>()) {\n      throw \"typed result did not retain T\"\n    }\n    item\n  }\n\n  binding_evaluations == 1\n    && reflect.Type.of_value(escaped) == item_ct.as_type()\n}\n"
}

function source_2() -> string {
    "\nfunction main() -> bool {\n  let string_t = reflect.Type.of<string>()\n  let int_t = reflect.Type.of<int>()\n  let check = (bound: reflect.Type) -> {\n    type T = unreflect(bound);\n    reflect.Type.of<T>() == bound\n  }\n\n  type T = unreflect(string_t);\n  let inner = {\n    type T = unreflect(int_t);\n    reflect.Type.of<T>() == int_t\n  }\n  reflect.Type.of<T>() == string_t && inner && check(int_t)\n}\n"
}

function source_3() -> string {
    "\n        interface Parser<Out> {\n            function parse_it(self, text: string) -> Out throws unknown\n            function describe(self) -> string throws never\n        }\n\n        class Holder<T> {\n            function new() -> Holder<T> throws never {\n                Holder {}\n            }\n\n            implements Parser<T> {\n                function parse_it(self, text: string) -> T {\n                    baml.sap.parse<T>(text)\n                }\n\n                function describe(self) -> string throws never {\n                    reflect.Type.of<T>().to_string()\n                }\n            }\n        }\n\n        function main() -> string {\n            let output_type = reflect.class.new(\"RuntimeOutput\", {\n                \"name\": reflect.Type.of<string>(),\n            }).as_type()\n            type Out = unreflect(output_type)\n\n            let holder = Holder<Out>.new()\n            let parsed = holder.parse_it(`{\"name\":\"Pixel\"}`)\n            `${holder.describe()}|${reflect.class.get_field<string>(parsed, \"name\")}`\n        }\n        "
}

function source_4() -> string {
    "\n        \nclient DefaultClient = openai.ResponsesClient.new(\n    model = \"gpt-4o-mini\",\n    api_key = \"test-key\",\n    base_url = \"http://localhost:1234\",\n);\n\nclass ProbeClient {\n    reply: string,\n    requests: string[],\n\n    implements ai.Client {\n        function id(self) -> string {\n            \"probe\"\n        }\n\n        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {\n            let prompt = input.prompt(ai.internal.build_output_format(input.output_type)).text();\n            let _ = self.requests.push(prompt);\n            baml.http.Request { method: \"POST\", url: \"https://probe.invalid\", headers: {}, body: prompt }\n        }\n\n        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {\n            let _ = self.render(input);\n            ai.ModelTurn {\n                calls: [],\n                content: [ai.content.Text { text: self.reply }],\n                stop_reason: ai.content.StopReason.Complete,\n                usage: null,\n            }\n        }\n    }\n}\n\n\n        function DynamicOutput<T>() -> T {\n            client: DefaultClient\n            prompt: `${ctx.output_format()}`\n        }\n\n        function main() -> string {\n            let output_type = reflect.class.new(\"RuntimeOutput\", {\n                \"name\": reflect.Type.of<string>(),\n            }).as_type()\n            type Out = unreflect(output_type)\n\n            // Control: the direct SAP call has always worked.\n            let direct = baml.sap.parse<Out>(`{\"name\":\"Pixel\"}`)\n\n            let run = ai.Agent.new(\n                client = ProbeClient { reply: `{\"name\":\"Pixel\"}`, requests: [] },\n            ).run(DynamicOutput@spec<Out>())\n\n            let direct_name = reflect.class.get_field<string>(direct, \"name\")\n            let agent_name = reflect.class.get_field<string>(run.value, \"name\")\n            `${direct_name}|${agent_name}`\n        }\n        "
}

function source_5() -> string {
    "\n        \nclient DefaultClient = openai.ResponsesClient.new(\n    model = \"gpt-4o-mini\",\n    api_key = \"test-key\",\n    base_url = \"http://localhost:1234\",\n);\n\nclass ProbeClient {\n    reply: string,\n    requests: string[],\n\n    implements ai.Client {\n        function id(self) -> string {\n            \"probe\"\n        }\n\n        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {\n            let prompt = input.prompt(ai.internal.build_output_format(input.output_type)).text();\n            let _ = self.requests.push(prompt);\n            baml.http.Request { method: \"POST\", url: \"https://probe.invalid\", headers: {}, body: prompt }\n        }\n\n        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {\n            let _ = self.render(input);\n            ai.ModelTurn {\n                calls: [],\n                content: [ai.content.Text { text: self.reply }],\n                stop_reason: ai.content.StopReason.Complete,\n                usage: null,\n            }\n        }\n    }\n}\n\n\n        function DynamicOutput<T>() -> T {\n            client: DefaultClient\n            prompt: `${ctx.output_format()}`\n        }\n\n        function main() -> string {\n            let minted_class = reflect.class.new(\"MintedRecord\", {\n                \"minted_field\": reflect.Type.of<string>().meta(\n                    docstring = \"Minted runner field docs\",\n                ),\n            }).as_type()\n            type MintedRecord = unreflect(minted_class)\n            let class_client = ProbeClient { reply: `{\"minted_field\":\"ok\"}`, requests: [] }\n            let _ = ai.Agent.new(\n                client = class_client,\n            ).run(DynamicOutput@spec<MintedRecord>())\n\n            let minted_enum = reflect.enum.new(\n                \"MintedChoice\",\n                [\"PENDING\", \"RUNNING\"],\n            ).as_type()\n            type MintedChoice = unreflect(minted_enum)\n            let enum_client = ProbeClient { reply: `\"RUNNING\"`, requests: [] }\n            let _ = ai.Agent.new(\n                client = enum_client,\n            ).run(DynamicOutput@spec<MintedChoice>())\n\n            let alpha = reflect.class.new(\"AlphaShape\", {\n                \"alpha_only\": reflect.Type.of<string>(),\n            }).as_type()\n            let beta = reflect.class.new(\"BetaShape\", {\n                \"beta_only\": reflect.Type.of<int>(),\n            }).as_type()\n            let union = reflect.union.new([alpha, beta]).as_type()\n            type MintedUnion = unreflect(union)\n            let union_client = ProbeClient { reply: `{\"alpha_only\":\"ok\"}`, requests: [] }\n            let _ = ai.Agent.new(\n                client = union_client,\n            ).run(DynamicOutput@spec<MintedUnion>())\n\n            let package = reflect.Package.compile({ \"compiled.baml\": `\nclass CompiledRecord { compiled_field string }\n` })\n            let compiled = (package.get_class(\"root.CompiledRecord\")\n                ?? throw \"missing CompiledRecord\").as_type()\n            type CompiledRecord = unreflect(compiled)\n            let compiled_client = ProbeClient { reply: `{\"compiled_field\":\"ok\"}`, requests: [] }\n            let _ = ai.Agent.new(\n                client = compiled_client,\n            ).run(DynamicOutput@spec<CompiledRecord>())\n\n            let class_prompt = class_client.requests.at(0) ?? throw \"missing class request\"\n            let enum_prompt = enum_client.requests.at(0) ?? throw \"missing enum request\"\n            let union_prompt = union_client.requests.at(0) ?? throw \"missing union request\"\n            let compiled_prompt = compiled_client.requests.at(0) ?? throw \"missing compiled request\"\n            `${class_prompt}~~${enum_prompt}~~${union_prompt}~~${compiled_prompt}`\n        }\n        "
}

function source_6() -> string {
    "\n        interface Probe<Out> {\n            function same(self, t: reflect.Type) -> bool throws never\n            function same_from_default(self, t: reflect.Type) -> bool throws never {\n                reflect.Type.of<Out>() == t\n            }\n        }\n\n        interface Relay<Out> {\n            function relay(self, t: reflect.Type) -> bool throws unknown\n        }\n\n        class Holder<T> {\n            function new() -> Holder<T> throws never {\n                Holder {}\n            }\n\n            implements Probe<T> {\n                function same(self, t: reflect.Type) -> bool throws never {\n                    reflect.Type.of<T>() == t\n                }\n            }\n\n            implements Relay<T> {\n                function relay(self, t: reflect.Type) -> bool {\n                    // Two-hop: the second interface operand is materialized\n                    // inside this frame, so it has to carry the identity on.\n                    Holder<T>.new().same(t)\n                }\n            }\n        }\n\n        function main() -> string {\n            let output_type = reflect.class.new(\"RuntimeOutput\", {\n                \"name\": reflect.Type.of<string>(),\n            }).as_type()\n            type Out = unreflect(output_type)\n\n            let holder = Holder<Out>.new()\n            let impl_method = holder.same(output_type)\n            let default_method = holder.same_from_default(output_type)\n            let two_hop = holder.relay(output_type)\n            `${impl_method}|${default_method}|${two_hop}`\n        }\n        "
}

function source_7() -> string {
    "\n        class Entry {\n            key: reflect.Type,\n            label: string,\n        }\n\n        interface Named<Out> {\n            function label(self, first: Entry, second: Entry) -> string throws never\n        }\n\n        class Holder<T> {\n            function new() -> Holder<T> throws never {\n                Holder {}\n            }\n\n            implements Named<T> {\n                function label(self, first: Entry, second: Entry) -> string throws never {\n                    let wanted = reflect.Type.of<T>()\n                    if (first.key == wanted) {\n                        first.label\n                    } else if (second.key == wanted) {\n                        second.label\n                    } else {\n                        \"missing\"\n                    }\n                }\n            }\n        }\n\n        function main() -> string {\n            let first_type = reflect.class.new(\"Shape\", {\n                \"name\": reflect.Type.of<string>(),\n            }).as_type()\n            let second_type = reflect.class.new(\"Shape\", {\n                \"name\": reflect.Type.of<string>(),\n            }).as_type()\n            type First = unreflect(first_type)\n            type Second = unreflect(second_type)\n\n            let first_entry = Entry { key: first_type, label: \"first\" }\n            let second_entry = Entry { key: second_type, label: \"second\" }\n\n            let first_hit = Holder<First>.new().label(first_entry, second_entry)\n            let second_hit = Holder<Second>.new().label(first_entry, second_entry)\n            `${first_hit}|${second_hit}`\n        }\n        "
}

function source_8() -> string {
    "\n        interface Probe<Out> {\n            function same(self, t: reflect.Type) -> bool throws never\n        }\n\n        class Holder<T> {\n            function new() -> Holder<T> throws never {\n                Holder {}\n            }\n\n            implements Probe<T> {\n                function same(self, t: reflect.Type) -> bool throws never {\n                    reflect.Type.of<T>() == t\n                }\n            }\n        }\n\n        function main() -> string {\n            let mine = reflect.class.new(\"Shape\", {\n                \"name\": reflect.Type.of<string>(),\n            }).as_type()\n            let other = reflect.class.new(\"Shape\", {\n                \"name\": reflect.Type.of<string>(),\n            }).as_type()\n            type Mine = unreflect(mine)\n\n            let holder = Holder<Mine>.new()\n            let own_mint = holder.same(mine)\n            let foreign_mint = holder.same(other)\n\n            let static_holder = Holder<string>.new()\n            let static_match = static_holder.same(reflect.Type.of<string>())\n            let static_miss = static_holder.same(reflect.Type.of<int>())\n            `${own_mint}|${foreign_mint}|${static_match}|${static_miss}`\n        }\n        "
}

function source_9() -> string {
    "\n        interface Probe<Out> {\n            function same(self, t: reflect.Type) -> bool throws never\n            function name(self) -> string throws never {\n                reflect.Type.of<Out>().to_string()\n            }\n        }\n\n        class Holder<T> {\n            function new() -> Holder<T> throws never {\n                Holder {}\n            }\n\n            implements Probe<T> {\n                function same(self, t: reflect.Type) -> bool throws never {\n                    reflect.Type.of<T>() == t\n                }\n            }\n        }\n\n        function main() -> string {\n            let pkg = reflect.Package.compile({ \"items.baml\": `\nclass Item { value string }\n              ` })\n            let item_type = (pkg.get_class(\"root.Item\") ?? throw \"missing Item\").as_type()\n            type Item = unreflect(item_type)\n\n            let holder = Holder<Item>.new()\n            `${holder.same(item_type)}|${holder.name()}`\n        }\n        "
}

function source_10() -> string {
    "\n        class Item {\n            value: string,\n        }\n\n        interface Probe<Out> {\n            function same(self, t: reflect.Type) -> bool throws never\n        }\n\n        class Holder<T> {\n            function new() -> Holder<T> throws never {\n                Holder {}\n            }\n\n            implements Probe<T> {\n                function same(self, t: reflect.Type) -> bool throws never {\n                    reflect.Type.of<T>() == t\n                }\n            }\n        }\n\n        function main() -> string {\n            let pkg = reflect.Package.compile({ \"items.baml\": `\nclass Item { value string }\n              ` })\n            let runtime_item = (pkg.get_class(\"root.Item\") ?? throw \"missing Item\").as_type()\n            // Binding it merges `user.Item` into this frame's overlay, which is\n            // what every type materialized here from now on carries.\n            type Shadow = unreflect(runtime_item)\n            let _shadow_holder = Holder<Shadow>.new()\n\n            let holder = Holder<Item>.new()\n            `${holder.same(reflect.Type.of<Item>())}|${holder.same(runtime_item)}`\n        }\n        "
}

function source_11() -> string {
    "\n        interface Probe<Out> {\n            function same(self, t: reflect.Type) -> bool throws never\n        }\n\n        class Holder<T> {\n            function new() -> Holder<T> throws never {\n                Holder {}\n            }\n\n            implements Probe<T> {\n                function same(self, t: reflect.Type) -> bool throws never {\n                    reflect.Type.of<T>() == t\n                }\n            }\n        }\n\n        function main() -> string {\n            let first = reflect.Package.compile({ \"a.baml\": `\nclass Item { value string }\n              ` })\n            let second = reflect.Package.compile({ \"b.baml\": `\nclass Item { value string }\n              ` })\n            let first_item = (first.get_class(\"root.Item\") ?? throw \"missing A\").as_type()\n            let second_item = (second.get_class(\"root.Item\") ?? throw \"missing B\").as_type()\n            type First = unreflect(first_item)\n            type Second = unreflect(second_item)\n\n            let _first_holder = Holder<First>.new()\n            let holder = Holder<Second>.new()\n            // The holder's own declaration answers true \u2014 the slot carries its\n            // head. The failure this guards is the SECOND value being `true`:\n            // answering with a same-named foreign package's identity.\n            `${holder.same(second_item)}|${holder.same(first_item)}`\n        }\n        "
}

function source_12() -> string {
    "\n        interface Probe<Out> {\n            function pair<M>(self, own: reflect.Type, method: reflect.Type) -> string throws never\n        }\n\n        class Holder<T> {\n            function new() -> Holder<T> throws never {\n                Holder {}\n            }\n\n            implements Probe<T> {\n                function pair<M>(self, own: reflect.Type, method: reflect.Type) -> string throws never {\n                    `${reflect.Type.of<T>() == own}|${reflect.Type.of<M>() == method}|${reflect.Type.of<T>() == method}`\n                }\n            }\n        }\n\n        function main() -> string {\n            let owner_type = reflect.class.new(\"Owner\", {\n                \"name\": reflect.Type.of<string>(),\n            }).as_type()\n            let method_type = reflect.class.new(\"Method\", {\n                \"name\": reflect.Type.of<string>(),\n            }).as_type()\n            type Owner = unreflect(owner_type)\n            type Method = unreflect(method_type)\n\n            Holder<Owner>.new().pair<Method>(owner_type, method_type)\n        }\n        "
}

function source_13() -> string {
    "\n        interface Probe<Out> {\n            function same(self, t: reflect.Type) -> bool throws never\n        }\n\n        class Holder<T> {\n            function new() -> Holder<T> throws never {\n                Holder {}\n            }\n\n            implements Probe<T> {\n                function same(self, t: reflect.Type) -> bool throws never {\n                    reflect.Type.of<T>() == t\n                }\n            }\n        }\n\n        function main() -> string {\n            let choice = reflect.enum.new(\"Choice\", [\"FIRST\", \"SECOND\"]).as_type()\n            let other = reflect.enum.new(\"Choice\", [\"FIRST\", \"SECOND\"]).as_type()\n            type Choice = unreflect(choice)\n\n            let holder = Holder<Choice>.new()\n            `${holder.same(choice)}|${holder.same(other)}`\n        }\n        "
}

function source_14() -> string {
    "\n        client TestClient = openai.ResponsesClient.new(\n            model = \"gpt-4o-mini\",\n            api_key = \"test-key\",\n            base_url = \"http://localhost:1234\",\n        );\n\n        function Render<T>() -> T {\n            client: TestClient\n            prompt: `${ctx.output_format()}`\n        }\n\n        function main() -> string {\n            // `next` makes `Item` recursive, which forces the LLM schema to\n            // hoist it under a *name* rather than inline its shape.\n            let pkg = reflect.Package.compile({ \"items.baml\": `\nclass Item { value string, next Item? }\nfunction Items() -> Item[] { [Item { value: \"bound\", next: null }] }\n              ` })\n            let item_type = (pkg.get_class(\"root.Item\") ?? throw \"missing Item\").as_type()\n            type Item = unreflect(item_type)\n\n            // A diagnostic that has to name the type it rejected.\n            let contract = pkg.get_function<() -> Item>(\"root.Items\") catch (e) {\n                reflect.errors.CompilationError => e.diagnostics[0].message,\n                _ => \"wrong error\",\n            }\n            let diagnostic = if contract is string { contract } else { \"no diagnostic\" }\n\n            // `~~` and not `|`: `to_baml()` and the LLM schema both spell a\n            // union with `|`, so a `|` join could split *inside* a surface and\n            // leave the assertions below silently comparing the wrong text.\n            let schema = Render@render_prompt<Item[]>()\n            `${item_type.to_string()}~~${item_type.to_baml()}~~${schema}~~${diagnostic}`\n        }\n        "
}

function source_15() -> string {
    "\n        function main() -> string {\n            let first = reflect.Package.compile({ \"a.baml\": `\nclass Item { value string }\nfunction Make() -> Item { Item { value: \"a\" } }\n              ` })\n            let second = reflect.Package.compile({ \"b.baml\": `\nclass Item { value string }\n              ` })\n            type First = unreflect((first.get_class(\"root.Item\") ?? throw \"missing A\").as_type())\n            type Second = unreflect((second.get_class(\"root.Item\") ?? throw \"missing B\").as_type())\n\n            let make = first.get_function<() -> First>(\"root.Make\") ?? throw \"missing root.Make\"\n            let value: unknown = make()\n            let own = if value is First { \"yes\" } else { \"no\" }\n            let foreign = if value is Second { \"yes\" } else { \"no\" }\n            `${own}|${foreign}`\n        }\n        "
}

function source_16() -> string {
    "\n        client TestClient = openai.ResponsesClient.new(\n            model = \"gpt-4o-mini\",\n            api_key = \"test-key\",\n            base_url = \"http://localhost:1234\",\n        );\n\n        function Render<T>() -> T {\n            client: TestClient\n            prompt: `${ctx.output_format()}`\n        }\n\n        function main() -> string {\n            // `next` makes each `Item` recursive, which forces the schema to\n            // hoist it under a name rather than inline its shape.\n            let first = reflect.Package.compile({ \"a.baml\": `\nclass Item { alpha string, next Item? }\n              ` })\n            let second = reflect.Package.compile({ \"b.baml\": `\nclass Item { beta int, next Item? }\n              ` })\n            type First = unreflect((first.get_class(\"root.Item\") ?? throw \"missing A\").as_type())\n            type Second = unreflect((second.get_class(\"root.Item\") ?? throw \"missing B\").as_type())\n\n            `${Render@render_prompt<First[]>()}~${Render@render_prompt<Second[]>()}`\n        }\n        "
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
