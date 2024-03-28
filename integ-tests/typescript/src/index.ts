import { LLMResponse, LLMResponseStream } from '@boundaryml/baml-core';
import b from "../baml_client";

interface Foo {
  bar: string;
}


class MinRepro<T> implements AsyncIterable<Partial<T>> {
  //[Symbol.asyncIterator] = async function* () {

  constructor(readonly words: T[]) {}
    
  [Symbol.asyncIterator](): AsyncIterator<Partial<T>> {

    const words = [...this.words];
    
    return {
      next: async (): Promise<IteratorResult<Partial<T>>> => {
        const word = words.shift();
        if (word === undefined) {
          return { value: undefined, done: true };
        }
        return { value: word, done: false };
      }
    }
  }
}

const testCompilation = async () => {

  async function* events(): AsyncGenerator<LLMResponse> {
    await new Promise((resolve) => setTimeout(resolve, 2000));
    yield {
      generated: "llm1",
      model_name: "model",
      meta: {},
    }
    await new Promise((resolve) => setTimeout(resolve, 2000));
    yield {
      generated: "llm2",
      model_name: "model",
      meta: {},
    }
    await new Promise((resolve) => setTimeout(resolve, 2000));
    yield {
      generated: "llm3",
      model_name: "model",
      meta: {},
    }
  }
  
  const llm = new LLMResponseStream<string | null[]>(events(), (partial: string) => null, (final: string) => final);

  console.log("testing compile");
  for await (const result of llm) {
    console.log(JSON.stringify(result, null, 2));
  }
};

const main = async () => {
  console.log("Hello, World!")

  const repro = new MinRepro<string>(["lorem", "ipsum"]);

  for await (const word of repro) {
    console.log(word);
  }

  //const result = await b.OptionalTest_Function.stream("Hello, World!");
  //const stream = b.OptionalTest_Function.getImpl("v1").stream("Hello, World!");
  const stream = b.OptionalTest_Function.stream("Hello, World!");

  console.log(stream);

  let i = 0;
  for await (const result of stream) {
    if (i++ > 5) break;
    if (result.is_parseable) {
      console.log(JSON.stringify(result.parsed, null, 2));
    }
  }

  const final = await stream.getFinalResponse();
  console.log(`final: ${JSON.stringify(final, null, 2)}`);
}
const nonstreamed = async () => {
  //const result = await b.OptionalTest_Function.stream("Hello, World!");
  const result = b.OptionalTest_Function.getImpl("v1").run("Hello, World!");
  console.log(JSON.stringify(result, null, 2));
}

if (require.main === module) {
  //nonstreamed();
  main();
}
