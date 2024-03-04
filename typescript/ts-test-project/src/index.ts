import { trace, setTags, traceAsync } from "baml-client-lib";

const fx = trace((a: number, b: number) => {
  setTags({ "foo": 'bar' })
  return a + b;
}, 'add', [{ name: 'a', type: 'number' }, { name: 'b', type: 'number' }], false, 'number');

const callLLM = traceAsync(async (blah: string) => {
  await new Promise((resolve) => setTimeout(resolve, 1000));
  console.log(blah);
  return "callLLMResult";
}, 'callLLM', [{ name: "blah", type: "string" }], false, 'string');

async function blah() {
  return callLLM("test-in");
}

blah().then((result) => {
  console.log(result);
});