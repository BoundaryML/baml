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

class FlatOmit {
    note: string?,
    extra: map<string, json> @json.flatten,

    @@json.omit_nulls
}

class Plain2 {
    id: string,
}

class FlattenFirst {
    extra: map<string, json> @json.flatten,
    id: string @json.rename("extra"),
}

class FlattenLast {
    id: string @json.rename("extra"),
    extra: map<string, json> @json.flatten,
}

class TaggedScope {
    role: "admin",
    type: "manager",
}

class OtherScope {
    type: "UNKNOWN",
    raw: map<string, json> @json.fallback,
}

type Scope = TaggedScope | OtherScope

class Storage {
    service: "s3",
    type: "storage",
}

class Queue {
    service: "sqs",
    type: "storage",
}

class Ai {
    type: "ai",
}

class BindingOther {
    type: "UNKNOWN",
    raw: map<string, json> @json.fallback,
}

type AnyBinding = Storage | Queue | Ai | BindingOther

function binding(text: string) -> string {
    let read = baml.json.decode<AnyBinding>(text) catch (e) {
        baml.json.DecodeError => return "error",
        baml.json.ParseError => return "parse error",
    };
    let parsed = baml.json.to<AnyBinding>(baml.json.parse(text)) catch (e) {
        baml.json.DecodeError => return "error",
        baml.json.ParseError => return "parse error",
    };
    match (parsed) {
        let b: Storage => {}
        let b: Queue => {}
        let b: Ai => {}
        let b: BindingOther => {}
    }
    match (read) {
        let b: Storage => "s3",
        let b: Queue => "sqs",
        let b: Ai => "ai",
        let b: BindingOther => "other",
    }
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
    test "a flattened class in a union is not skipped" {
        let c = baml.json.decode<Config | Plain2>(`{"id":"i","type":"t","a":1}`);
        match (c) {
            let c: Config => { assert.equal(c.extra.length(), 1); }
            let p: Plain2 => { assert.equal(p.id, "not this member"); }
        }
    }
    test "a flatten field shares no key with a renamed field" {
        assert.equal(out(FlattenFirst { extra: {"a": 1}, id: "i" }), `{"a":1,"extra":"i"}`);
        assert.equal(out(FlattenLast { id: "i", extra: {"a": 1} }), `{"extra":"i","a":1}`);
    }
    test "only the tag decides: a wrong literal elsewhere fails as that member" {
        let wrong = baml.json.decode<Scope>(`{"type":"manager","role":"viewer"}`) catch (e) {
            baml.json.DecodeError => TaggedScope { role: "admin", type: "manager" },
            baml.json.ParseError => OtherScope { type: "UNKNOWN", raw: {} },
        };
        match (wrong) {
            let t: TaggedScope => { assert.equal(t.role, "admin"); }
            let o: OtherScope => { assert.equal(o.type, "this is not an unknown member"); }
        }
    }
    test "a second tag is read when the first leaves several members" {
        assert.equal(binding(`{"type":"storage","service":"s3"}`), "s3");
        assert.equal(binding(`{"type":"storage","service":"sqs"}`), "sqs");
        assert.equal(binding(`{"type":"storage","service":"minio"}`), "other");
        assert.equal(binding(`{"type":"future"}`), "other");
        assert.equal(binding(`{"type":"ai","service":"anything"}`), "ai");
    }
    test "a second tag that is missing or of the wrong kind is an error" {
        assert.equal(binding(`{"type":"storage"}`), "error");
        assert.equal(binding(`{"type":"storage","service":5}`), "error");
        assert.equal(binding(`{"type":"storage","service":null}`), "error");
    }
    test "a flatten map is a copy that refuses a cycle" {
        let src = baml.json.parse(`{"id":"i","type":"t","n":{"a":1}}`);
        let c = baml.json.to<Config>(src);
        match (c.extra["n"]) {
            let m: map<string, json> => { m["a"] = 2; }
            _ => {}
        }
        assert.equal(baml.json.stringify(src), `{"id":"i","type":"t","n":{"a":1}}`);
    }
    test "a flatten key beside an omitted field is written" {
        assert.equal(out(FlatOmit { note: null, extra: {"note": "x", "b": 1} }), `{"note":"x","b":1}`);
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
