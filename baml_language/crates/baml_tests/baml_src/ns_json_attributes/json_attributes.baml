// `@json` attributes: what a class reads and writes through `baml.json`, with
// no override and no arguments.

class UseDefault {
    type: "use-default",
}

class ByoVpc {
    privateSubnets: string[] @json.rename("private_subnet_ids"),
    securityGroups: string[]? @json.rename("security_group_ids"),
    type: "byo-vpc-aws",
    vpcId: string @json.rename("vpc_id"),

    @@json.omit_nulls
}

class NetworkOther {
    type: "UNKNOWN",
    raw: map<string, json> @json.fallback,
}

type Network = UseDefault | ByoVpc | NetworkOther

class Settings {
    network: Network?,
    name: string?,

    @@json.omit_nulls
}

class Config {
    id: string,
    type: string,
    extra: map<string, json> @json.flatten,
}

class Plain {
    a: string?,
}

enum Mode {
    Fast @json.rename("fast-mode")
    Slow
}

class WithMode {
    mode: Mode,
}

class Both {
    value: string @alias("shown") @json.rename("wire"),
}

class Binding {
    region: json,
    bucket: json?,

    @@json.omit_nulls
}

function read_network(text: string) -> Network throws baml.json.ParseError | baml.json.DecodeError {
    baml.json.decode<Network>(text)
}

function kind(text: string) -> string {
    let network = read_network(text) catch (e) {
        baml.json.DecodeError => return "error",
        baml.json.ParseError => return "parse error",
    };
    match (network) {
        let n: ByoVpc => "byo " + n.vpcId,
        let n: UseDefault => "default",
        let n: NetworkOther => "other " + n.type,
    }
}

function out(value: unknown) -> string throws baml.json.SerializationError {
    baml.json.stringify(value)
}

testset "json attributes" {
    test "known tag at the root reads renamed keys" {
        assert.equal(kind(`{"type":"byo-vpc-aws","vpc_id":"v","private_subnet_ids":["a"]}`), "byo v");
        assert.equal(kind(`{"type":"use-default"}`), "default");
    }
    test "unknown tag is the fallback" {
        assert.equal(kind(`{"type":"future","foo":1}`), "other UNKNOWN");
    }
    test "an object with no tag is an error" {
        assert.equal(kind(`{"kind":"no type"}`), "error");
        assert.equal(kind(`{"type":1}`), "error");
    }
    test "a known tag that does not decode is an error" {
        assert.equal(kind(`{"type":"byo-vpc-aws","vpc_id":"v"}`), "error");
    }
    test "the declared name is not a key" {
        assert.equal(kind(`{"type":"byo-vpc-aws","vpcId":"v","privateSubnets":[]}`), "error");
    }
    test "the fallback writes what it read" {
        let text = `{"foo":1,"type":"future"}`;
        assert.equal(out(read_network(text)), text);
    }
    test "omit nulls and renames write" {
        let n = ByoVpc { privateSubnets: ["a"], securityGroups: null, type: "byo-vpc-aws", vpcId: "v" };
        assert.equal(out(n), `{"private_subnet_ids":["a"],"type":"byo-vpc-aws","vpc_id":"v"}`);
    }
    test "omit nulls is the class's own" {
        assert.equal(out(Plain { a: null }), `{"a":null}`);
        assert.equal(out(Settings { network: null, name: "x" }), `{"name":"x"}`);
    }
    test "null in a json field is a value" {
        assert.equal(out(Binding { region: null, bucket: null }), `{"region":null}`);
    }
    test "a union inside a class" {
        let s = baml.json.decode<Settings>(`{"network":{"type":"future"}}`);
        assert.equal(out(s), `{"network":{"type":"future"}}`);
    }
    test "flatten reads and writes" {
        let text = `{"id":"i","type":"t","a":{"k":1},"b":null}`;
        let c = baml.json.decode<Config>(text);
        assert.equal(c.extra.length(), 2);
        assert.equal(out(c), text);
    }
    test "to reads the same as decode" {
        let c = baml.json.to<Config>(baml.json.parse(`{"id":"i","type":"t","a":1}`));
        assert.equal(c.extra.length(), 1);
    }
    test "a declared field beats a flattened key" {
        let c = Config { id: "real", type: "t", extra: {"id": "from-extra", "b": 1} };
        assert.equal(out(c), `{"id":"real","type":"t","b":1}`);
    }
    test "enum rename" {
        assert.equal(out(WithMode { mode: Mode.Fast }), `{"mode":"fast-mode"}`);
        assert.equal(baml.json.decode<WithMode>(`{"mode":"fast-mode"}`).mode, Mode.Fast);
    }
    test "rename beats alias" {
        assert.equal(out(Both { value: "x" }), `{"wire":"x"}`);
        let aliased = baml.json.stringify(Both { value: "x" }, naming = baml.json.Naming.Aliased);
        assert.equal(aliased, `{"wire":"x"}`);
    }
}
