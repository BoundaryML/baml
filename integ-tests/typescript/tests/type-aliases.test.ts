import { b } from "./test-setup";

const arrayDepth = (value: unknown): number => {
  if (!Array.isArray(value)) {
    return 0;
  }
  if (value.length === 0) {
    return 1;
  }
  return 1 + Math.max(...value.map(arrayDepth));
};

const arrayWidth = (value: unknown): number =>
  Array.isArray(value) ? value.length : 0;

describe("Type aliases tests", () => {
  it("primitive union alias", async () => {
    const arg = "test";
    const res = await b.request.PrimitiveAlias(arg);
    const json = res.body.json();
    expect(json.messages[0].content[0].text).toContain("int or string or bool or float");

    const parsed = b.parse.PrimitiveAlias(JSON.stringify(arg));
    expect(parsed).toEqual(arg);
  });

  it("map alias", async () => {
    const arg = { A: ["B", "C"], B: [], C: [] };
    const res = await b.request.MapAlias(arg);
    const json = res.body.json();
    expect(json.messages[0].content[0].text).toContain("map<string, string[]>");

    const parsed = b.parse.MapAlias(JSON.stringify(arg));
    expect(parsed).toEqual(arg);
  });

  it("alias union", async () => {
    const stringArg = "test";
    const stringRes = await b.request.NestedAlias(stringArg);
    const stringJson = stringRes.body.json();
    expect(stringJson.messages[0].content[0].text).toContain("int or string or bool or float or string[] or map<string, string[]>");

    const parsedString = b.parse.NestedAlias(JSON.stringify(stringArg));
    expect(parsedString).toEqual(stringArg);

    const mapArg = { A: ["B", "C"], B: [], C: [] };
    const mapRes = await b.request.NestedAlias(mapArg);
    const mapJson = mapRes.body.json();
    expect(mapJson.messages[0].content[0].text).toContain("int or string or bool or float or string[] or map<string, string[]>");

    const parsedMap = b.parse.NestedAlias(JSON.stringify(mapArg));
    expect(parsedMap).toEqual(mapArg);
  });

  it("alias pointing to recursive class", async () => {
    const arg = { value: 1, next: null };
    const res = await b.request.AliasThatPointsToRecursiveType(arg);
    const json = res.body.json();
    expect(json.messages[0].content[0].text).toContain(`LinkedListAliasNode {
  value: int,
  next: LinkedListAliasNode or null,
}

Answer in JSON using this schema: LinkedListAliasNode`);

    const parsed = b.parse.AliasThatPointsToRecursiveType(JSON.stringify(arg));
    expect(parsed).toEqual(arg);
  });

  it("class pointing to alias that points to recursive class", async () => {
    const arg = { list: { value: 1, next: null } };
    const res = await b.request.ClassThatPointsToRecursiveClassThroughAlias(arg);
    const json = res.body.json();
    expect(json.messages[0].content[0].text).toContain(`LinkedListAliasNode {
  value: int,
  next: LinkedListAliasNode or null,
}

Answer in JSON using this schema:
{
  list: LinkedListAliasNode,
}`)

    const parsed = b.parse.ClassThatPointsToRecursiveClassThroughAlias(
      JSON.stringify(arg),
    );
    expect(parsed).toEqual(arg);
  });

  it("recursive class with alias indirection", async () => {
    const arg = { value: 1, next: { value: 2, next: null } };
    const res = await b.request.RecursiveClassWithAliasIndirection(arg);
    const json = res.body.json();
    console.log(JSON.stringify(json.messages[0].content[0].text));
    expect(json.messages[0].content[0].text).toContain("NodeWithAliasIndirection {\n  value: int,\n  next: NodeWithAliasIndirection or null,\n}\n\nAnswer in JSON using this schema: NodeWithAliasIndirection");

    const parsed = b.parse.RecursiveClassWithAliasIndirection(JSON.stringify(arg));
    expect(parsed).toEqual(arg);
  });

  it("merge alias attributes", async () => {
    const res = await b.MergeAliasAttributes(123);
    console.log(JSON.stringify(res));
    expect(res.amount.value).toEqual(123);
    expect(res.amount.checks["gt_ten"].status).toEqual("succeeded");

  });

  // Inputs with checks are not supported yet
  // it('return alias with merged attrs', async () => {
  //   const res = await b.ReturnAliasWithMergedAttributes({
  //     value: 123,
  //     checks: {
  //       gt_ten: {
  //         name: 'gt_ten',
  //         expr: 'value > 10',
  //         status: 'succeeded',
  //       },
  //     },
  //   })
  //   expect(res.value).toEqual(123)
  //   expect(res.checks['gt_ten'].status).toEqual('succeeded')
  // })

  // TODO: checks as inputs are not supported yet
  // it('alias with multiple attrs', async () => {
  //   const res = await b.AliasWithMultipleAttrs(123)
  //   expect(res.value).toEqual(123)
  //   expect(res.checks['gt_ten'].status).toEqual('succeeded')
  // })

  it("simple recursive map alias", async () => {
    const arg = { one: { two: { three: {} } } };
    const res = await b.request.SimpleRecursiveMapAlias(arg);
    const json = res.body.json();
    expect(json.messages[0].content[0].text).toContain(`RecursiveMapAlias = map<string, RecursiveMapAlias>

Answer in JSON using this schema: RecursiveMapAlias`)

    const parsed = b.parse.SimpleRecursiveMapAlias(JSON.stringify(arg));
    expect(parsed).toEqual(arg);
  });

  it("simple recursive list alias", async () => {
    const arg = [[], [], [[]]];
    const res = await b.request.SimpleRecursiveListAlias(arg);
    const json = res.body.json();

    const parsed = b.parse.SimpleRecursiveListAlias(JSON.stringify(arg));
    expect(parsed).toEqual(arg);
  });

  it("recursive alias cycles", async () => {
    const data = [[], [], [[]]];
    const res = await b.request.RecursiveAliasCycle(data);
    const json = res.body.json();
    expect(json.messages[0].content[0].text).toContain("RecAliasOne = RecAliasTwo\nRecAliasTwo = RecAliasThree\nRecAliasThree = RecAliasOne[]\n\nAnswer in JSON using this schema: RecAliasOne")

    const parsed = b.parse.RecursiveAliasCycle(JSON.stringify(data));
    expect(parsed).toEqual(data);
  });

  it("json type alias cycle", async () => {
    const data = {
      number: 1,
      string: "test",
      bool: true,
      list: [1, 2, 3],
      object: { number: 1, string: "test", bool: true, list: [1, 2, 3] },
      json: {
        number: 1,
        string: "test",
        bool: true,
        list: [1, 2, 3],
        object: { number: 1, string: "test", bool: true, list: [1, 2, 3] },
      },
    };
    const res = await b.request.JsonTypeAliasCycle(data);
    const json = res.body.json();
    expect(json.messages[0].content[0].text).toContain(`
JsonValue = int or string or bool or float or JsonObject or JsonArray
JsonObject = map<string, JsonValue>
JsonArray = JsonValue[]

Answer in JSON using this schema: JsonValue`)

    const parsed = b.parse.JsonTypeAliasCycle(JSON.stringify(data));
    expect(parsed).toEqual(data);
  });

  it("json type alias as class dependency", async () => {
    const data = {
      number: 1,
      string: "test",
      bool: true,
      list: [1, 2, 3],
      object: { number: 1, string: "test", bool: true, list: [1, 2, 3] },
      json: {
        number: 1,
        string: "test",
        bool: true,
        list: [1, 2, 3],
        object: { number: 1, string: "test", bool: true, list: [1, 2, 3] },
      },
    };
    const res = await b.request.TakeRecAliasDep({ value: data });
    const json = res.body.json();
    expect(json.messages[0].content[0].text).toContain(`
JsonValue = int or string or bool or float or JsonObject or JsonArray
JsonObject = map<string, JsonValue>
JsonArray = JsonValue[]

Answer in JSON using this schema:
{
  value: JsonValue,
}`)

    const parsed = b.parse.TakeRecAliasDep(JSON.stringify({ value: data }));
    expect(parsed.value).toEqual(data);
  });
});
