class JsonUser { name string age int tags string[] manager int? }
class JsonAmount { cents int | float }
type JsonBranch = int | null | JsonBranch[]
function selector_error(doc:json, selector:string) -> string {
  baml.json.path<string>(doc,selector) catch(e) {
    baml.json.PathError => e.message + "|" + e.selector
  }
}
function main() -> int {
  let doc=baml.json.parse("{\"users\":[{\"name\":\"old\",\"name\":\"Ada\",\"age\":36,\"tags\":[\"a\",\"é\"],\"manager\":null}],\"é\":{\"x\":1},\"a.b\":\"line\\nend\",\"big\":4611686018427387904}");
  let user=baml.json.to<JsonUser>(baml.json.path<json[]>(doc,".users")[0]);
  let check=user.age+user.name.length()+user.tags.length()+ (user.manager ?? 7);
  check+=baml.json.path<int>(doc,".users[0].age");
  check+=baml.json.path<string>(doc,"['a.b']").length();
  check+=baml.json.stringify(baml.json.path<bigint>(doc,".big")).length();
  check+=baml.json.path<int>(doc,".é.x");
  let literal:json={"xs":[1,2],"text":"hello"};
  check+=baml.json.path<int>(literal,".xs[1]");
  check+=baml.json.path<string>(literal,".text").length();
  for(let selector in ["", "users", ".missing", ".users[-1]", ".users[0].age.x", ".users[0].age[0]", ".users[0].age", ".é['x']oops", "['unterminated", "[", ".users[bad]"]) {
    let message=selector_error(doc,selector);
    for(let c in message.to_code_points()){check=(check*31+c)%1000000007;}
  }
  let rows=baml.json.decode<JsonAmount[]>("[{\"cents\":1},{\"cents\":2.0},{\"cents\":3,\"cents\":4}]");
  for(let row in rows){check+=match(row.cents){let i:int=>i,let f:float=>f.floor().itrunc()};}
  let branch=baml.json.decode<JsonBranch[]>("[[1,null,[2]],3]");
  check+=baml.json.stringify(branch).length();
  check+=if "é".is_ascii(){1000}else{11};
  check+=if "hello".is_ascii(){13}else{1000};
  let int_error=(1e30).itrunc() catch(e){baml.errors.InvalidArgument=>e.message.length()};
  check+=int_error;
  check
}
