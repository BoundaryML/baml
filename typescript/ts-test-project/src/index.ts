import { trace, initTracer, setTags, traceAsync } from "baml-client-lib";
initTracer();

const fx = trace((a: number, b: number) => {
  setTags({ "foo": 'bar' })
  return a + b;
}, 'add', [{ name: 'a', type: 'number' }, { name: 'b', type: 'number' }], false, 'number');

const callLLM = traceAsync(async () => {
  console.log("callLLM");
  return "callLLMResult";
}, 'callLLM', [{ name: "blah", type: "string" }], false, 'string');

async function blah() {
  callLLM();
}

blah();