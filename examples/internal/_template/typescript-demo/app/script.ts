import { b } from "./baml_client";

async function test() {
  const result = await b.request.ExtractResume("Vaibhav Gupta");
  console.log(result.body.json());
}

test().catch(console.error);